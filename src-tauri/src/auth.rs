//! Local application login.
//!
//! The password is never stored. Only an Argon2id hash (with a per-user random
//! salt) is written to the local SQLite database. The hash cannot be reversed,
//! and it is never logged, exported or sent anywhere.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use password_hash::{rand_core::OsRng, SaltString};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Mutex;
use tauri::State;

/// In-memory session. Nothing about the session is persisted, so closing the
/// app always signs you out.
#[derive(Default)]
pub struct Session(pub Mutex<Option<SessionUser>>);

#[derive(Clone, Serialize)]
pub struct SessionUser {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub password_changed: bool,
}

pub fn hash_password(plain: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            AppError::new("PASSWORD_HASH_FAILED", "The password could not be secured.")
                .detail(e.to_string())
        })
}

fn verify_password(plain: &str, stored: &str) -> bool {
    match PasswordHash::new(stored) {
        Ok(parsed) => Argon2::default()
            .verify_password(plain.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// True once a local account has been created on this computer (spec: first
/// run shows Register, every run after that shows Sign in).
#[tauri::command]
pub fn has_account(db: State<'_, Db>) -> AppResult<bool> {
    let conn = db.0.lock().unwrap();
    let existing: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
    Ok(existing > 0)
}

/// First-run registration: creates the one local account this computer will
/// have, but only after the activation key has been confirmed "paid" by the
/// licensing server. This step requires internet — there is nothing local to
/// fall back to yet. On success the account and the activation key are both
/// written to the local database and the caller is signed in immediately.
#[tauri::command]
pub async fn register(
    db: State<'_, Db>,
    session: State<'_, Session>,
    username: String,
    password: String,
    activation_key: String,
) -> AppResult<SessionUser> {
    let username = username.trim().to_string();
    let activation_key = activation_key.trim().to_string();

    if username.len() < 3 {
        return Err(AppError::new("USERNAME_TOO_SHORT", "Use at least 3 characters for the username."));
    }
    if password.chars().count() < 10 {
        return Err(AppError::new("PASSWORD_TOO_SHORT", "Use at least 10 characters.")
            .recover("A passphrase of three or four unrelated words is easy to remember and hard to guess."));
    }
    if activation_key.is_empty() {
        return Err(AppError::new(
            "ACTIVATION_KEY_REQUIRED",
            "Enter the activation key you received when you purchased DevWorkstation.",
        ));
    }

    {
        let conn = db.0.lock().unwrap();
        let existing: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        if existing > 0 {
            return Err(AppError::new(
                "ALREADY_REGISTERED",
                "This computer already has a DevWorkstation account.",
            )
            .recover("Sign in instead."));
        }
    }

    // Registration is not allowed to fall back to "offline, try later" — the
    // spec requires internet on first run so the key can be checked once.
    let (valid, status) = crate::license::verify_remote(&activation_key).await.map_err(|e| {
        AppError::new(
            "NO_INTERNET",
            "Setting up DevWorkstation for the first time needs an internet connection, to verify your activation key.",
        )
        .detail(e.to_string())
        .recover("Connect to the internet and try again.")
    })?;

    if !valid {
        let conn = db.0.lock().unwrap();
        logging::write(
            &conn, "warn", "AUTH", "Registration rejected: activation key not paid",
            Some(&format!("status={status}")),
        );
        return Err(AppError::new(
            "ACTIVATION_INVALID",
            "That activation key isn't valid or doesn't show as paid.",
        )
        .detail(format!("server status: {status}"))
        .recover("Opening the activation page so you can purchase a key."));
    }

    let hash = hash_password(&password)?;
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO users (username, display_name, password_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![username, username, hash, crate::now()],
    )
    .map_err(|_| AppError::new("USERNAME_TAKEN", "That username is already in use on this computer."))?;
    let id = conn.last_insert_rowid();

    // The user picked this password themselves, so there's nothing to nag about.
    crate::db::set_setting(&conn, "password_changed", "true")?;
    crate::license::save_license(&conn, &activation_key, &status)?;
    logging::write(&conn, "info", "AUTH", "Account registered", Some(&format!("user={username}")));

    let user = SessionUser {
        id,
        username: username.clone(),
        display_name: username,
        password_changed: true,
    };
    *session.0.lock().unwrap() = Some(user.clone());
    Ok(user)
}

fn password_changed(conn: &Connection) -> bool {
    crate::db::get_setting(conn, "password_changed").as_deref() == Some("true")
}

#[tauri::command]
pub async fn sign_in(
    db: State<'_, Db>,
    session: State<'_, Session>,
    username: String,
    password: String,
) -> AppResult<SessionUser> {
    // Password check first, with the DB lock dropped before we ever hit the
    // network (a MutexGuard can't be held across an .await).
    let (id, uname, display) = {
        let conn = db.0.lock().unwrap();

        let row: Option<(i64, String, String, String)> = conn
            .query_row(
                "SELECT id, username, display_name, password_hash FROM users WHERE username = ?1 COLLATE NOCASE",
                params![username.trim()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .ok();

        // Same error for unknown user and wrong password, so the login screen can't
        // be used to discover which usernames exist.
        let reject = || {
            AppError::new("SIGN_IN_REJECTED", "That username and password don't match.")
                .recover("Check for caps lock. The account was created the first time DevWorkstation was set up.")
        };

        let (id, uname, display, hash) = row.ok_or_else(reject)?;
        if !verify_password(&password, &hash) {
            logging::write(&conn, "warn", "AUTH", "Sign-in rejected", Some(&format!("user={}", uname)));
            return Err(reject());
        }
        (id, uname, display)
    };

    // Activation check: only when this gadget has internet right now. Offline
    // sign-ins proceed straight to the dashboard, unverified, by design.
    if crate::system::is_online() {
        let key = {
            let conn = db.0.lock().unwrap();
            crate::license::load_license(&conn).map(|(k, _)| k)
        };

        if let Some(key) = key {
            if let Ok((valid, status)) = crate::license::verify_remote(&key).await {
                let conn = db.0.lock().unwrap();
                let _ = crate::license::save_license(&conn, &key, &status);
                if !valid {
                    logging::write(
                        &conn, "warn", "AUTH", "Sign-in blocked: activation key not paid",
                        Some(&format!("user={uname} status={status}")),
                    );
                    return Err(AppError::new(
                        "ACTIVATION_INVALID",
                        "Your activation key isn't valid or doesn't show as paid.",
                    )
                    .detail(format!("server status: {status}"))
                    .recover("Opening the activation page so you can purchase a key."));
                }
            }
            // A network hiccup that stops the *license check itself* (server
            // down, timeout) doesn't lock a paying user out — that's treated
            // the same as an offline sign-in below.
        }
    }

    let conn = db.0.lock().unwrap();
    conn.execute("UPDATE users SET last_login_at = ?1 WHERE id = ?2", params![crate::now(), id])?;
    logging::write(&conn, "info", "AUTH", "Signed in", Some(&format!("user={}", uname)));

    let user = SessionUser {
        id,
        username: uname,
        display_name: display,
        password_changed: password_changed(&conn),
    };
    *session.0.lock().unwrap() = Some(user.clone());
    Ok(user)
}

#[tauri::command]
pub fn sign_out(session: State<'_, Session>) -> AppResult<()> {
    *session.0.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub fn current_user(session: State<'_, Session>) -> Option<SessionUser> {
    session.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn change_password(
    db: State<'_, Db>,
    session: State<'_, Session>,
    current_password: String,
    new_password: String,
) -> AppResult<()> {
    let me = session
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::new("NOT_SIGNED_IN", "Sign in before changing the password."))?;

    if new_password.chars().count() < 10 {
        return Err(AppError::new(
            "PASSWORD_TOO_SHORT",
            "Use at least 10 characters.",
        )
        .recover("A passphrase of three or four unrelated words is easy to remember and hard to guess."));
    }
    if new_password == current_password {
        return Err(AppError::new("PASSWORD_UNCHANGED", "The new password is the same as the old one."));
    }

    let conn = db.0.lock().unwrap();
    let stored: String = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        params![me.id],
        |r| r.get(0),
    )?;
    if !verify_password(&current_password, &stored) {
        return Err(AppError::new("CURRENT_PASSWORD_WRONG", "The current password is not correct."));
    }

    let hash = hash_password(&new_password)?;
    conn.execute(
        "UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3",
        params![hash, crate::now(), me.id],
    )?;
    crate::db::set_setting(&conn, "password_changed", "true")?;
    logging::write(&conn, "info", "AUTH", "Password changed", None);

    if let Some(s) = session.0.lock().unwrap().as_mut() {
        s.password_changed = true;
    }
    Ok(())
}

#[tauri::command]
pub fn change_username(
    db: State<'_, Db>,
    session: State<'_, Session>,
    password: String,
    new_username: String,
    display_name: String,
) -> AppResult<SessionUser> {
    let me = session
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::new("NOT_SIGNED_IN", "Sign in before changing your account."))?;

    let new_username = new_username.trim().to_string();
    if new_username.len() < 3 {
        return Err(AppError::new("USERNAME_TOO_SHORT", "Use at least 3 characters."));
    }

    let conn = db.0.lock().unwrap();
    let stored: String = conn.query_row(
        "SELECT password_hash FROM users WHERE id = ?1",
        params![me.id],
        |r| r.get(0),
    )?;
    if !verify_password(&password, &stored) {
        return Err(AppError::new("CURRENT_PASSWORD_WRONG", "The password is not correct."));
    }

    conn.execute(
        "UPDATE users SET username = ?1, display_name = ?2, updated_at = ?3 WHERE id = ?4",
        params![new_username, display_name.trim(), crate::now(), me.id],
    )
    .map_err(|_| {
        AppError::new("USERNAME_TAKEN", "That username is already in use on this computer.")
    })?;
    logging::write(&conn, "info", "AUTH", "Account details updated", None);

    let updated = SessionUser {
        id: me.id,
        username: new_username,
        display_name: display_name.trim().to_string(),
        password_changed: password_changed(&conn),
    };
    *session.0.lock().unwrap() = Some(updated.clone());
    Ok(updated)
}

/// Every module command calls this first, so a module cannot be driven
/// without a signed-in session.
pub fn require_session(session: &State<'_, Session>) -> AppResult<SessionUser> {
    session
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::new("NOT_SIGNED_IN", "Your session ended. Sign in again."))
}
