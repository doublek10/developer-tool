//! Project, server and database profiles (spec §9, §17).
//!
//! Server and database rows hold a `vault_ref` only. The secret itself lives in
//! the OS keystore and is fetched at the moment a connection is opened.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Serialize, Deserialize)]
pub struct Project {
    #[serde(default)] pub id: Option<i64>,
    pub name: String,
    pub path: String,
    #[serde(default)] pub technology: String,
    #[serde(default)] pub repository: String,
    #[serde(default)] pub server_id: Option<i64>,
    #[serde(default)] pub database_id: Option<i64>,
    #[serde(default)] pub notes: String,
    #[serde(default = "dev")] pub status: String,
    #[serde(default)] pub last_scan: Option<String>,
}
fn dev() -> String { "development".into() }

#[derive(Serialize, Deserialize)]
pub struct Server {
    #[serde(default)] pub id: Option<i64>,
    pub name: String,
    pub host: String,
    #[serde(default = "p22")] pub port: i64,
    pub username: String,
    #[serde(default = "pw")] pub auth_kind: String, // password | key
    #[serde(default)] pub key_path: String,
    #[serde(default)] pub vault_ref: String,
    #[serde(default = "ssh")] pub kind: String, // ssh | cpanel
}
fn p22() -> i64 { 22 }
fn pw() -> String { "password".into() }
fn ssh() -> String { "ssh".into() }

#[derive(Serialize, Deserialize)]
pub struct DatabaseProfile {
    #[serde(default)] pub id: Option<i64>,
    pub name: String,
    pub engine: String, // sqlite | postgres | mysql | mariadb | sqlserver
    #[serde(default)] pub host: String,
    #[serde(default)] pub port: i64,
    #[serde(default)] pub dbname: String,
    #[serde(default)] pub username: String,
    #[serde(default)] pub file_path: String,
    #[serde(default)] pub vault_ref: String,
}

#[tauri::command]
pub fn projects_list(db: State<'_, Db>, session: State<'_, Session>) -> AppResult<Vec<Project>> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, path, technology, repository, server_id, database_id, notes, status, last_scan
         FROM projects ORDER BY COALESCE(last_scan, created_at) DESC")?;
    let rows = stmt.query_map([], |r| Ok(Project {
        id: r.get(0)?, name: r.get(1)?, path: r.get(2)?, technology: r.get(3)?,
        repository: r.get(4)?, server_id: r.get(5)?, database_id: r.get(6)?,
        notes: r.get(7)?, status: r.get(8)?, last_scan: r.get(9)?,
    }))?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn project_save(db: State<'_, Db>, session: State<'_, Session>, project: Project) -> AppResult<i64> {
    require_session(&session)?;
    if project.name.trim().is_empty() {
        return Err(AppError::new("PROJECT_NAME_REQUIRED", "Give the project a name."));
    }
    let conn = db.0.lock().unwrap();
    match project.id {
        Some(id) => {
            conn.execute(
                "UPDATE projects SET name=?1, path=?2, technology=?3, repository=?4, server_id=?5,
                 database_id=?6, notes=?7, status=?8 WHERE id=?9",
                params![project.name, project.path, project.technology, project.repository,
                        project.server_id, project.database_id, project.notes, project.status, id])?;
            Ok(id)
        }
        None => {
            conn.execute(
                "INSERT INTO projects (name, path, technology, repository, server_id, database_id, notes, status, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![project.name, project.path, project.technology, project.repository,
                        project.server_id, project.database_id, project.notes, project.status, crate::now()])?;
            let id = conn.last_insert_rowid();
            logging::write(&conn, "info", "PROJECTS", "Project added", Some(&project.name));
            Ok(id)
        }
    }
}

#[tauri::command]
pub fn project_delete(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> AppResult<()> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    Ok(())
}

#[tauri::command]
pub fn servers_list(db: State<'_, Db>, session: State<'_, Session>) -> AppResult<Vec<Server>> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, host, port, username, auth_kind, key_path, vault_ref, kind FROM servers ORDER BY name")?;
    let rows = stmt.query_map([], |r| Ok(Server {
        id: r.get(0)?, name: r.get(1)?, host: r.get(2)?, port: r.get(3)?, username: r.get(4)?,
        auth_kind: r.get(5)?, key_path: r.get(6)?, vault_ref: r.get(7)?, kind: r.get(8)?,
    }))?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn server_save(db: State<'_, Db>, session: State<'_, Session>, server: Server) -> AppResult<i64> {
    require_session(&session)?;
    if server.host.trim().is_empty() {
        return Err(AppError::new("SERVER_HOST_REQUIRED", "Enter the address of the server."));
    }
    let conn = db.0.lock().unwrap();
    match server.id {
        Some(id) => {
            conn.execute(
                "UPDATE servers SET name=?1, host=?2, port=?3, username=?4, auth_kind=?5, key_path=?6, vault_ref=?7, kind=?8 WHERE id=?9",
                params![server.name, server.host, server.port, server.username, server.auth_kind,
                        server.key_path, server.vault_ref, server.kind, id])?;
            Ok(id)
        }
        None => {
            conn.execute(
                "INSERT INTO servers (name, host, port, username, auth_kind, key_path, vault_ref, kind, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![server.name, server.host, server.port, server.username, server.auth_kind,
                        server.key_path, server.vault_ref, server.kind, crate::now()])?;
            let id = conn.last_insert_rowid();
            logging::write(&conn, "info", "SERVERS", "Server profile added", Some(&server.name));
            Ok(id)
        }
    }
}

#[tauri::command]
pub fn server_delete(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> AppResult<()> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM servers WHERE id = ?1", params![id])?;
    Ok(())
}

/// Confirms the machine is reachable on the configured port. Deliberately does
/// not authenticate: opening a session is a separate, explicit action.
#[tauri::command]
pub fn server_reachable(db: State<'_, Db>, session: State<'_, Session>, host: String, port: u16) -> AppResult<u128> {
    require_session(&session)?;
    use std::net::ToSocketAddrs;
    let start = std::time::Instant::now();
    let addr = format!("{host}:{port}").to_socket_addrs()
        .map_err(|e| AppError::new("DNS_RESOLUTION_FAILED", format!("{host} could not be resolved."))
            .detail(e.to_string()))?
        .next()
        .ok_or_else(|| AppError::new("NO_ADDRESS", "No address was returned for that host."))?;
    std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(6))
        .map_err(|e| AppError::new("SERVER_UNREACHABLE", format!("Nothing answered on {host}:{port}."))
            .detail(e.to_string())
            .recover("Check the host and port, whether the machine is powered on, and any firewall rules."))?;
    let ms = start.elapsed().as_millis();
    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "SERVERS", "Reachability check", Some(&format!("host={host} port={port} ms={ms}")));
    Ok(ms)
}

#[tauri::command]
pub fn databases_list(db: State<'_, Db>, session: State<'_, Session>) -> AppResult<Vec<DatabaseProfile>> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare(
        "SELECT id, name, engine, host, port, dbname, username, file_path, vault_ref FROM databases ORDER BY name")?;
    let rows = stmt.query_map([], |r| Ok(DatabaseProfile {
        id: r.get(0)?, name: r.get(1)?, engine: r.get(2)?, host: r.get(3)?, port: r.get(4)?,
        dbname: r.get(5)?, username: r.get(6)?, file_path: r.get(7)?, vault_ref: r.get(8)?,
    }))?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn database_save(db: State<'_, Db>, session: State<'_, Session>, profile: DatabaseProfile) -> AppResult<i64> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    match profile.id {
        Some(id) => {
            conn.execute(
                "UPDATE databases SET name=?1, engine=?2, host=?3, port=?4, dbname=?5, username=?6, file_path=?7, vault_ref=?8 WHERE id=?9",
                params![profile.name, profile.engine, profile.host, profile.port, profile.dbname,
                        profile.username, profile.file_path, profile.vault_ref, id])?;
            Ok(id)
        }
        None => {
            conn.execute(
                "INSERT INTO databases (name, engine, host, port, dbname, username, file_path, vault_ref, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![profile.name, profile.engine, profile.host, profile.port, profile.dbname,
                        profile.username, profile.file_path, profile.vault_ref, crate::now()])?;
            Ok(conn.last_insert_rowid())
        }
    }
}

#[tauri::command]
pub fn database_delete(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> AppResult<()> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM databases WHERE id = ?1", params![id])?;
    Ok(())
}
