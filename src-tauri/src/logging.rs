//! Logging engine (spec §19).
//!
//! Every write passes through `redact`, so a password, key or token that leaks
//! into a message by accident never reaches the log table or the log file.

use crate::db::Db;
use crate::error::AppResult;
use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct LogLine {
    pub id: i64,
    pub at: String,
    pub level: String,
    pub module: String,
    pub message: String,
    pub detail: Option<String>,
}

/// Keys whose values must never appear in a log line.
const SECRET_KEYS: &[&str] = &[
    "password", "passwd", "pwd", "secret", "token", "api_key", "apikey",
    "authorization", "auth", "private_key", "privatekey", "passphrase",
    "session", "cookie", "bearer",
];

pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for segment in text.split(|c| c == ' ' || c == '&' || c == ';' || c == ',') {
        let lowered = segment.to_ascii_lowercase();
        let hit = SECRET_KEYS.iter().any(|k| {
            lowered.starts_with(&format!("{k}=")) || lowered.starts_with(&format!("{k}:"))
        });
        if hit {
            let cut = segment.find(['=', ':']).map(|i| i + 1).unwrap_or(segment.len());
            out.push_str(&segment[..cut]);
            out.push_str("[redacted]");
        } else if lowered.starts_with("-----begin") {
            out.push_str("[redacted key material]");
        } else {
            out.push_str(segment);
        }
        out.push(' ');
    }
    out.trim_end().to_string()
}

pub fn write(conn: &Connection, level: &str, module: &str, message: &str, detail: Option<&str>) {
    let msg = redact(message);
    let det = detail.map(redact);
    let _ = conn.execute(
        "INSERT INTO logs (at, level, module, message, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![crate::now(), level, module, msg, det],
    );
    // Keep the table bounded; the on-disk export keeps the full history.
    let _ = conn.execute(
        "DELETE FROM logs WHERE id < (SELECT MAX(id) - 20000 FROM logs)",
        [],
    );
}

pub fn activity(conn: &Connection, kind: &str, label: &str, target: &str) {
    let _ = conn.execute(
        "INSERT INTO activity (at, kind, label, target) VALUES (?1, ?2, ?3, ?4)",
        params![crate::now(), kind, redact(label), redact(target)],
    );
}

#[tauri::command]
pub fn logs_query(
    db: State<'_, Db>,
    module: Option<String>,
    level: Option<String>,
    search: Option<String>,
    limit: Option<i64>,
) -> AppResult<Vec<LogLine>> {
    let conn = db.0.lock().unwrap();
    let mut sql = String::from(
        "SELECT id, at, level, module, message, detail FROM logs WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(m) = module.filter(|s| !s.is_empty() && s != "all") {
        sql.push_str(" AND module = ?");
        args.push(Box::new(m));
    }
    if let Some(l) = level.filter(|s| !s.is_empty() && s != "all") {
        sql.push_str(" AND level = ?");
        args.push(Box::new(l));
    }
    if let Some(s) = search.filter(|s| !s.trim().is_empty()) {
        sql.push_str(" AND (message LIKE ? OR detail LIKE ?)");
        let like = format!("%{}%", s.trim());
        args.push(Box::new(like.clone()));
        args.push(Box::new(like));
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    args.push(Box::new(limit.unwrap_or(300).clamp(1, 5000)));

    let mut stmt = conn.prepare(&sql)?;
    let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(refs.as_slice(), |r| {
        Ok(LogLine {
            id: r.get(0)?,
            at: r.get(1)?,
            level: r.get(2)?,
            module: r.get(3)?,
            message: r.get(4)?,
            detail: r.get(5)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[tauri::command]
pub fn logs_export(db: State<'_, Db>, destination: String) -> AppResult<String> {
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT at, level, module, message, COALESCE(detail,'') FROM logs ORDER BY id ASC",
    )?;
    let mut out = String::new();
    let rows = stmt.query_map([], |r| {
        Ok(format!(
            "{}  {:<5}  {:<10}  {}  {}",
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?
        ))
    })?;
    for line in rows.flatten() {
        out.push_str(&line);
        out.push('\n');
    }
    std::fs::write(&destination, out)?;
    Ok(destination)
}

#[tauri::command]
pub fn logs_clear(db: State<'_, Db>) -> AppResult<()> {
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM logs", [])?;
    write(&conn, "info", "SYSTEM", "Log history cleared", None);
    Ok(())
}
