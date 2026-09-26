//! Secure credential vault (spec §18).
//!
//! Secrets go to Windows Credential Manager (or Keychain / libsecret). SQLite
//! only ever holds the *label* of an entry, never its value. A secret leaves
//! the vault in exactly one direction: into the module that needs it, at the
//! moment it is used. There is no command that returns a stored secret to the
//! user interface.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use tauri::State;

const SERVICE: &str = "DevWorkstation";

#[derive(Serialize, Deserialize, Clone)]
pub struct VaultItem {
    /// Stable key, e.g. "server:3:password" or "api:openai".
    pub reference: String,
    pub label: String,
    pub kind: String, // ssh | cpanel | api | database | token | website
    pub username: String,
}

fn index(conn: &rusqlite::Connection) -> AppResult<Vec<VaultItem>> {
    let raw = crate::db::get_setting(conn, "vault_index").unwrap_or_else(|| "[]".into());
    Ok(serde_json::from_str(&raw)?)
}

fn save_index(conn: &rusqlite::Connection, items: &[VaultItem]) -> AppResult<()> {
    crate::db::set_setting(conn, "vault_index", &serde_json::to_string(items)?)
}

/// Used by Server Center / Database Lab at the moment of connection.
pub fn read_secret(reference: &str) -> AppResult<String> {
    let entry = Entry::new(SERVICE, reference)?;
    entry.get_password().map_err(|e| {
        AppError::new("VAULT_ENTRY_MISSING", "That credential is not in the vault.")
            .detail(e.to_string())
            .recover("Open Vault and save the credential again.")
    })
}

#[tauri::command]
pub fn vault_list(db: State<'_, Db>, session: State<'_, Session>) -> AppResult<Vec<VaultItem>> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    index(&conn)
}

#[tauri::command]
pub fn vault_store(
    db: State<'_, Db>,
    session: State<'_, Session>,
    reference: String,
    label: String,
    kind: String,
    username: String,
    secret: String,
) -> AppResult<()> {
    require_session(&session)?;
    if secret.is_empty() {
        return Err(AppError::new("VAULT_EMPTY_SECRET", "Enter the value you want to store."));
    }

    let entry = Entry::new(SERVICE, &reference)?;
    entry.set_password(&secret)?;

    let conn = db.0.lock().unwrap();
    let mut items = index(&conn)?;
    items.retain(|i| i.reference != reference);
    items.push(VaultItem { reference: reference.clone(), label, kind, username });
    save_index(&conn, &items)?;

    // Note: the secret itself is not in this message, and redact() would strip it anyway.
    logging::write(&conn, "info", "VAULT", "Credential saved", Some(&format!("ref={reference}")));
    Ok(())
}

#[tauri::command]
pub fn vault_delete(
    db: State<'_, Db>,
    session: State<'_, Session>,
    reference: String,
) -> AppResult<()> {
    require_session(&session)?;
    if let Ok(entry) = Entry::new(SERVICE, &reference) {
        let _ = entry.delete_credential();
    }
    let conn = db.0.lock().unwrap();
    let mut items = index(&conn)?;
    items.retain(|i| i.reference != reference);
    save_index(&conn, &items)?;
    logging::write(&conn, "info", "VAULT", "Credential removed", Some(&format!("ref={reference}")));
    Ok(())
}

/// Confirms an entry is present and readable without ever returning its value.
#[tauri::command]
pub fn vault_verify(session: State<'_, Session>, reference: String) -> AppResult<bool> {
    require_session(&session)?;
    Ok(read_secret(&reference).is_ok())
}
