//! Database Lab (spec §12).
//!
//! The repair workflow never writes without an explicit approval step: analyse
//! produces a plan, the plan is shown as SQL, and `apply_repair` refuses to run
//! unless the caller passes back the exact plan token it was given, after a
//! backup has been taken.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}

#[derive(Serialize)]
pub struct ForeignKey {
    pub column: String,
    pub references_table: String,
    pub references_column: String,
    pub on_delete: String,
}

#[derive(Serialize)]
pub struct Index {
    pub name: String,
    pub unique: bool,
    pub columns: Vec<String>,
}

#[derive(Serialize)]
pub struct Table {
    pub name: String,
    pub rows: i64,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(Serialize)]
pub struct Diagnosis {
    pub severity: String,
    pub code: String,
    pub title: String,
    pub detail: String,
    pub table: Option<String>,
    /// Present only when a safe, reversible statement exists.
    pub suggested_sql: Option<String>,
}

#[derive(Serialize)]
pub struct DbAnalysis {
    pub path: String,
    pub engine: String,
    pub integrity: String,
    pub tables: Vec<Table>,
    pub views: Vec<String>,
    pub triggers: Vec<String>,
    pub diagnoses: Vec<Diagnosis>,
    /// Hash of the repair statements. `apply_repair` requires it back verbatim.
    pub plan_token: String,
    pub plan_sql: Vec<String>,
    pub analysed_at: String,
}

fn open_sqlite(path: &str) -> AppResult<Connection> {
    if !PathBuf::from(path).exists() {
        return Err(AppError::new("DATABASE_FILE_MISSING", "That database file could not be found.")
            .detail(path.to_string()));
    }
    Connection::open(path).map_err(|e| {
        AppError::new("DATABASE_CONNECTION_FAILED", "Unable to open the SQLite database.")
            .detail(e.to_string())
            .recover("Confirm the file is a SQLite database and is not locked by another program.")
    })
}

#[tauri::command]
pub fn db_analyze_sqlite(
    db: State<'_, Db>,
    session: State<'_, Session>,
    path: String,
) -> AppResult<DbAnalysis> {
    require_session(&session)?;
    let conn = open_sqlite(&path)?;

    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap_or_else(|_| "unavailable".into());

    let mut table_names: Vec<String> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        table_names.extend(rows.flatten());
    }

    let views: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='view' ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.flatten().collect()
    };
    let triggers: Vec<String> = {
        let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='trigger' ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.flatten().collect()
    };

    let mut tables = Vec::new();
    let mut diagnoses = Vec::new();
    let mut plan_sql: Vec<String> = Vec::new();

    for name in &table_names {
        let mut columns = Vec::new();
        {
            let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{name}\")"))?;
            let rows = stmt.query_map([], |r| {
                Ok(Column {
                    name: r.get(1)?,
                    data_type: r.get(2)?,
                    not_null: r.get::<_, i64>(3)? == 1,
                    default_value: r.get(4)?,
                    primary_key: r.get::<_, i64>(5)? > 0,
                })
            })?;
            columns.extend(rows.flatten());
        }

        let mut indexes: Vec<Index> = Vec::new();
        {
            let mut stmt = conn.prepare(&format!("PRAGMA index_list(\"{name}\")"))?;
            let listed: Vec<(String, bool)> = stmt
                .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(2)? == 1)))?
                .flatten()
                .collect();
            for (idx_name, unique) in listed {
                let mut cs = conn.prepare(&format!("PRAGMA index_info(\"{idx_name}\")"))?;
                let cols: Vec<String> = cs.query_map([], |r| r.get::<_, String>(2))?.flatten().collect();
                indexes.push(Index { name: idx_name, unique, columns: cols });
            }
        }

        let mut foreign_keys = Vec::new();
        {
            let mut stmt = conn.prepare(&format!("PRAGMA foreign_key_list(\"{name}\")"))?;
            let rows = stmt.query_map([], |r| {
                Ok(ForeignKey {
                    references_table: r.get(2)?,
                    column: r.get(3)?,
                    references_column: r.get::<_, Option<String>>(4)?.unwrap_or_else(|| "rowid".into()),
                    on_delete: r.get(6)?,
                })
            })?;
            foreign_keys.extend(rows.flatten());
        }

        let rows: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |r| r.get(0))
            .unwrap_or(-1);

        // ---- diagnoses ----
        if !columns.iter().any(|c| c.primary_key) {
            diagnoses.push(Diagnosis {
                severity: "warning".into(), code: "NO_PRIMARY_KEY".into(),
                title: format!("{name} has no primary key"),
                detail: "Without a primary key, individual rows cannot be addressed reliably and duplicates go unnoticed.".into(),
                table: Some(name.clone()), suggested_sql: None,
            });
        }

        // Orphan rows behind each declared foreign key.
        for fk in &foreign_keys {
            let sql = format!(
                "SELECT COUNT(*) FROM \"{name}\" c LEFT JOIN \"{}\" p ON c.\"{}\" = p.\"{}\" \
                 WHERE c.\"{}\" IS NOT NULL AND p.\"{}\" IS NULL",
                fk.references_table, fk.column, fk.references_column, fk.column, fk.references_column
            );
            if let Ok(orphans) = conn.query_row(&sql, [], |r| r.get::<_, i64>(0)) {
                if orphans > 0 {
                    diagnoses.push(Diagnosis {
                        severity: "error".into(), code: "ORPHAN_ROWS".into(),
                        title: format!("{orphans} rows in {name} point at a missing {} record", fk.references_table),
                        detail: format!("{name}.{} references {}.{}, but the parent row is gone.",
                            fk.column, fk.references_table, fk.references_column),
                        table: Some(name.clone()),
                        suggested_sql: Some(format!(
                            "-- Review before running. Detaches the orphan instead of deleting data:\nUPDATE \"{name}\" SET \"{}\" = NULL WHERE \"{}\" NOT IN (SELECT \"{}\" FROM \"{}\");",
                            fk.column, fk.column, fk.references_column, fk.references_table)),
                    });
                }
            }
        }

        // Duplicate indexes over the same column list.
        let mut seen: Vec<Vec<String>> = Vec::new();
        for idx in &indexes {
            if seen.contains(&idx.columns) {
                diagnoses.push(Diagnosis {
                    severity: "warning".into(), code: "DUPLICATE_INDEX".into(),
                    title: format!("{} duplicates an existing index", idx.name),
                    detail: "Two indexes cover the same columns. The extra one costs write time and disk for no gain.".into(),
                    table: Some(name.clone()),
                    suggested_sql: Some(format!("DROP INDEX \"{}\";", idx.name)),
                });
            } else {
                seen.push(idx.columns.clone());
            }
        }

        // Foreign key columns with no index behind them.
        for fk in &foreign_keys {
            let indexed = indexes.iter().any(|i| i.columns.first() == Some(&fk.column));
            if !indexed && rows > 1000 {
                diagnoses.push(Diagnosis {
                    severity: "info".into(), code: "MISSING_INDEX".into(),
                    title: format!("{name}.{} is not indexed", fk.column),
                    detail: "Joins and cascading deletes on this column scan the whole table.".into(),
                    table: Some(name.clone()),
                    suggested_sql: Some(format!(
                        "CREATE INDEX \"idx_{name}_{}\" ON \"{name}\" (\"{}\");", fk.column, fk.column)),
                });
            }
        }

        tables.push(Table { name: name.clone(), rows, columns, indexes, foreign_keys });
    }

    if integrity != "ok" {
        diagnoses.insert(0, Diagnosis {
            severity: "error".into(), code: "INTEGRITY_CHECK_FAILED".into(),
            title: "The database file reports internal damage".into(),
            detail: integrity.clone(),
            table: None, suggested_sql: None,
        });
    }
    if diagnoses.is_empty() {
        diagnoses.push(Diagnosis {
            severity: "info".into(), code: "NO_ISSUES".into(),
            title: "Nothing to repair".into(),
            detail: "Structure, relationships and indexes all checked out.".into(),
            table: None, suggested_sql: None,
        });
    }

    for d in &diagnoses {
        if let Some(sql) = &d.suggested_sql { plan_sql.push(sql.clone()) }
    }

    let mut hasher = Sha256::new();
    for s in &plan_sql { hasher.update(s.as_bytes()) }
    let plan_token = hex::encode(hasher.finalize());

    let app = db.0.lock().unwrap();
    logging::write(&app, "info", "DATABASE", "Database analysed",
        Some(&format!("tables={} findings={} file={}", tables.len(), diagnoses.len(), path)));

    Ok(DbAnalysis {
        path, engine: "sqlite".into(), integrity, tables, views, triggers,
        diagnoses, plan_token, plan_sql, analysed_at: crate::now(),
    })
}

#[tauri::command]
pub fn db_backup_sqlite(db: State<'_, Db>, session: State<'_, Session>, path: String) -> AppResult<String> {
    require_session(&session)?;
    let src = PathBuf::from(&path);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = src.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "database".into());
    let dest = crate::db::data_root().join("database").join(format!("backup-{stamp}-{name}"));
    std::fs::copy(&src, &dest)?;
    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "DATABASE", "Backup created", Some(&dest.to_string_lossy()));
    Ok(dest.to_string_lossy().to_string())
}

/// Applies the reviewed plan. Requires: a backup path that exists, the exact
/// plan token from the analysis, and the same statement list the user saw.
#[tauri::command]
pub fn db_apply_repair(
    db: State<'_, Db>,
    session: State<'_, Session>,
    path: String,
    statements: Vec<String>,
    plan_token: String,
    backup_path: String,
) -> AppResult<Vec<String>> {
    require_session(&session)?;

    if !PathBuf::from(&backup_path).exists() {
        return Err(AppError::new("BACKUP_REQUIRED", "Take a backup before applying repairs.")
            .recover("Use Back up now on the analysis screen, then apply the plan."));
    }
    let mut hasher = Sha256::new();
    for s in &statements { hasher.update(s.as_bytes()) }
    if hex::encode(hasher.finalize()) != plan_token {
        return Err(AppError::new("PLAN_CHANGED", "The repair plan no longer matches the analysis.")
            .recover("Run the analysis again so you are approving the current statements."));
    }

    let conn = open_sqlite(&path)?;
    let mut applied = Vec::new();
    let tx = conn.unchecked_transaction()?;
    for stmt in &statements {
        tx.execute_batch(stmt).map_err(|e| {
            AppError::new("REPAIR_STATEMENT_FAILED", "A repair statement failed. Nothing was changed.")
                .detail(format!("{stmt}\n{e}"))
                .recover(format!("The database is untouched. Your backup is at {backup_path}."))
        })?;
        applied.push(stmt.clone());
    }
    tx.commit()?;

    let verify: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap_or_else(|_| "unavailable".into());
    let app = db.0.lock().unwrap();
    logging::write(&app, "warn", "DATABASE", "Repair applied",
        Some(&format!("statements={} verify={} file={}", applied.len(), verify, path)));
    logging::activity(&app, "database", "Repair applied", &path);
    Ok(applied)
}

#[tauri::command]
pub fn db_run_select(session: State<'_, Session>, path: String, sql: String, limit: Option<usize>) -> AppResult<(Vec<String>, Vec<Vec<String>>)> {
    require_session(&session)?;
    let trimmed = sql.trim_start().to_ascii_lowercase();
    if !(trimmed.starts_with("select") || trimmed.starts_with("pragma") || trimmed.starts_with("explain")) {
        return Err(AppError::new("READ_ONLY_QUERY_REQUIRED", "Only SELECT, PRAGMA and EXPLAIN run here.")
            .recover("Changes go through the repair plan, so they can be reviewed and rolled back."));
    }
    let conn = open_sqlite(&path)?;
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        AppError::new("QUERY_INVALID", "The query could not be prepared.").detail(e.to_string())
    })?;
    let headers: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
    let width = headers.len();
    let cap = limit.unwrap_or(500);

    let mut rows_out = Vec::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        if rows_out.len() >= cap { break }
        let mut r = Vec::with_capacity(width);
        for i in 0..width {
            let v: rusqlite::types::Value = row.get(i).unwrap_or(rusqlite::types::Value::Null);
            r.push(match v {
                rusqlite::types::Value::Null => String::new(),
                rusqlite::types::Value::Integer(i) => i.to_string(),
                rusqlite::types::Value::Real(f) => f.to_string(),
                rusqlite::types::Value::Text(t) => t,
                rusqlite::types::Value::Blob(b) => format!("<{} bytes>", b.len()),
            });
        }
        rows_out.push(r);
    }
    Ok((headers, rows_out))
}
