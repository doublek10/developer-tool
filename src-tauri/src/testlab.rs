//! Test Lab and Integrated Terminal (spec §13, §14, §15).
//!
//! Containment model for v0.1, stated plainly so it isn't mistaken for more
//! than it is: a project's own scripts are NOT run. Test Lab executes only
//! commands from a fixed allow-list, each with its own timeout, working
//! directory, trimmed environment and no inherited stdin. That stops the most
//! common hazard — a `postinstall` or `prepare` hook in an unfamiliar project
//! running the moment you inspect it — but it is process-level containment on
//! your own user account, not a virtual machine. Section 14's CPU and memory
//! limits need an OS job object and are listed in ROADMAP.md as not yet built.

use crate::auth::{require_session, Session};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::logging;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::State;

#[derive(Serialize)]
pub struct Detection {
    pub project_type: String,
    pub package_manager: String,
    pub test_framework: String,
    pub build_system: String,
    pub available_checks: Vec<Check>,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Check {
    pub id: String,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub description: String,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub id: String,
    pub label: String,
    pub outcome: String, // passed | failed | warning | skipped
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub problems: Vec<Problem>,
}

#[derive(Serialize)]
pub struct Problem {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub cause: String,
    pub suggestion: String,
}

/// The only programs Test Lab will start. A project cannot add to this list.
const ALLOWED: &[&str] = &["npm", "npx", "node", "python", "python3", "php", "cargo", "tsc", "java", "javac", "ruby"];

fn exists(root: &Path, name: &str) -> bool {
    root.join(name).exists()
}

#[tauri::command]
pub fn test_detect(session: State<'_, Session>, root: String) -> AppResult<Detection> {
    require_session(&session)?;
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(AppError::new("PROJECT_NOT_FOUND", "That project folder could not be opened.").detail(root));
    }

    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let mut project_type = "Unknown".to_string();
    let mut package_manager = "none".to_string();
    let mut test_framework = "none".to_string();
    let mut build_system = "none".to_string();

    if exists(&path, "package.json") {
        project_type = "Node.js".into();
        package_manager = if exists(&path, "pnpm-lock.yaml") { "pnpm" }
            else if exists(&path, "yarn.lock") { "yarn" }
            else { "npm" }.into();

        let text = std::fs::read_to_string(path.join("package.json")).unwrap_or_default();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        let deps = |k: &str| json.get("dependencies").and_then(|d| d.get(k)).is_some()
            || json.get("devDependencies").and_then(|d| d.get(k)).is_some();

        if deps("next") { project_type = "Next.js".into(); build_system = "next build".into(); }
        else if deps("vite") { build_system = "vite build".into(); }
        if deps("jest") { test_framework = "Jest".into() }
        else if deps("vitest") { test_framework = "Vitest".into() }

        let scripts = json.get("scripts").and_then(|s| s.as_object()).cloned().unwrap_or_default();

        if deps("typescript") {
            checks.push(Check {
                id: "typecheck".into(), label: "Type check".into(),
                program: "npx".into(), args: vec!["--no-install".into(), "tsc".into(), "--noEmit".into()],
                description: "Compiles the project's types without emitting files.".into(),
            });
        }
        if scripts.contains_key("lint") {
            checks.push(Check {
                id: "lint".into(), label: "Lint".into(),
                program: "npm".into(), args: vec!["run".into(), "lint".into(), "--silent".into()],
                description: "Runs the project's own lint script.".into(),
            });
        }
        if scripts.contains_key("test") {
            checks.push(Check {
                id: "test".into(), label: "Tests".into(),
                program: "npm".into(), args: vec!["run".into(), "test".into(), "--silent".into()],
                description: "Runs the project's own test script.".into(),
            });
        }
        if scripts.contains_key("build") {
            checks.push(Check {
                id: "build".into(), label: "Build".into(),
                program: "npm".into(), args: vec!["run".into(), "build".into(), "--silent".into()],
                description: "Produces a production build.".into(),
            });
        }
        if !exists(&path, "node_modules") {
            warnings.push("Dependencies are not installed, so most checks will fail until you run an install.".into());
        }
    } else if exists(&path, "requirements.txt") || exists(&path, "pyproject.toml") {
        project_type = "Python".into();
        package_manager = "pip".into();
        checks.push(Check {
            id: "pysyntax".into(), label: "Syntax check".into(),
            program: "python".into(), args: vec!["-m".into(), "compileall".into(), "-q".into(), ".".into()],
            description: "Compiles every module to catch syntax errors.".into(),
        });
        checks.push(Check {
            id: "pytest".into(), label: "Tests".into(),
            program: "python".into(), args: vec!["-m".into(), "pytest".into(), "-q".into()],
            description: "Runs pytest if it is installed in the environment.".into(),
        });
        test_framework = "pytest".into();
    } else if exists(&path, "composer.json") {
        project_type = "PHP".into();
        package_manager = "composer".into();
        checks.push(Check {
            id: "phplint".into(), label: "Syntax check".into(),
            program: "php".into(), args: vec!["-l".into()],
            description: "Checks each PHP file for syntax errors.".into(),
        });
    } else if exists(&path, "Cargo.toml") {
        project_type = "Rust".into();
        package_manager = "cargo".into();
        build_system = "cargo".into();
        checks.push(Check {
            id: "cargocheck".into(), label: "Compile check".into(),
            program: "cargo".into(), args: vec!["check".into(), "--quiet".into()],
            description: "Type-checks the crate without producing a binary.".into(),
        });
        checks.push(Check {
            id: "cargotest".into(), label: "Tests".into(),
            program: "cargo".into(), args: vec!["test".into(), "--quiet".into()],
            description: "Runs the crate's test suite.".into(),
        });
    }

    if checks.is_empty() {
        warnings.push("No recognised build or test setup was found in this folder.".into());
    }

    Ok(Detection { project_type, package_manager, test_framework, build_system, available_checks: checks, warnings })
}

#[tauri::command]
pub fn test_run(
    db: State<'_, Db>,
    session: State<'_, Session>,
    root: String,
    check: Check,
    timeout_seconds: Option<u64>,
    allow_network: Option<bool>,
) -> AppResult<CheckResult> {
    require_session(&session)?;
    if !ALLOWED.contains(&check.program.as_str()) {
        return Err(AppError::new("COMMAND_NOT_ALLOWED", "Test Lab will not run that program.")
            .detail(check.program.clone())
            .recover("Only the detected build and test tools can be started from here."));
    }
    let workdir = PathBuf::from(&root);
    if !workdir.is_dir() {
        return Err(AppError::new("PROJECT_NOT_FOUND", "That project folder could not be opened."));
    }

    let run = spawn_and_capture(&check.program, &check.args, &workdir, timeout_seconds, allow_network)?;
    let problems = parse_problems(&format!("{}\n{}", run.stdout, run.stderr));

    let conn = db.0.lock().unwrap();
    logging::write(&conn, if run.outcome == "passed" { "info" } else { "error" }, "TESTLAB",
        &format!("{} {}", check.label, run.outcome),
        Some(&format!("project={root} ms={}", run.duration_ms)));

    Ok(CheckResult {
        id: check.id, label: check.label, outcome: run.outcome, exit_code: run.exit_code,
        duration_ms: run.duration_ms, stdout: run.stdout, stderr: run.stderr, problems,
    })
}

/// Result of a single contained process run, before it's shaped into
/// whichever public struct the caller (test_run / code_run_file) returns.
struct RawRun {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    outcome: String, // passed | failed
    duration_ms: u128,
}

/// Shared containment: clean environment, no stdin, piped output, a hard
/// wall-clock timeout that kills the process. Used by both Test Lab's
/// project checks and the Code Editor's single-file Run button, so the same
/// guarantees apply everywhere DevWorkstation executes someone else's code.
fn spawn_and_capture(
    program: &str,
    args: &[String],
    workdir: &Path,
    timeout_seconds: Option<u64>,
    allow_network: Option<bool>,
) -> AppResult<RawRun> {
    let started = std::time::Instant::now();
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Start from a clean environment rather than inheriting the app's.
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CI", "1")
        .env("NO_COLOR", "1")
        .env("npm_config_ignore_scripts", "true")
        .env("npm_config_audit", "false")
        .env("npm_config_fund", "false");

    if cfg!(windows) {
        for k in ["SYSTEMROOT", "COMSPEC", "TEMP", "TMP", "PATHEXT", "USERPROFILE", "APPDATA"] {
            if let Ok(v) = std::env::var(k) { cmd.env(k, v); }
        }
    }
    if !allow_network.unwrap_or(false) {
        cmd.env("npm_config_offline", "true").env("NO_PROXY", "*");
    }

    let mut child = cmd.spawn().map_err(|e| {
        AppError::new("PROGRAM_NOT_FOUND", format!("{program} is not available on this computer."))
            .detail(e.to_string())
            .recover("Install the toolchain this file needs, then run it again. DevWorkstation itself does not need it.")
    })?;

    let limit = std::time::Duration::from_secs(timeout_seconds.unwrap_or(180).clamp(5, 1800));
    let deadline = std::time::Instant::now() + limit;
    let mut timed_out = false;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                timed_out = true;
                break;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(120)),
        }
    }

    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code();
    let duration_ms = started.elapsed().as_millis();
    let outcome = if timed_out { "failed" } else if exit_code == Some(0) { "passed" } else { "failed" };

    Ok(RawRun {
        exit_code,
        stdout: truncate(stdout),
        stderr: truncate(if timed_out { format!("Stopped after {} seconds.", limit.as_secs()) } else { stderr }),
        outcome: outcome.into(),
        duration_ms,
    })
}

// ------------------------------------------------------------- code editor

/// Which interpreter/compiler runs a given file extension, for the Code
/// Editor's Run button. Java is handled separately below (compile, then
/// run, as two steps). HTML and CSS are not run as processes at all — the
/// frontend opens HTML directly in the system browser and explains that CSS
/// has nothing to execute on its own.
fn runner_for(ext: &str) -> Option<&'static str> {
    match ext {
        "js" | "mjs" | "cjs" => Some("node"),
        "py" | "pyw" => Some("python3"),
        "php" => Some("php"),
        "rb" => Some("ruby"),
        _ => None,
    }
}

/// Runs a single source file the way the Code Editor's Run button expects:
/// save-then-run, one file, no project context. Reuses the exact same
/// contained-process machinery as Test Lab's project checks (`spawn_and_capture`),
/// so the containment guarantees described at the top of this file apply
/// here too — this is not a separate, weaker execution path.
#[tauri::command]
pub fn code_run_file(
    db: State<'_, Db>,
    session: State<'_, Session>,
    path: String,
    timeout_seconds: Option<u64>,
) -> AppResult<CheckResult> {
    require_session(&session)?;
    let file = PathBuf::from(&path);
    if !file.is_file() {
        return Err(AppError::new("FILE_NOT_FOUND", "That file no longer exists.").detail(path));
    }
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    let workdir = file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));

    if ext == "html" || ext == "htm" {
        return Err(AppError::new("OPEN_INSTEAD_OF_RUN", "HTML files open in your browser instead of running as a process.")
            .recover("Use the Open in browser button next to Run for this file type."));
    }
    if ext == "css" {
        return Err(AppError::new("NO_RUNNER_FOR_EXTENSION", "A CSS file has nothing to execute on its own.")
            .recover("Open the HTML file that links this stylesheet and run that instead."));
    }

    if ext == "java" {
        return run_java(db, &file, &workdir, timeout_seconds);
    }

    let Some(program) = runner_for(&ext) else {
        return Err(AppError::new("NO_RUNNER_FOR_EXTENSION",
            format!(".{} isn't a file type DevWorkstation knows how to run yet.", if ext.is_empty() { "?" } else { ext.as_str() }))
            .recover("Open Terminal or Test Lab and run it directly with whatever tool this file needs."));
    };

    // Python ships under different program names on different systems —
    // try python3 first, then fall back to python, rather than failing on
    // a machine that only has one of the two.
    let candidates: &[&str] = if program == "python3" { &["python3", "python"] } else { &[program] };
    let mut last_err = None;
    for (i, candidate) in candidates.iter().enumerate() {
        match spawn_and_capture(candidate, &[path.clone()], &workdir, timeout_seconds, Some(false)) {
            Ok(run) => return finish_run(db, "Run", &name, run),
            Err(e) if i + 1 < candidates.len() => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| AppError::new("PROGRAM_NOT_FOUND", "No runnable interpreter was found.")))
}

/// Java needs a compile step before it can run. The class name is assumed
/// to match the file's name, which is true for any single top-level public
/// class — the common case for a file opened straight in the editor.
fn run_java(db: State<'_, Db>, file: &Path, workdir: &Path, timeout_seconds: Option<u64>) -> AppResult<CheckResult> {
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("Main").to_string();
    let file_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file.java").to_string();

    let compile = spawn_and_capture("javac", &[file_name.clone()], workdir, timeout_seconds, Some(false))?;
    if compile.outcome != "passed" {
        let problems = parse_problems(&format!("{}\n{}", compile.stdout, compile.stderr));
        let conn = db.0.lock().unwrap();
        logging::write(&conn, "error", "TESTLAB", "Compile failed", Some(&file_name));
        return Ok(CheckResult {
            id: "run".into(), label: format!("Compile {file_name}"), outcome: "failed".into(),
            exit_code: compile.exit_code, duration_ms: compile.duration_ms,
            stdout: compile.stdout, stderr: compile.stderr, problems,
        });
    }

    let run = spawn_and_capture("java", &[stem], workdir, timeout_seconds, Some(false))?;
    finish_run(db, "Run", &file_name, run)
}

fn finish_run(db: State<'_, Db>, label: &str, target: &str, run: RawRun) -> AppResult<CheckResult> {
    let problems = parse_problems(&format!("{}\n{}", run.stdout, run.stderr));
    let conn = db.0.lock().unwrap();
    logging::write(&conn, if run.outcome == "passed" { "info" } else { "error" }, "TESTLAB",
        &format!("{label} {}", run.outcome), Some(target));
    Ok(CheckResult {
        id: "run".into(), label: label.into(), outcome: run.outcome, exit_code: run.exit_code,
        duration_ms: run.duration_ms, stdout: run.stdout, stderr: run.stderr, problems,
    })
}

fn truncate(s: String) -> String {
    if s.len() > 60_000 { format!("{}\n… output truncated", &s[..60_000]) } else { s }
}

/// Recognises the common `path:line:col: message` shapes emitted by tsc,
/// eslint, python and php, and turns each into an actionable row.
fn parse_problems(output: &str) -> Vec<Problem> {
    let mut out = Vec::new();
    for line in output.lines().take(4000) {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue }
        let parts: Vec<&str> = trimmed.splitn(4, ':').collect();
        if parts.len() >= 3 {
            if let Ok(ln) = parts[1].trim().parse::<u32>() {
                let col = parts.get(2).and_then(|c| c.trim().parse::<u32>().ok());
                let message = parts.get(3).map(|s| s.trim().to_string())
                    .unwrap_or_else(|| parts[2].trim().to_string());
                if message.is_empty() { continue }
                let (cause, suggestion) = explain(&message);
                out.push(Problem {
                    file: parts[0].trim().to_string(),
                    line: Some(ln), column: col, message, cause, suggestion,
                });
                if out.len() >= 300 { break }
            }
        }
    }
    out
}

fn explain(message: &str) -> (String, String) {
    let m = message.to_ascii_lowercase();
    if m.contains("cannot find module") || m.contains("module not found") {
        ("An import points at something that isn't installed or isn't where the path says.".into(),
         "Check the spelling of the path, then confirm the package is listed in the manifest and installed.".into())
    } else if m.contains("is not assignable to type") || m.contains("type ") && m.contains("error") {
        ("The value's shape doesn't match the type the code expects here.".into(),
         "Either widen the type or convert the value before it reaches this line.".into())
    } else if m.contains("is declared but") || m.contains("unused") {
        ("Something is defined and never used.".into(), "Remove it, or use it where it was intended.".into())
    } else if m.contains("unexpected token") || m.contains("syntax error") {
        ("The parser hit something it can't read as code at this position.".into(),
         "Look for an unclosed bracket, quote or brace just before this line.".into())
    } else if m.contains("econnrefused") || m.contains("connection refused") {
        ("Something the test needs is not listening on the address it tried.".into(),
         "Start the service, or point the test at the right host and port.".into())
    } else {
        ("Reported by the tool that ran this check.".into(),
         "Open the file at this line to see the surrounding context.".into())
    }
}

// ---------------------------------------------------------------- terminal

#[derive(Serialize)]
pub struct ShellResult {
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub cwd: String,
}

/// Single-shot execution, not an interactive session. The command runs in the
/// shell the user already has, so it can do whatever that user can do — the
/// interface says so before the first run.
#[tauri::command]
pub fn terminal_run(
    db: State<'_, Db>,
    session: State<'_, Session>,
    command: String,
    cwd: String,
) -> AppResult<ShellResult> {
    require_session(&session)?;
    if command.trim().is_empty() {
        return Err(AppError::new("EMPTY_COMMAND", "Type a command first."));
    }
    let dir = PathBuf::from(&cwd);
    if !dir.is_dir() {
        return Err(AppError::new("FOLDER_NOT_FOUND", "The working folder no longer exists.").detail(cwd));
    }

    let started = std::time::Instant::now();
    let output = if cfg!(windows) {
        Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &command])
            .current_dir(&dir).stdin(Stdio::null()).output()?
    } else {
        Command::new("sh").arg("-c").arg(&command)
            .current_dir(&dir).stdin(Stdio::null()).output()?
    };

    let conn = db.0.lock().unwrap();
    logging::write(&conn, "info", "TERMINAL", "Command run", Some(&command));

    Ok(ShellResult {
        command,
        exit_code: output.status.code(),
        stdout: truncate(String::from_utf8_lossy(&output.stdout).to_string()),
        stderr: truncate(String::from_utf8_lossy(&output.stderr).to_string()),
        duration_ms: started.elapsed().as_millis(),
        cwd: dir.to_string_lossy().to_string(),
    })
}
