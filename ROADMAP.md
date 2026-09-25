# DevWorkstation — Roadmap

This file is the honest other half of the README. It lists everything in the
specification that **v0.1 does not do yet**, why, and the concrete plug-in point
where it should be added. Each item names the crate/approach I would use, so the
work is a known quantity rather than an open question.

Nothing here is a hidden stub in the shipped UI. Where a capability is missing,
the relevant view says so in plain language instead of pretending.

---

## 1. Server Center — real SSH / SFTP sessions

**Now:** server profiles, TCP reachability probe, credentials in the OS vault.
**Missing:** actual command execution, file transfer, log tailing, service control.

- Crate: `russh` + `russh-keys` (pure Rust, no OpenSSL, works with the existing
  rustls stack) or `ssh2` if bundling libssh2 is acceptable.
- Plug-in point: `src-tauri/src/workspace.rs` — add `ssh.rs` beside it holding a
  `SessionPool` in Tauri state keyed by `server_id`.
- Commands to add: `ssh_connect`, `ssh_exec`, `ssh_disconnect`, `sftp_list`,
  `sftp_get`, `sftp_put`, `sftp_delete`, `remote_stat` (CPU/RAM/disk via
  `/proc` reads rather than parsing `top`).
- Secrets: read through `vault::read_secret()` only. Never return the key or
  password to the frontend — the existing vault design already enforces this.
- Spec §22 requires explicit user action per remote command. Keep a confirm step
  for anything that writes, deletes, or restarts.

## 2. Server Center — cPanel / WHM API

- Transport: `reqwest` (already a dependency) against cPanel UAPI
  `https://host:2083/execute/<Module>/<Function>` with an
  `Authorization: cpanel user:APITOKEN` header.
- Plug-in point: new `src-tauri/src/cpanel.rs`; token lives in the vault under a
  `cpanel:<server_id>` reference.
- First functions worth wiring: `Quota::get_quota_info`, `DomainInfo::list_domains`,
  `Mysql::list_databases`, `Cron::list_lines`, `SSL::installed_hosts`.

## 3. Database Lab — PostgreSQL, MySQL/MariaDB, SQL Server

**Now:** SQLite only, but the full workflow (connect → backup → analyze →
diagnose → plan → token-confirmed apply → verify) is real and works end to end.

- Postgres + MySQL: `sqlx` with `runtime-tokio-rustls` and the `postgres`,
  `mysql` features. SQL Server: `tiberius`.
- Plug-in point: `src-tauri/src/dblab.rs` is deliberately written as
  *analysis functions over an introspection result*. Introduce a
  `trait Introspect { fn tables(&self) -> …; fn indexes(&self) -> …; … }`,
  move the current SQLite code behind it, and the diagnosis rules
  (`NO_PRIMARY_KEY`, `ORPHAN_ROWS`, `DUPLICATE_INDEX`, `MISSING_INDEX`) carry over
  unchanged.
- Backups: `pg_dump`/`mysqldump` are not bundled, so for remote engines the
  backup step must either use a server-side dump over SSH (see §1) or the app
  must refuse to apply repairs — which is the correct default. The
  `db_apply_repair` gate already requires a backup to exist.

## 4. PDF Studio — OCR

- Approach: bundle Tesseract as a sidecar binary (`tauri.conf.json` →
  `bundle.externalBin`) plus `eng.traineddata`, and shell to it from Rust; or
  link `leptess` if you are willing to build Leptonica/Tesseract on the Windows
  runner.
- Plug-in point: `pdfstudio.rs` already classifies every page and sets
  `needs_ocr`. Add `pdf_ocr_pages(path, pages)` that rasterizes with `pdfium-render`
  and feeds each page image to the engine.
- Spec §29 forbids making the end user install anything, so the sidecar must be
  bundled — do not shell out to a system Tesseract.

## 5. PDF Studio — in-place text editing, annotation, compression

Editing text inside an arbitrary PDF is genuinely hard: text is positioned glyph
runs, not paragraphs. Realistic order of work:

1. **Annotation layer first** — highlights, shapes, signatures, freehand as new
   annotation objects appended with `lopdf`. Low risk, high value, no reflow.
2. **Add / delete text boxes** — new content streams, existing text untouched.
3. **Replace a text run** — only when the font is embedded with a usable
   encoding; refuse clearly otherwise rather than corrupting the file.
4. **Reflowing paragraph editing** — out of scope; say so in the UI.

Compression: re-encode images via `image` + recompress streams with
`flate2`; keep it as "Compress (lossy images)" with a preview of the size delta.

## 6. Code Analyzer — real parsing

**Now:** regex/heuristic detection of routes, APIs, DB calls, env vars, imports.
This is fast and works on thousands of files, but it is approximate.

- Upgrade: `tree-sitter` with `tree-sitter-{javascript,typescript,tsx,python,php,java,rust,go}`.
  Grammars compile into the binary — no runtime dependency, which keeps §29 satisfied.
- Plug-in point: `analyzer.rs::scan_file()`. Keep the heuristic path as a fallback
  for languages with no grammar loaded.
- This is what unlocks the §11 questions ("where does login happen?", "what
  happens when this button is clicked?") with real call-graph edges instead of
  import edges.

## 7. Test Lab — OS-level sandboxing (spec §14)

**Now:** allow-listed programs, cleared environment, `npm_config_ignore_scripts=true`,
null stdin, working directory pinned to the project, wall-clock timeout kill.
This is containment, not isolation, and the UI says exactly that.

- Windows: create the child in a **Job Object** with
  `JOB_OBJECT_LIMIT_ACTIVE_PROCESS`, `JOB_OBJECT_LIMIT_JOB_MEMORY`,
  `JOB_OBJECT_LIMIT_JOB_TIME` and `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` via the
  `windows` crate, and drop the token to a **low-integrity / AppContainer** profile
  so file writes outside the project are denied by the OS rather than by policy.
- Network: deny by default in the AppContainer capability set; the existing
  "allow internet for this run" checkbox becomes a real switch instead of a label.
- Plug-in point: `testlab.rs::spawn()` — one function, one place to change.

## 8. Auto-update (spec §28)

- `tauri-plugin-updater`. Generate a keypair with `npm run tauri signer generate`,
  put the public key in `tauri.conf.json` → `plugins.updater.pubkey`, publish
  `latest.json` + the signed NSIS bundle to a static endpoint (GitHub Releases is
  fine).
- The private key and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` belong in CI secrets —
  never in the repo.
- Pair it with Authenticode signing of the installer itself (`bundle.windows.certificateThumbprint`)
  so SmartScreen stops warning users.

## 9. Git integration (spec §31)

- Crate: `git2` (libgit2, vendored — no system Git needed, which is the §29
  requirement). Covers status, branch, log, diff, commit, checkout, clone.
- Push/pull over HTTPS needs credentials → vault. Over SSH, reuse §1's key handling.
- Plug-in point: new `src-tauri/src/git.rs`; surface in Projects as a per-project
  strip (branch, ahead/behind, dirty count) and a History tab.

## 10. Self-test suite (spec §32)

- Rust: `#[cfg(test)]` modules per file, plus integration tests hitting the
  command layer with a temp `data_root()`. The current code is already structured
  for this — every module takes paths/handles rather than reaching for globals.
- Frontend: `vitest` + `@testing-library/react` for the views,
  `@tauri-apps/api/mocks` to fake `invoke`.
- Worth testing first, because these are the destructive paths:
  `db_apply_repair` token/backup gate, `files.rs` unzip traversal guard,
  `auth.rs` password change, `logging.rs::redact()`, `website.rs` private-IP rejection.

## 11. Optional online AI (spec §21)

Deliberately left out of v0.1 rather than half-wired, because §21's real
requirement is the consent model, not the API call.

- Design: provider + API key configured in Settings, key stored in the vault.
- Before any request, show exactly what will be sent (file list, byte count,
  a preview) and require a per-request confirmation. Default to sending
  *analysis output* — the structural summary the analyzer already produces —
  rather than raw source.
- Plug-in point: `system.rs` already exposes `is_online`; add `ai.rs` with a
  single `ai_explain(kind, payload)` command so there is one auditable egress path.

## 12. Smaller gaps

- **TLS certificate details** — Website Lab reports TLS reachability and security
  headers, but not issuer/expiry/SAN validation. Add via `rustls` + `x509-parser`
  by capturing the peer chain during handshake.
- **Broken-link crawl** — currently single-page analysis; add a bounded, polite
  crawler (depth ≤ 2, same-origin, rate-limited, respects `robots.txt`).
- **Resizable panels / split views / tabs** (§4) — the shell is single-pane with a
  nav rail. `react-resizable-panels` would slot in at `App.tsx` without touching views.
- **Architecture diagram export to PNG/SVG/PDF** (§26) — the view renders SVG
  already; serialize the node and hand it to `files.rs` to write.
- **Background task queue with cancel** (§24) — long operations currently run as
  awaited commands. Move to `tauri::async_runtime::spawn` with a task registry
  in state, emitting progress events, and a `task_cancel` command.
- **Drag-to-reorder CV sections** — reorder is by buttons today.

---

## A note on Rust compile-verification

The Rust backend was written but could not be `cargo check`-verified in the
sandbox that built it: apt's newest available Rust there is 1.75.0 (Ubuntu
Noble, Dec 2023), and current crates.io packages — `idna_adapter`, transitive
deps of `quinn`/`reqwest`, and others — now require the 2024 edition, which
that Cargo can't parse. Installing a current toolchain via `rustup` wasn't
possible either, since that sandbox's network allowlist doesn't include
`static.rust-lang.org`/`sh.rustup.rs`. The `.github/workflows/build-windows.yml`
included in this repo runs the first real compile, on a current toolchain, on
push. If it fails, the error will point at a specific crate/API mismatch to
fix — pinned versions here were chosen carefully but not compiler-verified.

## 13. Code Editor — more runnable languages

**Now:** the editor runs JavaScript (`node`), Python (`python3`/`python`),
PHP, Ruby, and Java (`javac` then `java`, assuming one top-level public class
matching the file name). HTML opens in the system browser; CSS correctly
explains it has nothing to execute standalone.

- **TypeScript** — needs a run-without-a-project story (`npx tsx file.ts` is
  the natural fit, but that means a network fetch on first use unless `tsx`
  is bundled or pre-cached; document that trade-off in the UI rather than
  silently going online).
- **C / C++** — compile with a bundled `gcc`/`clang` (MSVC's `cl.exe` on
  Windows if VS Build Tools happen to be present, but that can't be assumed)
  then run the binary, mirroring the Java compile-then-run shape already in
  `testlab.rs::run_java`.
- **Go, Rust as scripts** — `go run file.go` if a Go toolchain is present;
  `rust-script` or a scratch `cargo run` for a single `.rs` file.
- **Shell** — `.sh` via `sh`/`bash` on the allow-list (already possible in
  Terminal; wiring it into the editor's Run button is the only gap).
- Plug-in point either way: `testlab.rs::runner_for()` is a single match
  statement mapping extension → program; add a case there and, for anything
  needing a compile step, a sibling function next to `run_java`.

## Suggested order

1. Self-tests (§10) — everything after this is safer with them in place.
2. Job-object sandboxing (§7) — the one place the current design has a real
   security ceiling.
3. SSH/SFTP (§1) — unlocks Server Center, and remote DB backups after it.
4. tree-sitter (§6) — unlocks the code-explanation questions.
5. Updater + signing (§8) — needed before this is distributed to anyone else.
6. Postgres/MySQL (§3), then OCR (§4), then annotations (§5).
