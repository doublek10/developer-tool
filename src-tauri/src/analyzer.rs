//! Code Analyzer (spec §10, §11, §25, §26).
//!
//! Runs entirely offline. Reads source as text only — nothing in a scanned
//! project is ever executed. Heavy directories are skipped unless asked for.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use tauri::State;
use walkdir::WalkDir;

const SKIP_DIRS: &[&str] = &[
    "node_modules", ".git", "dist", "build", ".next", "__pycache__",
    "vendor", "target", ".venv", "venv", ".cache", "coverage", ".idea", ".vscode",
];

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Serialize)]
pub struct FileNode {
    pub path: String,
    pub language: String,
    pub lines: usize,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: String, // import | api | database
}

#[derive(Serialize)]
pub struct Insight {
    pub severity: String,
    pub category: String,
    pub title: String,
    pub detail: String,
    pub path: Option<String>,
    pub line: Option<usize>,
}

#[derive(Serialize)]
pub struct Analysis {
    pub root: String,
    pub technologies: Vec<String>,
    pub entry_points: Vec<String>,
    pub routes: Vec<String>,
    pub api_calls: Vec<String>,
    pub database_hints: Vec<String>,
    pub env_vars: Vec<String>,
    pub dependencies: BTreeMap<String, String>,
    pub language_totals: BTreeMap<String, usize>,
    pub files: Vec<FileNode>,
    pub edges: Vec<Edge>,
    pub insights: Vec<Insight>,
    pub file_count: usize,
    pub skipped_count: usize,
    pub scanned_at: String,
}

fn language_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "ts" => "TypeScript",
        "tsx" => "TypeScript React",
        "js" | "mjs" | "cjs" => "JavaScript",
        "jsx" => "JavaScript React",
        "py" => "Python",
        "php" => "PHP",
        "rs" => "Rust",
        "java" => "Java",
        "cs" => "C#",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "go" => "Go",
        "rb" => "Ruby",
        "sql" => "SQL",
        "html" | "htm" => "HTML",
        "css" | "scss" | "sass" | "less" => "CSS",
        "json" => "JSON",
        "yml" | "yaml" => "YAML",
        "xml" => "XML",
        "sh" | "bash" => "Shell",
        "ps1" => "PowerShell",
        "md" => "Markdown",
        _ => "",
    }
}

#[tauri::command]
pub fn analyze_project(
    db: State<'_, Db>,
    session: State<'_, Session>,
    root: String,
    include_heavy_dirs: Option<bool>,
    extra_ignores: Option<Vec<String>>,
) -> AppResult<Analysis> {
    require_session(&session)?;
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(AppError::new("PROJECT_NOT_FOUND", "That folder could not be opened.")
            .detail(root.clone())
            .recover("Pick the top-level folder of the project you want to analyse."));
    }

    let heavy = include_heavy_dirs.unwrap_or(false);
    let extra: HashSet<String> = extra_ignores.unwrap_or_default().into_iter().collect();

    let mut files = Vec::new();
    let mut technologies: HashSet<String> = HashSet::new();
    let mut entry_points = Vec::new();
    let mut routes = Vec::new();
    let mut api_calls: HashSet<String> = HashSet::new();
    let mut database_hints: HashSet<String> = HashSet::new();
    let mut env_vars: HashSet<String> = HashSet::new();
    let mut dependencies = BTreeMap::new();
    let mut language_totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut edges = Vec::new();
    let mut insights = Vec::new();
    let mut skipped = 0usize;

    let walker = WalkDir::new(&root_path).follow_links(false).into_iter().filter_entry(|e| {
        if e.depth() == 0 { return true }
        let name = e.file_name().to_string_lossy().to_string();
        if e.file_type().is_dir() {
            if extra.contains(&name) { return false }
            if !heavy && SKIP_DIRS.contains(&name.as_str()) { return false }
        }
        true
    });

    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() { continue }
        let path = entry.path().to_path_buf();
        let rel = path.strip_prefix(&root_path).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let name = entry.file_name().to_string_lossy().to_string();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        // Manifests tell us what the project is.
        match name.as_str() {
            "package.json" => {
                technologies.insert("Node.js".into());
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        for key in ["dependencies", "devDependencies"] {
                            if let Some(map) = json.get(key).and_then(|d| d.as_object()) {
                                for (k, v) in map {
                                    dependencies.insert(k.clone(), v.as_str().unwrap_or("").to_string());
                                    match k.as_str() {
                                        "next" => { technologies.insert("Next.js".into()); }
                                        "react" => { technologies.insert("React".into()); }
                                        "vue" => { technologies.insert("Vue".into()); }
                                        "express" => { technologies.insert("Express".into()); }
                                        "typescript" => { technologies.insert("TypeScript".into()); }
                                        "prisma" | "@prisma/client" => { database_hints.insert("Prisma ORM".into()); }
                                        "mongoose" => { database_hints.insert("MongoDB (mongoose)".into()); }
                                        "pg" => { database_hints.insert("PostgreSQL (pg)".into()); }
                                        "mysql" | "mysql2" => { database_hints.insert("MySQL".into()); }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if let Some(main) = json.get("main").and_then(|m| m.as_str()) {
                            entry_points.push(main.to_string());
                        }
                    }
                }
            }
            "requirements.txt" | "pyproject.toml" => { technologies.insert("Python".into()); }
            "composer.json" => { technologies.insert("PHP".into()); }
            "Cargo.toml" => { technologies.insert("Rust".into()); }
            "pom.xml" | "build.gradle" => { technologies.insert("Java".into()); }
            "Dockerfile" => { technologies.insert("Docker".into()); }
            "go.mod" => { technologies.insert("Go".into()); }
            _ => {}
        }

        if matches!(name.as_str(), "index.js"|"index.ts"|"main.ts"|"main.py"|"app.py"|"server.js"|"main.rs"|"index.php") {
            entry_points.push(rel.clone());
        }

        let lang = language_of(&path);
        if lang.is_empty() { continue }
        if size > MAX_FILE_BYTES { skipped += 1; continue }

        let Ok(text) = std::fs::read_to_string(&path) else { skipped += 1; continue };
        let lines = text.lines().count();
        *language_totals.entry(lang.to_string()).or_insert(0) += lines;

        // Next.js / Express style route detection.
        if rel.contains("/pages/") || rel.contains("/app/") || rel.starts_with("pages/") || rel.starts_with("app/") {
            if matches!(lang, "TypeScript"|"TypeScript React"|"JavaScript"|"JavaScript React") {
                routes.push(rel.clone());
            }
        }

        for (i, line) in text.lines().enumerate() {
            let ln = i + 1;
            let trimmed = line.trim();

            // imports -> dependency edges
            if let Some(target) = import_target(trimmed) {
                if target.starts_with('.') {
                    edges.push(Edge { from: rel.clone(), to: normalise_rel(&rel, &target), kind: "import".into() });
                }
            }
            if trimmed.contains("app.get(") || trimmed.contains("app.post(") || trimmed.contains("router.") {
                routes.push(format!("{rel}:{ln}"));
            }
            if trimmed.contains("fetch(") || trimmed.contains("axios.") || trimmed.contains("requests.get") {
                api_calls.insert(format!("{rel}:{ln}"));
                edges.push(Edge { from: rel.clone(), to: "external API".into(), kind: "api".into() });
            }
            for hint in ["createConnection", "createPool", "psycopg2", "sqlalchemy", "PDO(", "mysqli", "MongoClient", "sqlite3"] {
                if trimmed.contains(hint) {
                    database_hints.insert(format!("{hint} in {rel}:{ln}"));
                    edges.push(Edge { from: rel.clone(), to: "database".into(), kind: "database".into() });
                }
            }
            for marker in ["process.env.", "os.environ", "getenv("] {
                if let Some(pos) = trimmed.find(marker) {
                    let rest = &trimmed[pos + marker.len()..];
                    let key: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    if key.len() > 2 { env_vars.insert(key); }
                }
            }

            // Findings the analyzer can be certain about from text alone.
            if trimmed.contains("TODO") || trimmed.contains("FIXME") {
                insights.push(Insight {
                    severity: "info".into(), category: "Maintenance".into(),
                    title: "Unfinished work marked in code".into(),
                    detail: trimmed.chars().take(120).collect(),
                    path: Some(rel.clone()), line: Some(ln),
                });
            }
            if looks_like_hardcoded_secret(trimmed) {
                insights.push(Insight {
                    severity: "error".into(), category: "Security".into(),
                    title: "Possible credential written into the source".into(),
                    detail: "A literal value is assigned to a key, password or token variable. Move it to an environment variable.".into(),
                    path: Some(rel.clone()), line: Some(ln),
                });
            }
            if trimmed.contains("eval(") && !rel.ends_with(".md") {
                insights.push(Insight {
                    severity: "warning".into(), category: "Security".into(),
                    title: "eval() used".into(),
                    detail: "Evaluating strings at runtime turns any injected text into code.".into(),
                    path: Some(rel.clone()), line: Some(ln),
                });
            }
        }

        files.push(FileNode { path: rel, language: lang.to_string(), lines, bytes: size });
    }

    // Project-level findings.
    let has_env_example = files.iter().any(|f| f.path.ends_with(".env.example"));
    if !env_vars.is_empty() && !has_env_example {
        insights.push(Insight {
            severity: "warning".into(), category: "Configuration".into(),
            title: "Environment variables are used but not documented".into(),
            detail: format!("{} variables are read at runtime with no .env.example to describe them.", env_vars.len()),
            path: None, line: None,
        });
    }
    if files.iter().any(|f| f.path == ".env") {
        insights.push(Insight {
            severity: "error".into(), category: "Security".into(),
            title: ".env is inside the project folder".into(),
            detail: "Confirm this file is listed in .gitignore before the project is pushed anywhere.".into(),
            path: Some(".env".into()), line: None,
        });
    }
    for f in &files {
        if f.lines > 800 {
            insights.push(Insight {
                severity: "info".into(), category: "Structure".into(),
                title: "Very large file".into(),
                detail: format!("{} lines. Large files tend to hide more than one responsibility.", f.lines),
                path: Some(f.path.clone()), line: None,
            });
        }
    }

    routes.sort(); routes.dedup();
    entry_points.sort(); entry_points.dedup();

    let analysis = Analysis {
        root: root.clone(),
        technologies: sorted(technologies),
        entry_points,
        routes,
        api_calls: sorted(api_calls),
        database_hints: sorted(database_hints),
        env_vars: sorted(env_vars),
        dependencies,
        language_totals,
        file_count: files.len(),
        files,
        edges,
        insights,
        skipped_count: skipped,
        scanned_at: crate::now(),
    };

    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "ANALYZER", "Project scanned",
        Some(&format!("files={} root={}", analysis.file_count, root)));
    logging::activity(&conn, "analyze", "Code analysis", &root);
    conn.execute("UPDATE projects SET last_scan = ?1 WHERE path = ?2",
        rusqlite::params![crate::now(), root])?;
    Ok(analysis)
}

fn sorted(set: HashSet<String>) -> Vec<String> {
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort();
    v
}

fn import_target(line: &str) -> Option<String> {
    for marker in ["from \"", "from '", "require(\"", "require('", "import \"", "import '"] {
        if let Some(i) = line.find(marker) {
            let rest = &line[i + marker.len()..];
            let quote = marker.chars().last().unwrap();
            if let Some(end) = rest.find(quote) {
                return Some(rest[..end].to_string());
            }
        }
    }
    None
}

fn normalise_rel(from: &str, target: &str) -> String {
    let dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut parts: Vec<&str> = if dir.is_empty() { vec![] } else { dir.split('/').collect() };
    for seg in target.split('/') {
        match seg {
            "." | "" => {}
            ".." => { parts.pop(); }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn looks_like_hardcoded_secret(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let named = ["password =", "password=", "api_key =", "api_key=", "apikey=",
                 "secret =", "secret=", "token =", "token=", "private_key="];
    if !named.iter().any(|n| lower.contains(n)) { return false }
    // Ignore references to config rather than literals.
    if lower.contains("process.env") || lower.contains("os.environ") || lower.contains("getenv") { return false }
    // Require a quoted literal of meaningful length.
    line.split(['"', '\'']).any(|chunk| chunk.len() >= 8 && chunk.chars().any(|c| c.is_ascii_alphanumeric()))
}
