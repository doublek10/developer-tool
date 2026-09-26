//! Activation key storage + remote verification.
//!
//! The activation key the user enters at first-run registration is stored
//! locally, next to the local account (see the `license` table in db.rs — a
//! single row, since one computer has one key). It is never sent anywhere
//! except to the verification endpoint below, over HTTPS.
//!
//! - First-time registration (auth::register) REQUIRES a successful check.
//! - Every later sign-in (auth::sign_in) re-checks the key only if this
//!   computer is online right now; an offline sign-in proceeds unverified.

use crate::db::Db;
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use sysinfo::System;
use tauri::State;

/// Verification endpoint. Expects `{"activation_key": "...", "machine_info": "..."}`
/// as JSON and answers with JSON describing whether the key exists, its paid
/// status, and (device-binding) whether this machine is the one it's
/// registered to (see the accompanying verify.php for the exact contract).
const VERIFY_URL: &str = "https://api.wonderbizz.top/v2/verify.php";

/// Where a user with no valid key — or whose key is already bound to a
/// different machine — is sent to buy or fix one.
pub const PURCHASE_URL: &str = "https://devworkstation-website.vercel.app/activationkey";

#[derive(Serialize, Clone)]
pub struct LicenseInfo {
    pub activation_key: String,
    pub status: String,
    pub last_verified_at: Option<String>,
}

#[derive(Deserialize, Default)]
struct VerifyResponse {
    #[serde(default)]
    valid: bool,
    /// e.g. "paid", "unpaid", "not_found", "device_mismatch"
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// A stable-enough fingerprint for "this computer": the OS-native machine ID
/// (Windows: the registry MachineGuid; macOS: IOPlatformUUID; Linux:
/// /etc/machine-id) plus hostname and OS version for a human to recognize
/// in a support conversation. This is what binds one activation key to one
/// machine — see zerotrust in verify.php.
fn machine_fingerprint() -> String {
    let id = machine_uid::get().unwrap_or_else(|_| "unknown-machine".to_string());
    let host = System::host_name().unwrap_or_else(|| "unknown-host".to_string());
    let os = System::long_os_version().unwrap_or_else(|| "unknown-os".to_string());
    serde_json::json!({ "machine_id": id, "hostname": host, "os": os }).to_string()
}

/// Calls the licensing API for one key, from this machine. Returns
/// `(is_paid_and_bound_to_this_machine, raw_status, message_for_the_user)`.
/// An `Err` here means the *check itself* failed (no network, bad response) —
/// it says nothing about whether the key is good.
pub async fn verify_remote(activation_key: &str) -> AppResult<(bool, String, String)> {
    let machine_info = machine_fingerprint();

    let client = reqwest::Client::builder()
        .user_agent("DevWorkstation/0.1 (license check)")
        .timeout(Duration::from_secs(12))
        .build()?;

    let resp = client
        .post(VERIFY_URL)
        .json(&serde_json::json!({
            "activation_key": activation_key,
            "machine_info": machine_info,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(AppError::new(
            "LICENSE_SERVER_ERROR",
            "The activation server did not respond correctly.",
        )
        .detail(format!("HTTP {}", resp.status()))
        .recover("Try again in a moment."));
    }

    let body: VerifyResponse = resp.json().await.map_err(|e| {
        AppError::new(
            "LICENSE_SERVER_ERROR",
            "The activation server sent an unexpected response.",
        )
        .detail(e.to_string())
    })?;

    let status = body.status.unwrap_or_else(|| "unknown".to_string());
    let message = body.message.unwrap_or_else(|| {
        if body.valid {
            "Activation key is valid.".to_string()
        } else {
            "That activation key isn't valid or doesn't show as paid.".to_string()
        }
    });
    Ok((body.valid, status, message))
}

/// Saves (or overwrites) the one activation key this computer knows about,
/// along with the status the server most recently reported.
pub fn save_license(conn: &Connection, activation_key: &str, status: &str) -> AppResult<()> {
    let now = crate::now();
    conn.execute(
        "INSERT INTO license (id, activation_key, status, last_verified_at, created_at)
         VALUES (1, ?1, ?2, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            activation_key   = excluded.activation_key,
            status           = excluded.status,
            last_verified_at = excluded.last_verified_at",
        params![activation_key, status, now],
    )?;
    Ok(())
}

/// Returns `(activation_key, status)` for this computer, if one has ever been saved.
pub fn load_license(conn: &Connection) -> Option<(String, String)> {
    conn.query_row(
        "SELECT activation_key, status FROM license WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .ok()
}

/// Read-only view for the UI (e.g. Settings), so it can show "Licensed —
/// last checked 2026-09-24 10:02" without exposing the key elsewhere.
#[tauri::command]
pub fn license_status(db: State<'_, Db>) -> Option<LicenseInfo> {
    let conn = db.0.lock().unwrap();
    conn.query_row(
        "SELECT activation_key, status, last_verified_at FROM license WHERE id = 1",
        [],
        |r| {
            Ok(LicenseInfo {
                activation_key: r.get(0)?,
                status: r.get(1)?,
                last_verified_at: r.get(2)?,
            })
        },
    )
    .ok()
}

