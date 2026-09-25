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
use tauri::State;

/// Verification endpoint. Expects `{"activation_key": "..."}` as JSON and
/// answers with JSON describing whether the key exists and its paid status
/// (see the accompanying verify.php for the exact contract).
const VERIFY_URL: &str = "https://api.wonderbizz.top/v2/verify.php";

/// Where a user with no valid key is sent to buy or renew one.
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
    /// e.g. "paid", "unpaid", "expired", "not_found"
    #[serde(default)]
    status: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    message: Option<String>,
}

/// Calls the licensing API for one key. Returns `(is_paid_and_valid, raw_status)`.
/// A `Err` here means the *check itself* failed (no network, bad response) —
/// it says nothing about whether the key is good.
pub async fn verify_remote(activation_key: &str) -> AppResult<(bool, String)> {
    let client = reqwest::Client::builder()
        .user_agent("DevWorkstation/0.1 (license check)")
        .timeout(Duration::from_secs(12))
        .build()?;

    let resp = client
        .post(VERIFY_URL)
        .json(&serde_json::json!({ "activation_key": activation_key }))
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
    Ok((body.valid && status == "paid", status))
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
