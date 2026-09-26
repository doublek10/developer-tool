//! Web Page Clone.
//!
//! Fetches one publicly reachable page and separates what it finds into an
//! html/css/js bundle, so a user can see exactly how a page was laid out and
//! colored, keep it, or carry on editing it in Code Editor. Like Website Lab,
//! this sends one ordinary request per resource, never crafted payloads or
//! login attempts, and refuses to be pointed at a private or local address.
//!
//! Extraction is deliberately a lightweight tag scan rather than a full HTML
//! parser (matching the approach in `website.rs`): it moves `<style>` blocks,
//! stylesheet `<link>` tags, and `<script>` tags out of the markup, fetches
//! anything they point to, and leaves everything else untouched — including
//! inline `style="…"` attributes, which is where a lot of a page's exact
//! coloring actually lives.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use crate::website::{ensure_public, validate};
use regex::Regex;
use serde::Serialize;
use std::time::Duration;
use tauri::State;
use url::Url;

const MAX_ASSETS: usize = 20;
const MAX_ASSET_BYTES: usize = 2 * 1024 * 1024; // 2 MB per linked file

#[derive(Serialize)]
pub struct ClonedAsset {
    pub url: String,
    pub kind: String, // stylesheet | script
    pub bytes: usize,
    pub included: bool,
    pub note: Option<String>,
}

#[derive(Serialize)]
pub struct ClonedSite {
    pub source_url: String,
    pub final_url: String,
    pub title: String,
    pub html: String,
    pub css: String,
    pub js: String,
    pub assets: Vec<ClonedAsset>,
    pub warnings: Vec<String>,
    pub generated_at: String,
}

/// Case-insensitive `name="value"` / `name='value'` lookup inside a raw
/// attribute string (everything between the tag name and its closing `>`).
fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let pattern = format!(
        r#"(?is)\b{n}\s*=\s*"([^"]*)"|\b{n}\s*=\s*'([^']*)'"#,
        n = regex::escape(name)
    );
    let re = Regex::new(&pattern).ok()?;
    let caps = re.captures(attrs)?;
    caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str().trim().to_string())
}

async fn fetch_text(client: &reqwest::Client, url: &Url) -> Result<String, String> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err("unsupported address scheme".into());
    }
    ensure_public(url).map_err(|e| e.message)?;
    let resp = client.get(url.clone()).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("server responded {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_ASSET_BYTES {
        return Err("file too large to include".into());
    }
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// Where a cloned bundle lands when the user asks to open it in Code Editor
/// instead of picking a folder of their own — a scratch spot under this
/// app's own data directory, never the user's project folders.
#[derive(Serialize)]
pub struct ClonePlacement {
    pub root: String,
    pub index_path: String,
}

fn sanitize_slug(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    cleaned.trim_matches('-').chars().take(40).collect()
}

#[tauri::command]
pub fn website_clone_stage(
    session: State<'_, Session>,
    html: String,
    css: String,
    js: String,
    label: Option<String>,
) -> AppResult<ClonePlacement> {
    require_session(&session)?;
    let slug = label.as_deref().map(sanitize_slug).filter(|s| !s.is_empty()).unwrap_or_else(|| "site".into());
    let stamp = crate::now().chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    let root = crate::db::data_root().join("temp").join("clone").join(format!("{slug}-{stamp}"));
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("index.html"), html)?;
    std::fs::write(root.join("styles.css"), css)?;
    std::fs::write(root.join("script.js"), js)?;
    Ok(ClonePlacement {
        root: root.to_string_lossy().to_string(),
        index_path: root.join("index.html").to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn website_clone(
    db: State<'_, Db>,
    session: State<'_, Session>,
    target: String,
) -> AppResult<ClonedSite> {
    require_session(&session)?;
    let url = validate(&target)?;
    ensure_public(&url)?;

    {
        let conn = db.0.lock().unwrap();
        logging::write(&conn, "info", "CLONE", "Clone started", Some(&format!("target={url}")));
    }

    let client = reqwest::Client::builder()
        .user_agent("DevWorkstation/0.1 (page clone)")
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()?;

    let response = client.get(url.clone()).send().await.map_err(|e| {
        AppError::new("CLONE_FETCH_FAILED", "The page could not be fetched.")
            .detail(e.to_string())
            .recover("Check the address and that the site is reachable.")
    })?;
    let final_url = response.url().clone();
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::new("CLONE_BAD_STATUS", format!("The server responded with {status}."))
            .recover("Only a page that loads normally in a browser can be cloned."));
    }
    let body = response.text().await.map_err(|e| {
        AppError::new("CLONE_FETCH_FAILED", "The page body could not be read.").detail(e.to_string())
    })?;

    let title_re = Regex::new(r"(?is)<title\b[^>]*>(.*?)</title\s*>").unwrap();
    let style_re = Regex::new(r"(?is)<style\b[^>]*>(.*?)</style\s*>").unwrap();
    let link_re = Regex::new(r"(?is)<link\b([^>]*)/?>").unwrap();
    let script_re = Regex::new(r"(?is)<script\b([^>]*)>(.*?)</script\s*>").unwrap();

    let title = title_re
        .captures(&body)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_default();

    let mut css_parts: Vec<String> = Vec::new();
    let mut js_parts: Vec<String> = Vec::new();
    let mut assets: Vec<ClonedAsset> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut html = body.clone();
    let mut fetched = 0usize;

    for cap in style_re.captures_iter(&body) {
        let whole = cap.get(0).unwrap().as_str();
        let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if !inner.is_empty() {
            css_parts.push(inner.to_string());
        }
        html = html.replacen(whole, "", 1);
    }

    for cap in link_re.captures_iter(&body) {
        let whole = cap.get(0).unwrap().as_str();
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let rel = attr_value(attrs, "rel").unwrap_or_default().to_lowercase();
        if !rel.split_whitespace().any(|r| r == "stylesheet") {
            continue;
        }
        html = html.replacen(whole, "", 1);
        let Some(href) = attr_value(attrs, "href") else { continue };

        if fetched >= MAX_ASSETS {
            assets.push(ClonedAsset {
                url: href, kind: "stylesheet".into(), bytes: 0, included: false,
                note: Some("Skipped: reached the per-page asset limit".into()),
            });
            continue;
        }
        let Ok(asset_url) = final_url.join(&href) else {
            assets.push(ClonedAsset {
                url: href, kind: "stylesheet".into(), bytes: 0, included: false,
                note: Some("Skipped: could not resolve this address".into()),
            });
            continue;
        };
        fetched += 1;
        match fetch_text(&client, &asset_url).await {
            Ok(text) => {
                assets.push(ClonedAsset {
                    url: asset_url.to_string(), kind: "stylesheet".into(),
                    bytes: text.len(), included: true, note: None,
                });
                css_parts.push(format!("/* {asset_url} */\n{}", text.trim()));
            }
            Err(note) => {
                warnings.push(format!("Stylesheet not included: {asset_url} ({note})"));
                assets.push(ClonedAsset {
                    url: asset_url.to_string(), kind: "stylesheet".into(),
                    bytes: 0, included: false, note: Some(note),
                });
            }
        }
    }

    for cap in script_re.captures_iter(&body) {
        let whole = cap.get(0).unwrap().as_str();
        let attrs = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let inner = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let script_type = attr_value(attrs, "type").unwrap_or_default().to_lowercase();
        // Leave non-executable script tags alone (JSON-LD, inline templates,
        // etc.) — moving them into a .js file would change what they mean.
        let is_js_type = script_type.is_empty()
            || script_type.contains("javascript")
            || script_type == "module"
            || script_type == "text/babel";
        if !is_js_type {
            continue;
        }
        html = html.replacen(whole, "", 1);

        if let Some(src) = attr_value(attrs, "src") {
            if fetched >= MAX_ASSETS {
                assets.push(ClonedAsset {
                    url: src, kind: "script".into(), bytes: 0, included: false,
                    note: Some("Skipped: reached the per-page asset limit".into()),
                });
                continue;
            }
            let Ok(asset_url) = final_url.join(&src) else {
                assets.push(ClonedAsset {
                    url: src, kind: "script".into(), bytes: 0, included: false,
                    note: Some("Skipped: could not resolve this address".into()),
                });
                continue;
            };
            fetched += 1;
            match fetch_text(&client, &asset_url).await {
                Ok(text) => {
                    assets.push(ClonedAsset {
                        url: asset_url.to_string(), kind: "script".into(),
                        bytes: text.len(), included: true, note: None,
                    });
                    js_parts.push(format!("/* {asset_url} */\n{}", text.trim()));
                }
                Err(note) => {
                    warnings.push(format!("Script not included: {asset_url} ({note})"));
                    assets.push(ClonedAsset {
                        url: asset_url.to_string(), kind: "script".into(),
                        bytes: 0, included: false, note: Some(note),
                    });
                }
            }
        } else if !inner.trim().is_empty() {
            js_parts.push(inner.trim().to_string());
        }
    }

    if !css_parts.is_empty() {
        let tag = "  <link rel=\"stylesheet\" href=\"styles.css\">\n";
        match html.to_ascii_lowercase().find("</head>") {
            Some(idx) => html.insert_str(idx, tag),
            None => html = format!("{tag}{html}"),
        }
    }
    if !js_parts.is_empty() {
        let tag = "  <script src=\"script.js\"></script>\n";
        match html.to_ascii_lowercase().rfind("</body>") {
            Some(idx) => html.insert_str(idx, tag),
            None => html.push_str(&format!("\n{tag}")),
        }
    }

    let report = ClonedSite {
        source_url: url.to_string(),
        final_url: final_url.to_string(),
        title,
        html: html.trim().to_string(),
        css: css_parts.join("\n\n"),
        js: js_parts.join("\n\n"),
        assets,
        warnings,
        generated_at: crate::now(),
    };

    {
        let conn = db.0.lock().unwrap();
        logging::write(&conn, "info", "CLONE", "Clone complete",
            Some(&format!("target={} assets={}", report.source_url, report.assets.len())));
        logging::activity(&conn, "clone", "Web page cloned", url.host_str().unwrap_or(""));
        let _ = conn.execute(
            "INSERT INTO reports (module, title, payload, created_at) VALUES ('clone', ?1, ?2, ?3)",
            rusqlite::params![
                url.host_str().unwrap_or(""),
                serde_json::to_string(&report)?,
                crate::now()
            ],
        );
    }
    Ok(report)
}
