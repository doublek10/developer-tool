//! PDF Studio (spec §7). Everything here runs locally.
//!
//! Honest about the format: a PDF is a page-description file, not a word
//! processor document. `inspect` classifies each page so the interface can
//! tell the user which pages hold real text and which are pictures of text.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use lopdf::{Document, Object};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct PageInfo {
    pub number: u32,
    pub classification: String, // text | image | mixed | empty
    pub characters: usize,
    pub images: usize,
    pub width: f32,
    pub height: f32,
    pub rotation: i64,
}

#[derive(Serialize)]
pub struct PdfInfo {
    pub path: String,
    pub pages: Vec<PageInfo>,
    pub page_count: usize,
    pub encrypted: bool,
    pub title: String,
    pub author: String,
    pub producer: String,
    pub version: String,
    pub bytes: u64,
    pub needs_ocr: bool,
    pub note: String,
}

fn load(path: &str) -> AppResult<Document> {
    Document::load(path).map_err(|e| {
        AppError::new("PDF_UNREADABLE", "That PDF could not be opened.")
            .detail(e.to_string())
            .recover("The file may be damaged, or protected with a password.")
    })
}

fn meta(doc: &Document, key: &str) -> String {
    doc.trailer
        .get(b"Info")
        .and_then(|o| doc.dereference(o).map(|(_, v)| v))
        .ok()
        .and_then(|info| info.as_dict().ok().cloned())
        .and_then(|d| d.get(key.as_bytes()).ok().cloned())
        .and_then(|v| match v {
            Object::String(bytes, _) => Some(String::from_utf8_lossy(&bytes).to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn pdf_inspect(db: State<'_, Db>, session: State<'_, Session>, path: String) -> AppResult<PdfInfo> {
    require_session(&session)?;
    let doc = load(&path)?;
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    let mut pages = Vec::new();
    let page_map = doc.get_pages();

    for (number, id) in &page_map {
        let text = doc.extract_text(&[*number]).unwrap_or_default();
        let characters = text.trim().chars().filter(|c| !c.is_whitespace()).count();

        let (mut width, mut height, mut rotation, mut images) = (595.0f32, 842.0f32, 0i64, 0usize);
        if let Ok(dict) = doc.get_dictionary(*id) {
            if let Ok(Object::Array(bx)) = dict.get(b"MediaBox") {
                let nums: Vec<f32> = bx.iter().filter_map(|o| o.as_float().ok()).collect();
                if nums.len() == 4 {
                    width = nums[2] - nums[0];
                    height = nums[3] - nums[1];
                }
            }
            rotation = dict.get(b"Rotate").and_then(|o| o.as_i64()).unwrap_or(0);
            if let Ok(res) = dict.get(b"Resources").and_then(|o| doc.dereference(o).map(|(_, v)| v)) {
                if let Ok(rd) = res.as_dict() {
                    if let Ok(xo) = rd.get(b"XObject").and_then(|o| doc.dereference(o).map(|(_, v)| v)) {
                        if let Ok(xd) = xo.as_dict() { images = xd.iter().count() }
                    }
                }
            }
        }

        let classification = match (characters, images) {
            (0, 0) => "empty",
            (0, _) => "image",
            (_, 0) => "text",
            _ => "mixed",
        };

        pages.push(PageInfo {
            number: *number,
            classification: classification.into(),
            characters,
            images,
            width,
            height,
            rotation,
        });
    }

    let needs_ocr = !pages.is_empty()
        && pages.iter().all(|p| p.classification == "image" || p.classification == "empty");

    let note = if needs_ocr {
        "Every page is a picture. The words cannot be selected or edited until the pages are put through text recognition.".to_string()
    } else if pages.iter().any(|p| p.classification == "image") {
        "Some pages are pictures of text. Those pages can be rearranged and annotated, but their words cannot be edited directly.".to_string()
    } else {
        "This document contains real text, so it can be searched and annotated.".to_string()
    };

    let info = PdfInfo {
        path: path.clone(),
        page_count: pages.len(),
        pages,
        encrypted: doc.is_encrypted(),
        title: meta(&doc, "Title"),
        author: meta(&doc, "Author"),
        producer: meta(&doc, "Producer"),
        version: doc.version.clone(),
        bytes,
        needs_ocr,
        note,
    };

    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "PDF", "Document inspected",
        Some(&format!("pages={} file={}", info.page_count, path)));
    Ok(info)
}

#[tauri::command]
pub fn pdf_extract_text(session: State<'_, Session>, path: String, page: u32) -> AppResult<String> {
    require_session(&session)?;
    let doc = load(&path)?;
    doc.extract_text(&[page]).map_err(|e| {
        AppError::new("PDF_TEXT_UNAVAILABLE", "No selectable text on that page.")
            .detail(e.to_string())
            .recover("The page is most likely a scan. Run text recognition first.")
    })
}

#[tauri::command]
pub fn pdf_delete_pages(db: State<'_, Db>, session: State<'_, Session>, path: String, pages: Vec<u32>, destination: String) -> AppResult<String> {
    require_session(&session)?;
    let mut doc = load(&path)?;
    doc.delete_pages(&pages);
    doc.prune_objects();
    doc.compress();
    doc.save(&destination).map_err(|e| {
        AppError::new("PDF_SAVE_FAILED", "The edited PDF could not be saved.").detail(e.to_string())
    })?;
    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "PDF", "Pages removed", Some(&format!("count={} out={}", pages.len(), destination)));
    Ok(destination)
}

#[tauri::command]
pub fn pdf_rotate_pages(session: State<'_, Session>, path: String, pages: Vec<u32>, degrees: i64, destination: String) -> AppResult<String> {
    require_session(&session)?;
    let mut doc = load(&path)?;
    let map = doc.get_pages();
    for p in &pages {
        if let Some(id) = map.get(p) {
            if let Ok(dict) = doc.get_object_mut(*id).and_then(|o| o.as_dict_mut()) {
                let current = dict.get(b"Rotate").and_then(|o| o.as_i64()).unwrap_or(0);
                dict.set("Rotate", Object::Integer((current + degrees).rem_euclid(360)));
            }
        }
    }
    doc.save(&destination).map_err(|e| {
        AppError::new("PDF_SAVE_FAILED", "The rotated PDF could not be saved.").detail(e.to_string())
    })?;
    Ok(destination)
}

#[tauri::command]
pub fn pdf_merge(db: State<'_, Db>, session: State<'_, Session>, sources: Vec<String>, destination: String) -> AppResult<String> {
    require_session(&session)?;
    if sources.len() < 2 {
        return Err(AppError::new("MERGE_NEEDS_TWO", "Choose at least two PDFs to combine."));
    }

    let mut merged = Document::with_version("1.5");
    let mut page_ids = Vec::new();
    let mut max_id = 1u32;

    for src in &sources {
        let mut doc = load(src)?;
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;
        let pages = doc.get_pages();
        for (_, id) in pages {
            page_ids.push(id);
        }
        merged.objects.extend(doc.objects);
    }

    let pages_id = merged.new_object_id();
    let mut pages_dict = lopdf::Dictionary::new();
    pages_dict.set("Type", "Pages");
    pages_dict.set("Count", Object::Integer(page_ids.len() as i64));
    pages_dict.set(
        "Kids",
        Object::Array(page_ids.iter().map(|id| Object::Reference(*id)).collect()),
    );
    for id in &page_ids {
        if let Ok(d) = merged.get_object_mut(*id).and_then(|o| o.as_dict_mut()) {
            d.set("Parent", Object::Reference(pages_id));
        }
    }
    merged.objects.insert(pages_id, Object::Dictionary(pages_dict));

    let catalog_id = merged.new_object_id();
    let mut catalog = lopdf::Dictionary::new();
    catalog.set("Type", "Catalog");
    catalog.set("Pages", Object::Reference(pages_id));
    merged.objects.insert(catalog_id, Object::Dictionary(catalog));
    merged.trailer.set("Root", Object::Reference(catalog_id));

    merged.compress();
    merged.save(&destination).map_err(|e| {
        AppError::new("PDF_SAVE_FAILED", "The combined PDF could not be saved.").detail(e.to_string())
    })?;

    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "PDF", "Documents combined",
        Some(&format!("sources={} out={}", sources.len(), destination)));
    Ok(destination)
}

#[tauri::command]
pub fn pdf_split(session: State<'_, Session>, path: String, keep: Vec<u32>, destination: String) -> AppResult<String> {
    require_session(&session)?;
    let mut doc = load(&path)?;
    let all: Vec<u32> = doc.get_pages().keys().copied().collect();
    let drop: Vec<u32> = all.into_iter().filter(|p| !keep.contains(p)).collect();
    if drop.is_empty() && keep.is_empty() {
        return Err(AppError::new("NO_PAGES_SELECTED", "Choose the pages you want to keep."));
    }
    doc.delete_pages(&drop);
    doc.prune_objects();
    doc.compress();
    doc.save(&destination).map_err(|e| {
        AppError::new("PDF_SAVE_FAILED", "The extracted pages could not be saved.").detail(e.to_string())
    })?;
    Ok(destination)
}
