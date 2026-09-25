//! CV Builder (spec §6). Stores CV documents locally and typesets them to PDF
//! with a font compiled into the binary — no browser print dialog, no internet.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use printpdf::{BuiltinFont, Mm, PdfDocument, PdfDocumentReference, PdfLayerReference};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::io::BufWriter;
use tauri::State;

#[derive(Serialize, Deserialize, Clone)]
pub struct CvItem {
    #[serde(default)] pub title: String,
    #[serde(default)] pub subtitle: String,
    #[serde(default)] pub period: String,
    #[serde(default)] pub location: String,
    #[serde(default)] pub bullets: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CvSection {
    pub id: String,
    pub heading: String,
    /// list | items | text
    pub layout: String,
    #[serde(default)] pub items: Vec<CvItem>,
    #[serde(default)] pub entries: Vec<String>,
    #[serde(default)] pub text: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CvStyle {
    #[serde(default = "d_font")] pub font: String,
    #[serde(default = "d_size")] pub body_size: f32,
    #[serde(default = "d_heading")] pub heading_size: f32,
    #[serde(default = "d_name")] pub name_size: f32,
    #[serde(default = "d_leading")] pub line_spacing: f32,
    #[serde(default = "d_margin")] pub margin_mm: f32,
    #[serde(default = "d_page")] pub page: String,
    #[serde(default = "d_accent")] pub accent: String,
    #[serde(default = "d_align")] pub header_align: String,
}
fn d_font() -> String { "Helvetica".into() }
fn d_size() -> f32 { 10.0 }
fn d_heading() -> f32 { 12.0 }
fn d_name() -> f32 { 22.0 }
fn d_leading() -> f32 { 1.35 }
fn d_margin() -> f32 { 18.0 }
fn d_page() -> String { "A4".into() }
fn d_accent() -> String { "#1F3A5F".into() }
fn d_align() -> String { "left".into() }

impl Default for CvStyle {
    fn default() -> Self {
        Self { font: d_font(), body_size: d_size(), heading_size: d_heading(), name_size: d_name(),
               line_spacing: d_leading(), margin_mm: d_margin(), page: d_page(),
               accent: d_accent(), header_align: d_align() }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CvDocument {
    pub full_name: String,
    #[serde(default)] pub headline: String,
    #[serde(default)] pub email: String,
    #[serde(default)] pub phone: String,
    #[serde(default)] pub location: String,
    #[serde(default)] pub website: String,
    #[serde(default)] pub summary: String,
    #[serde(default)] pub sections: Vec<CvSection>,
    #[serde(default)] pub style: CvStyle,
}

#[derive(Serialize)]
pub struct CvSummary {
    pub id: i64,
    pub name: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn cv_list(db: State<'_, Db>, session: State<'_, Session>) -> AppResult<Vec<CvSummary>> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    let mut stmt = conn.prepare("SELECT id, name, updated_at FROM cvs ORDER BY updated_at DESC")?;
    let rows = stmt.query_map([], |r| Ok(CvSummary { id: r.get(0)?, name: r.get(1)?, updated_at: r.get(2)? }))?;
    Ok(rows.flatten().collect())
}

#[tauri::command]
pub fn cv_load(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> AppResult<CvDocument> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    let raw: String = conn.query_row("SELECT document FROM cvs WHERE id = ?1", params![id], |r| r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}

#[tauri::command]
pub fn cv_save(db: State<'_, Db>, session: State<'_, Session>, id: Option<i64>, name: String, document: CvDocument) -> AppResult<i64> {
    require_session(&session)?;
    let json = serde_json::to_string(&document)?;
    let conn = db.0.lock().unwrap();
    let now = crate::now();
    match id {
        Some(existing) => {
            conn.execute("UPDATE cvs SET name = ?1, document = ?2, updated_at = ?3 WHERE id = ?4",
                params![name, json, now, existing])?;
            logging::write(&conn, "info", "CV", "CV saved", Some(&name));
            Ok(existing)
        }
        None => {
            conn.execute("INSERT INTO cvs (name, document, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
                params![name, json, now])?;
            let new_id = conn.last_insert_rowid();
            logging::write(&conn, "info", "CV", "CV created", Some(&name));
            Ok(new_id)
        }
    }
}

#[tauri::command]
pub fn cv_duplicate(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> AppResult<i64> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    let (name, doc): (String, String) = conn.query_row(
        "SELECT name, document FROM cvs WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    conn.execute("INSERT INTO cvs (name, document, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![format!("{name} (copy)"), doc, crate::now()])?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn cv_delete(db: State<'_, Db>, session: State<'_, Session>, id: i64) -> AppResult<()> {
    require_session(&session)?;
    let conn = db.0.lock().unwrap();
    conn.execute("DELETE FROM cvs WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------- PDF engine

struct Cursor {
    y: f32,
    page_h: f32,
    page_w: f32,
    margin: f32,
}

fn page_size(name: &str) -> (f32, f32) {
    match name {
        "Letter" => (215.9, 279.4),
        "Legal" => (215.9, 355.6),
        _ => (210.0, 297.0), // A4
    }
}

/// Rough character-width metric for the built-in Helvetica/Times faces, good
/// enough to wrap paragraphs without embedding a metrics table.
fn wrap(text: &str, size: f32, width_mm: f32) -> Vec<String> {
    let char_mm = size * 0.35278 * 0.50;
    let max_chars = ((width_mm / char_mm).floor() as usize).max(12);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if line.is_empty() {
                line = word.to_string();
            } else if line.len() + 1 + word.len() <= max_chars {
                line.push(' ');
                line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines
}

fn hex_rgb(hex: &str) -> (f32, f32, f32) {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 { return (0.12, 0.23, 0.37) }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    (p(0), p(2), p(4))
}

struct Writer<'a> {
    doc: &'a PdfDocumentReference,
    layer: PdfLayerReference,
    font: printpdf::IndirectFontRef,
    bold: printpdf::IndirectFontRef,
    cursor: Cursor,
    style: &'a CvStyle,
}

impl<'a> Writer<'a> {
    fn new_page(&mut self) {
        let (w, h) = (self.cursor.page_w, self.cursor.page_h);
        let (page, layer) = self.doc.add_page(Mm(w), Mm(h), "Layer");
        self.layer = self.doc.get_page(page).get_layer(layer);
        self.cursor.y = h - self.cursor.margin;
    }

    fn space(&mut self, mm: f32) {
        self.cursor.y -= mm;
    }

    fn ensure(&mut self, needed: f32) {
        if self.cursor.y - needed < self.cursor.margin {
            self.new_page();
        }
    }

    fn line(&mut self, text: &str, size: f32, bold: bool, indent: f32, color: Option<(f32, f32, f32)>) {
        let lh = size * 0.35278 * self.style.line_spacing;
        self.ensure(lh);
        let (r, g, b) = color.unwrap_or((0.10, 0.10, 0.12));
        self.layer.set_fill_color(printpdf::Color::Rgb(printpdf::Rgb::new(r, g, b, None)));
        let face = if bold { &self.bold } else { &self.font };
        self.layer.use_text(text, size as f32, Mm(self.cursor.margin + indent), Mm(self.cursor.y), face);
        self.cursor.y -= lh;
    }

    fn rule(&mut self, accent: (f32, f32, f32)) {
        self.ensure(3.0);
        let x0 = self.cursor.margin;
        let x1 = self.cursor.page_w - self.cursor.margin;
        let y = self.cursor.y + 1.5;
        let points = vec![
            (printpdf::Point::new(Mm(x0), Mm(y)), false),
            (printpdf::Point::new(Mm(x1), Mm(y)), false),
        ];
        self.layer.set_outline_color(printpdf::Color::Rgb(printpdf::Rgb::new(accent.0, accent.1, accent.2, None)));
        self.layer.set_outline_thickness(0.6);
        self.layer.add_line(printpdf::Line { points, is_closed: false });
        self.cursor.y -= 2.5;
    }
}

#[tauri::command]
pub fn cv_export_pdf(
    db: State<'_, Db>,
    session: State<'_, Session>,
    document: CvDocument,
    destination: String,
) -> AppResult<String> {
    require_session(&session)?;
    if document.full_name.trim().is_empty() {
        return Err(AppError::new("CV_NAME_REQUIRED", "Add your name before exporting.")
            .recover("The name appears at the top of every template."));
    }

    let style = document.style.clone();
    let (pw, ph) = page_size(&style.page);
    let (doc, page, layer) = PdfDocument::new("Curriculum Vitae", Mm(pw), Mm(ph), "Layer");

    let (regular, bold) = match style.font.as_str() {
        "Times" => (BuiltinFont::TimesRoman, BuiltinFont::TimesBold),
        "Courier" => (BuiltinFont::Courier, BuiltinFont::CourierBold),
        _ => (BuiltinFont::Helvetica, BuiltinFont::HelveticaBold),
    };
    let font = doc.add_builtin_font(regular)
        .map_err(|e| AppError::new("PDF_FONT_FAILED", "The document font could not be prepared.").detail(e.to_string()))?;
    let font_bold = doc.add_builtin_font(bold)
        .map_err(|e| AppError::new("PDF_FONT_FAILED", "The document font could not be prepared.").detail(e.to_string()))?;

    let accent = hex_rgb(&style.accent);
    let content_w = pw - style.margin_mm * 2.0;

    let mut w = Writer {
        doc: &doc,
        layer: doc.get_page(page).get_layer(layer),
        font,
        bold: font_bold,
        cursor: Cursor { y: ph - style.margin_mm, page_h: ph, page_w: pw, margin: style.margin_mm },
        style: &style,
    };

    // Header
    w.line(&document.full_name, style.name_size, true, 0.0, Some(accent));
    if !document.headline.is_empty() {
        w.line(&document.headline, style.body_size + 1.0, false, 0.0, Some((0.35, 0.35, 0.40)));
    }
    let contact: Vec<String> = [&document.email, &document.phone, &document.location, &document.website]
        .iter().filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
    if !contact.is_empty() {
        w.line(&contact.join("   |   "), style.body_size - 0.5, false, 0.0, Some((0.40, 0.40, 0.45)));
    }
    w.space(2.0);
    w.rule(accent);
    w.space(2.0);

    if !document.summary.trim().is_empty() {
        w.line("Profile", style.heading_size, true, 0.0, Some(accent));
        w.space(1.0);
        for l in wrap(&document.summary, style.body_size, content_w) {
            w.line(&l, style.body_size, false, 0.0, None);
        }
        w.space(3.0);
    }

    for section in &document.sections {
        if section.heading.trim().is_empty() { continue }
        w.ensure(18.0);
        w.line(&section.heading, style.heading_size, true, 0.0, Some(accent));
        w.space(1.0);

        match section.layout.as_str() {
            "items" => {
                for item in &section.items {
                    w.ensure(12.0);
                    if !item.title.is_empty() {
                        w.line(&item.title, style.body_size + 0.5, true, 0.0, None);
                    }
                    let meta: Vec<String> = [&item.subtitle, &item.location, &item.period]
                        .iter().filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
                    if !meta.is_empty() {
                        w.line(&meta.join("  ·  "), style.body_size - 0.5, false, 0.0, Some((0.42, 0.42, 0.47)));
                    }
                    for bullet in &item.bullets {
                        for (i, l) in wrap(bullet, style.body_size, content_w - 5.0).into_iter().enumerate() {
                            let text = if i == 0 { format!("•  {l}") } else { format!("    {l}") };
                            w.line(&text, style.body_size, false, 3.0, None);
                        }
                    }
                    w.space(2.0);
                }
            }
            "list" => {
                for entry in &section.entries {
                    for (i, l) in wrap(entry, style.body_size, content_w - 5.0).into_iter().enumerate() {
                        let text = if i == 0 { format!("•  {l}") } else { format!("    {l}") };
                        w.line(&text, style.body_size, false, 3.0, None);
                    }
                }
                w.space(2.0);
            }
            _ => {
                for l in wrap(&section.text, style.body_size, content_w) {
                    w.line(&l, style.body_size, false, 0.0, None);
                }
                w.space(2.0);
            }
        }
    }

    let file = std::fs::File::create(&destination)?;
    doc.save(&mut BufWriter::new(file))
        .map_err(|e| AppError::new("PDF_SAVE_FAILED", "The PDF could not be written.")
            .detail(e.to_string())
            .recover("Choose a folder you can write to, and make sure the file isn't open elsewhere."))?;

    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "CV", "CV exported to PDF", Some(&destination));
    logging::activity(&conn, "cv", "CV exported", &destination);
    Ok(destination)
}
