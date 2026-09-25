//! Dashboard data, connectivity state and report export (spec §5, §20, §27).

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::AppResult;
use serde::Serialize;
use sysinfo::System;
use tauri::State;

#[derive(Serialize)]
pub struct Resources {
    pub cpu_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub host: String,
    pub os: String,
}

#[derive(Serialize)]
pub struct ActivityRow {
    pub at: String,
    pub kind: String,
    pub label: String,
    pub target: String,
}

#[derive(Serialize)]
pub struct DashboardData {
    pub resources: Resources,
    pub activity: Vec<ActivityRow>,
    pub recent_errors: Vec<crate::logging::LogLine>,
    pub project_count: i64,
    pub server_count: i64,
    pub cv_count: i64,
    pub online: bool,
    pub data_folder: String,
}

#[tauri::command]
pub fn system_resources() -> Resources {
    let mut sys = System::new_all();
    sys.refresh_cpu_usage();
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    Resources {
        cpu_percent: sys.global_cpu_usage(),
        memory_used_mb: sys.used_memory() / 1_048_576,
        memory_total_mb: sys.total_memory() / 1_048_576,
        host: System::host_name().unwrap_or_else(|| "this computer".into()),
        os: System::long_os_version().unwrap_or_else(|| "Windows".into()),
    }
}

/// Offline-first check (spec §20): a single short DNS probe, so modules can
/// show "Internet connection required" instead of failing with a stack trace.
#[tauri::command]
pub fn is_online() -> bool {
    use std::net::ToSocketAddrs;
    "example.com:443".to_socket_addrs().map(|mut a| a.next().is_some()).unwrap_or(false)
}

#[tauri::command]
pub fn dashboard(db: State<'_, Db>, session: State<'_, Session>) -> AppResult<DashboardData> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();

    let activity: Vec<ActivityRow> = {
        let mut stmt = conn.prepare("SELECT at, kind, label, target FROM activity ORDER BY id DESC LIMIT 12")?;
        let rows = stmt.query_map([], |r| Ok(ActivityRow {
            at: r.get(0)?, kind: r.get(1)?, label: r.get(2)?, target: r.get(3)?,
        }))?;
        rows.flatten().collect()
    };

    let recent_errors: Vec<crate::logging::LogLine> = {
        let mut stmt = conn.prepare(
            "SELECT id, at, level, module, message, detail FROM logs WHERE level IN ('error','warn') ORDER BY id DESC LIMIT 6")?;
        let rows = stmt.query_map([], |r| Ok(crate::logging::LogLine {
            id: r.get(0)?, at: r.get(1)?, level: r.get(2)?, module: r.get(3)?,
            message: r.get(4)?, detail: r.get(5)?,
        }))?;
        rows.flatten().collect()
    };

    let count = |t: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap_or(0)
    };

    Ok(DashboardData {
        resources: system_resources(),
        activity,
        recent_errors,
        project_count: count("projects"),
        server_count: count("servers"),
        cv_count: count("cvs"),
        online: is_online(),
        data_folder: crate::db::data_root().to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub fn export_report(
    session: State<'_, Session>,
    payload: String,
    format: String,
    destination: String,
) -> AppResult<String> {
    require_session(&session)?;
    let body = match format.as_str() {
        "json" => payload,
        "csv" => json_to_csv(&payload),
        _ => payload,
    };
    std::fs::write(&destination, body)?;
    Ok(destination)
}

fn json_to_csv(raw: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else { return raw.to_string() };
    let Some(array) = value.as_array() else { return raw.to_string() };
    let Some(first) = array.first().and_then(|v| v.as_object()) else { return raw.to_string() };

    let headers: Vec<String> = first.keys().cloned().collect();
    let mut out = headers.join(",");
    out.push('\n');
    for item in array {
        let Some(obj) = item.as_object() else { continue };
        let row: Vec<String> = headers.iter().map(|h| {
            let cell = obj.get(h).map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }).unwrap_or_default();
            format!("\"{}\"", cell.replace('"', "\"\""))
        }).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    out
}
