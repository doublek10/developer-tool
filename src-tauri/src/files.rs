//! File Manager (spec §16). Plain filesystem work with structured errors.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;
use walkdir::WalkDir;

#[derive(Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    pub kind: String,
}

fn kind_of(p: &Path, is_dir: bool) -> String {
    if is_dir { return "folder".into() }
    match p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "pdf" => "pdf",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => "image",
        "zip" | "7z" | "rar" | "tar" | "gz" => "archive",
        "sqlite" | "sqlite3" | "db" => "database",
        "md" | "txt" | "log" => "text",
        "json" | "yml" | "yaml" | "toml" | "xml" | "ini" | "env" => "config",
        "" => "file",
        _ => "code",
    }
    .to_string()
}

fn stamp(meta: &std::fs::Metadata) -> String {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default()
}

#[tauri::command]
pub fn fs_list(session: State<'_, Session>, path: String) -> AppResult<Vec<Entry>> {
    require_session(&session)?;
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(AppError::new("FOLDER_NOT_FOUND", "That folder could not be opened.")
            .detail(path)
            .recover("It may have been moved or renamed. Go up a level and try again."));
    }
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir)?.flatten() {
        let meta = match e.metadata() { Ok(m) => m, Err(_) => continue };
        let p = e.path();
        out.push(Entry {
            name: e.file_name().to_string_lossy().to_string(),
            path: p.to_string_lossy().to_string(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified: stamp(&meta),
            kind: kind_of(&p, meta.is_dir()),
        });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

#[tauri::command]
pub fn fs_home(session: State<'_, Session>) -> AppResult<String> {
    require_session(&session)?;
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    Ok(home.to_string_lossy().to_string())
}

#[tauri::command]
pub fn fs_read_text(session: State<'_, Session>, path: String, max_bytes: Option<u64>) -> AppResult<String> {
    require_session(&session)?;
    let cap = max_bytes.unwrap_or(512 * 1024);
    let meta = std::fs::metadata(&path)?;
    if meta.len() > cap {
        return Err(AppError::new("FILE_TOO_LARGE", "This file is too large to preview.")
            .detail(format!("{} bytes", meta.len()))
            .recover("Open it in your editor instead."));
    }
    std::fs::read_to_string(&path).map_err(|e| {
        AppError::new("FILE_NOT_TEXT", "This file isn't readable as text.")
            .detail(e.to_string())
            .recover("It may be a binary file such as an image or an executable.")
    })
}

#[tauri::command]
pub fn fs_write_text(db: State<'_, Db>, session: State<'_, Session>, path: String, content: String) -> AppResult<()> {
    require_session(&session)?;
    std::fs::write(&path, content)?;
    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "FILES", "File saved", Some(&path));
    Ok(())
}

#[tauri::command]
pub fn fs_create_dir(session: State<'_, Session>, path: String) -> AppResult<()> {
    require_session(&session)?;
    std::fs::create_dir_all(&path)?;
    Ok(())
}

#[tauri::command]
pub fn fs_rename(session: State<'_, Session>, from: String, to: String) -> AppResult<()> {
    require_session(&session)?;
    if PathBuf::from(&to).exists() {
        return Err(AppError::new("NAME_IN_USE", "Something with that name is already here."));
    }
    std::fs::rename(&from, &to)?;
    Ok(())
}

#[tauri::command]
pub fn fs_delete(db: State<'_, Db>, session: State<'_, Session>, path: String) -> AppResult<()> {
    require_session(&session)?;
    let p = PathBuf::from(&path);
    if p.parent().is_none() {
        return Err(AppError::new("DELETE_REFUSED", "A drive root cannot be deleted."));
    }
    if p.is_dir() { std::fs::remove_dir_all(&p)? } else { std::fs::remove_file(&p)? }
    let conn = db.0.lock().unwrap();
    logging::write(&conn, "warn", "FILES", "Deleted", Some(&path));
    Ok(())
}

#[tauri::command]
pub fn fs_copy(session: State<'_, Session>, from: String, to: String) -> AppResult<()> {
    require_session(&session)?;
    let src = PathBuf::from(&from);
    if src.is_dir() {
        for entry in WalkDir::new(&src).into_iter().filter_map(|e| e.ok()) {
            let rel = entry.path().strip_prefix(&src).unwrap();
            let dest = PathBuf::from(&to).join(rel);
            if entry.file_type().is_dir() { std::fs::create_dir_all(&dest)? }
            else { std::fs::copy(entry.path(), &dest)?; }
        }
    } else {
        std::fs::copy(&src, &to)?;
    }
    Ok(())
}

#[tauri::command]
pub fn fs_search(session: State<'_, Session>, root: String, query: String, limit: Option<usize>) -> AppResult<Vec<Entry>> {
    require_session(&session)?;
    let needle = query.to_lowercase();
    let cap = limit.unwrap_or(300);
    let mut out = Vec::new();
    for entry in WalkDir::new(&root).max_depth(8).into_iter().filter_map(|e| e.ok()) {
        if out.len() >= cap { break }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.contains(&needle) { continue }
        let Ok(meta) = entry.metadata() else { continue };
        out.push(Entry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            is_dir: meta.is_dir(),
            size: meta.len(),
            modified: stamp(&meta),
            kind: kind_of(entry.path(), meta.is_dir()),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn fs_zip(session: State<'_, Session>, source: String, destination: String) -> AppResult<String> {
    require_session(&session)?;
    let src = PathBuf::from(&source);
    let file = std::fs::File::create(&destination)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    if src.is_file() {
        zip.start_file(src.file_name().unwrap().to_string_lossy(), opts)
            .map_err(|e| AppError::new("ZIP_FAILED", "The archive could not be written.").detail(e.to_string()))?;
        std::io::copy(&mut std::fs::File::open(&src)?, &mut zip)?;
    } else {
        for entry in WalkDir::new(&src).into_iter().filter_map(|e| e.ok()) {
            let rel = entry.path().strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
            if rel.is_empty() { continue }
            if entry.file_type().is_dir() {
                let _ = zip.add_directory(format!("{rel}/"), opts);
            } else {
                zip.start_file(&rel, opts)
                    .map_err(|e| AppError::new("ZIP_FAILED", "The archive could not be written.").detail(e.to_string()))?;
                std::io::copy(&mut std::fs::File::open(entry.path())?, &mut zip)?;
            }
        }
    }
    zip.finish().map_err(|e| AppError::new("ZIP_FAILED", "The archive could not be closed.").detail(e.to_string()))?;
    Ok(destination)
}

#[tauri::command]
pub fn fs_unzip(session: State<'_, Session>, archive: String, destination: String) -> AppResult<String> {
    require_session(&session)?;
    let file = std::fs::File::open(&archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| AppError::new("ARCHIVE_UNREADABLE", "That archive could not be opened.").detail(e.to_string()))?;
    let root = PathBuf::from(&destination);
    std::fs::create_dir_all(&root)?;

    for i in 0..zip.len() {
        let mut item = zip.by_index(i)
            .map_err(|e| AppError::new("ARCHIVE_UNREADABLE", "An entry in the archive is damaged.").detail(e.to_string()))?;
        // Refuse entries that would escape the destination folder.
        let Some(safe) = item.enclosed_name() else { continue };
        let out = root.join(safe);
        if item.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(parent) = out.parent() { std::fs::create_dir_all(parent)? }
            std::io::copy(&mut item, &mut std::fs::File::create(&out)?)?;
        }
    }
    Ok(destination)
}
