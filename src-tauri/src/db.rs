//! Local application database (spec §30). SQLite is compiled into the binary,
//! so the end user never installs a database engine.
//!
//! Nothing secret is ever written here. Credentials live in the OS keystore
//! (see vault.rs); this file only stores the *reference* to them.

use crate::error::AppResult;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

/// %APPDATA%\DevWorkstation on Windows, ~/.local/share/DevWorkstation elsewhere.
pub fn data_root() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
        })
    };
    base.unwrap_or_else(std::env::temp_dir).join("DevWorkstation")
}

/// Directory layout from spec §30.
pub fn ensure_layout() -> AppResult<PathBuf> {
    let root = data_root();
    for sub in [
        "application", "resources", "engines", "templates",
        "projects", "cache", "logs", "temp", "database",
    ] {
        std::fs::create_dir_all(root.join(sub))?;
    }
    Ok(root)
}

pub fn open() -> AppResult<Connection> {
    let root = ensure_layout()?;
    let path = root.join("database").join("workstation.sqlite3");
    let conn = Connection::open(&path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);

        CREATE TABLE IF NOT EXISTS users (
            id            INTEGER PRIMARY KEY,
            username      TEXT NOT NULL UNIQUE,
            display_name  TEXT NOT NULL DEFAULT '',
            password_hash TEXT NOT NULL,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            last_login_at TEXT
        );

        -- One row (id = 1): the activation key this computer is registered
        -- with, and the status the licensing server last reported for it.
        CREATE TABLE IF NOT EXISTS license (
            id                INTEGER PRIMARY KEY CHECK (id = 1),
            activation_key    TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'unverified',
            last_verified_at  TEXT,
            created_at        TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key        TEXT PRIMARY KEY,
            value      TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS projects (
            id          INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            path        TEXT NOT NULL,
            technology  TEXT NOT NULL DEFAULT '',
            repository  TEXT NOT NULL DEFAULT '',
            server_id   INTEGER REFERENCES servers(id) ON DELETE SET NULL,
            database_id INTEGER REFERENCES databases(id) ON DELETE SET NULL,
            notes       TEXT NOT NULL DEFAULT '',
            status      TEXT NOT NULL DEFAULT 'development',
            last_scan   TEXT,
            created_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS servers (
            id          INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            host        TEXT NOT NULL,
            port        INTEGER NOT NULL DEFAULT 22,
            username    TEXT NOT NULL,
            auth_kind   TEXT NOT NULL DEFAULT 'password',
            key_path    TEXT NOT NULL DEFAULT '',
            vault_ref   TEXT NOT NULL DEFAULT '',
            kind        TEXT NOT NULL DEFAULT 'ssh',
            created_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS databases (
            id          INTEGER PRIMARY KEY,
            name        TEXT NOT NULL,
            engine      TEXT NOT NULL,
            host        TEXT NOT NULL DEFAULT '',
            port        INTEGER NOT NULL DEFAULT 0,
            dbname      TEXT NOT NULL DEFAULT '',
            username    TEXT NOT NULL DEFAULT '',
            file_path   TEXT NOT NULL DEFAULT '',
            vault_ref   TEXT NOT NULL DEFAULT '',
            created_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS cvs (
            id         INTEGER PRIMARY KEY,
            name       TEXT NOT NULL,
            document   TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS logs (
            id      INTEGER PRIMARY KEY,
            at      TEXT NOT NULL,
            level   TEXT NOT NULL,
            module  TEXT NOT NULL,
            message TEXT NOT NULL,
            detail  TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_logs_at ON logs(at DESC);
        CREATE INDEX IF NOT EXISTS idx_logs_module ON logs(module);

        CREATE TABLE IF NOT EXISTS activity (
            id      INTEGER PRIMARY KEY,
            at      TEXT NOT NULL,
            kind    TEXT NOT NULL,
            label   TEXT NOT NULL,
            target  TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS reports (
            id         INTEGER PRIMARY KEY,
            module     TEXT NOT NULL,
            title      TEXT NOT NULL,
            payload    TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )?;

    let v: i64 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))
        .unwrap_or(0);
    if v < 1 {
        conn.execute("INSERT INTO schema_version (version) VALUES (1)", [])?;
    }
    Ok(())
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, crate::now()],
    )?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .ok()
}

/// Guards against path-traversal style surprises when a command takes a path.
pub fn is_readable_path(p: &Path) -> bool {
    p.exists()
}
