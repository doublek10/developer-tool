# DevWorkstation

An offline-first developer workstation for Windows: CV builder, PDF studio,
code analyzer, database lab, test lab, terminal, file manager, project
register and a credential vault, in one application.

Built with Tauri 2, React, TypeScript, Tailwind and Rust. SQLite is compiled
into the binary. The person who installs it needs no Python, Node, Rust, PHP,
Java, Git or database server for the application itself to run.

---

## Your login and activation

The first time DevWorkstation starts on a computer, there is no account yet —
it shows a **Register** screen instead of Sign in, asking for a username, a
password and an **activation key** (the one issued at purchase).

Registration requires internet: the key is sent once to the licensing API
(`POST /v2/verify.php`) and checked against the `keyactive` table. If it comes
back paid, the account is created on that computer and you're signed straight
in. If it doesn't, the app opens the purchase page
(`https://devworkstation-website.vercel.app/activationkey`) instead of
creating the account, so you can buy or fix the key and try again.

After that first run, every sign-in re-checks the same stored key **only if
the computer is online at that moment**; signing in with no internet skips
the check and goes straight to the dashboard. None of this is sent anywhere
except the verification endpoint — the password is hashed with Argon2id, and
only the hash is written to the local SQLite database, so there is no
password reset: delete `%APPDATA%\DevWorkstation\database\` to start over
(this also forgets the stored activation key, so registration runs again).

Settings → Password and Settings → Account let you change the password and
username afterward.

---

## Get the installer (no setup on your machine)

The repo includes `.github/workflows/build-windows.yml`, which builds the real
`.exe` on GitHub's own Windows runner — current Rust, MSVC linker, everything
Tauri needs. You don't install anything locally for this path.

1. Push this folder to a GitHub repo (a free private repo is fine).
2. Actions tab → **Build Windows installer** → **Run workflow**.
3. Wait ~5-10 minutes, open the finished run, download the
   **DevWorkstation-Setup** artifact at the bottom of the page.
4. Unzip it — that's `DevWorkstation-Setup.exe`. Run it, click through
   SmartScreen's "unknown publisher" warning (More info → Run anyway; this
   goes away once you sign it, see below), and sign in with the credentials
   above.

It also builds automatically on every push to `main` and on tags like `v0.1.0`.

## Build it yourself instead

If you'd rather build locally, you need this once, on a Windows 10/11 machine:

1. **Rust** — https://rustup.rs (choose the MSVC toolchain)
2. **Visual Studio Build Tools** with "Desktop development with C++"
   (`rusqlite` and `printpdf` compile C, so this is required)
3. **Node.js 18+** — https://nodejs.org
4. **WebView2** — already present on Windows 11 and updated Windows 10

Then:

```bash
npm install
npm run app:dev      # run it with hot reload
npm run app:build    # produce the installer
```

The installer lands at:

    src-tauri/target/release/bundle/nsis/DevWorkstation_0.1.0_x64-setup.exe

That file is the whole product. It installs the app, its icons, a Start Menu
entry, and creates the data folder and SQLite database on first launch.

Either way you get the identical installer — the workflow just runs the same
`npm run app:build` on a clean Windows machine for you.

**Code signing (optional).** Unsigned, so SmartScreen warns on first run —
harmless, just click through it. To remove the warning you need a code-signing
certificate (from DigiCert, Sectigo, etc. — this costs money and isn't
something I can generate for you). Once you have one:

- Building locally: add to `tauri.conf.json`:
  ```json
  "windows": {
    "certificateThumbprint": "YOUR_THUMBPRINT",
    "digestAlgorithm": "sha256",
    "timestampUrl": "http://timestamp.digicert.com"
  }
  ```
- Building via the GitHub Actions workflow: add `TAURI_SIGNING_PRIVATE_KEY`
  and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as repo secrets (Settings → Secrets
  and variables → Actions) — the workflow already checks for them and signs
  automatically when present.

Automatic updates (spec §28) need a signed update endpoint; see ROADMAP.md.

---

## Where your data lives

    %APPDATA%\DevWorkstation\
      database\workstation.sqlite3   projects, servers, CVs, logs, settings
      logs\  cache\  temp\  projects\  templates\  resources\  engines\

Credentials are **not** in that folder. They go to Windows Credential Manager
through the `keyring` crate. The SQLite row for a server or database holds only
a reference string like `server:3:password`; the value is fetched from the
keystore at the moment a connection opens, and no command returns it to the
interface.

Log writes pass through a redactor that strips anything following `password=`,
`token:`, `api_key=` and similar, plus PEM key blocks.

---

## What each module does today

| Module | State |
|---|---|
| **Dashboard** | Live CPU and memory, activity feed, recent errors, storage location |
| **CV builder** | Full CRUD, multiple CVs, reorderable sections, font/size/spacing/margin/colour controls, PDF export typeset in Rust — fully offline |
| **PDF studio** | Inspect, per-page text/image/scan classification, text extraction, delete/rotate/extract pages, merge documents. Originals are never overwritten |
| **Code analyzer** | Scans a tree, detects technologies and dependencies, finds entry points, routes, API calls, database use and environment variables, maps import edges, flags hardcoded secrets, `eval`, oversized files and undocumented config. Files open straight into the Code editor |
| **Code editor** | VS Code-style file tree, tabs, and syntax-highlighted editing (JS/TS, Python, HTML, CSS, Java, PHP, Ruby and more) via CodeMirror 6. **Run** saves the file and executes it through the same sandboxed process runner as Test Lab — JavaScript, Python, PHP, Ruby and Java (compiled then run); HTML opens in your default browser; CSS explains it has nothing to run on its own |
| **Database lab** | SQLite: schema, columns, indexes, foreign keys, integrity check; finds orphan rows, duplicate indexes, missing indexes, tables with no primary key; backup → review SQL → approve → apply → verify |
| **Test lab** | Detects Node/Next/Python/PHP/Rust projects, offers the right checks, runs them with install hooks off and a timeout, parses errors into file, line, cause and suggested fix |
| **Terminal** | One-shot PowerShell execution with `cd` tracking, history, and `dev *` shortcuts into the modules |
| **Website lab** | DNS, TCP, HTTP, redirect chain, timings, protection headers, cookie flags, CORS, page structure, graded findings, JSON export |
| **Files** | Browse, preview and edit text, rename, delete, copy, zip, unzip, search, hand a folder to the analyzer or a file to the editor |
| **Projects** | Register folders with technology, repo, server, database, status, notes, scan history |
| **Servers** | Profiles with credentials in the vault, plus reachability checks |
| **Vault** | Save, verify and remove credentials in the OS keystore |
| **Logs** | Filter by module, level and text; follow live; export |
| **Settings** | Change password and username, see exactly where data is kept |

## Session persistence

Most modules remember what you were doing — the open folder, the last
analysis, an in-progress Database Lab repair, unsaved Code Editor tabs,
Terminal history — and restore it the next time you open that module,
including after fully closing and reopening DevWorkstation. This is separate
from the SQLite database: it's local browser storage inside the app, meant
for in-progress work rather than things that already have their own Save
button. Credentials, passwords and vault secrets are never included — those
only ever live in Windows Credential Manager.

**Not built yet:** live SSH/SFTP sessions, cPanel API calls, PostgreSQL/MySQL/
SQL Server drivers, OCR, PDF text editing in place, the auto-updater, and true
OS-level sandboxing for Test Lab. Each one is written up in ROADMAP.md with the
crate to use and where in this codebase it plugs in. Nothing in the interface
pretends these work.

---

## Layout

```
src/                    React interface
  App.tsx               shell, navigation, command palette, status strip
  lib/api.ts            typed bridge to every Rust command
  components/ui.tsx     panels, error notices, shared primitives
  views/                one file per module
src-tauri/src/
  main.rs               command registration
  auth.rs               Argon2id login, password and username changes
  db.rs                 SQLite schema, migrations, data folder layout
  vault.rs              OS keystore
  logging.rs            structured logs with secret redaction
  error.rs              AppError: code, message, detail, recovery
  cv.rs  pdfstudio.rs  website.rs  analyzer.rs
  dblab.rs  testlab.rs  files.rs  workspace.rs  system.rs
```

Every module is an independent engine behind one shell, so adding another means
a new `src-tauri/src/*.rs`, a line in `main.rs`, and a new `src/views/*.tsx`.
