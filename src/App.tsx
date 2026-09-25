import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react";
import { call, describe, DashboardData, LogLine, SessionUser } from "./lib/api";
import SignIn from "./views/SignIn";
import Register from "./views/Register";
import Dashboard from "./views/Dashboard";
import CvBuilder from "./views/CvBuilder";
import PdfStudio from "./views/PdfStudio";
import WebsiteLab from "./views/WebsiteLab";
import ServerCenter from "./views/ServerCenter";
import CodeAnalyzer from "./views/CodeAnalyzer";
import { Busy } from "./components/ui";
const CodeEditor = lazy(() => import("./views/CodeEditor"));
import DatabaseLab from "./views/DatabaseLab";
import TestLab from "./views/TestLab";
import Terminal from "./views/Terminal";
import Projects from "./views/Projects";
import Files from "./views/Files";
import Vault from "./views/Vault";
import Logs from "./views/Logs";
import Settings from "./views/Settings";

type ViewId =
  | "dashboard" | "cv" | "pdf" | "website" | "servers" | "analyzer" | "editor"
  | "database" | "tests" | "terminal" | "projects" | "files"
  | "vault" | "logs" | "settings";

interface NavItem { id: ViewId; label: string; group: string; needsNet?: boolean }

const NAV: NavItem[] = [
  { id: "dashboard", label: "Dashboard", group: "" },
  { id: "cv", label: "CV builder", group: "Create" },
  { id: "pdf", label: "PDF studio", group: "Create" },
  { id: "editor", label: "Code editor", group: "Create" },
  { id: "analyzer", label: "Code analyzer", group: "Analyze" },
  { id: "database", label: "Database lab", group: "Analyze" },
  { id: "tests", label: "Test lab", group: "Analyze" },
  { id: "website", label: "Website lab", group: "Analyze", needsNet: true },
  { id: "projects", label: "Projects", group: "Manage" },
  { id: "servers", label: "Servers", group: "Manage", needsNet: true },
  { id: "files", label: "Files", group: "Manage" },
  { id: "terminal", label: "Terminal", group: "Manage" },
  { id: "vault", label: "Vault", group: "System" },
  { id: "logs", label: "Logs", group: "System" },
  { id: "settings", label: "Settings", group: "System" },
];

export default function App() {
  const [user, setUser] = useState<SessionUser | null>(null);
  const [hasAccount, setHasAccount] = useState<boolean | null>(null);
  const [view, setView] = useState<ViewId>("dashboard");
  const [theme, setTheme] = useState<"dark" | "light">(
    () => (localStorage.getItem("dw.theme") as "dark" | "light") ?? "dark"
  );
  const [online, setOnline] = useState(true);
  const [tail, setTail] = useState<LogLine | null>(null);
  const [summary, setSummary] = useState<DashboardData | null>(null);
  const [palette, setPalette] = useState(false);
  const [openProject, setOpenProject] = useState<string>("");
  const [editorFile, setEditorFile] = useState<string>("");

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("dw.theme", theme);
  }, [theme]);

  // First run on this computer has no account yet, so it goes to Register
  // instead of Sign in.
  useEffect(() => {
    call<boolean>("has_account")
      .then(setHasAccount)
      .catch(() => setHasAccount(true));
  }, []);

  const refreshStatus = useCallback(async () => {
    if (!user) return;
    try {
      const [lines, net] = await Promise.all([
        call<LogLine[]>("logs_query", { limit: 1 }),
        call<boolean>("is_online"),
      ]);
      setTail(lines[0] ?? null);
      setOnline(net);
    } catch {
      /* the status strip never interrupts work */
    }
  }, [user]);

  useEffect(() => {
    if (!user) return;
    refreshStatus();
    const t = setInterval(refreshStatus, 6000);
    return () => clearInterval(t);
  }, [user, refreshStatus]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
      }
      if (e.key === "Escape") setPalette(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const signOut = async () => {
    await call("sign_out").catch(() => undefined);
    setUser(null);
    setView("dashboard");
  };

  const grouped = useMemo(() => {
    const out: Record<string, NavItem[]> = {};
    for (const item of NAV) (out[item.group] ??= []).push(item);
    return out;
  }, []);

  if (!user) {
    if (hasAccount === null) return <Busy label="Starting…" />;
    return hasAccount
      ? <SignIn onSignedIn={setUser} theme={theme} />
      : <Register onRegistered={(u) => { setUser(u); setHasAccount(true); }} theme={theme} />;
  }

  const openInAnalyzer = (path: string) => {
    setOpenProject(path);
    setView("analyzer");
  };

  const openInEditor = (path: string) => {
    setEditorFile(path);
    setView("editor");
  };

  return (
    <div className="h-full flex flex-col bg-bg text-body">
      <div className="flex-1 flex min-h-0">
        {/* navigation rail */}
        <nav className="w-[208px] shrink-0 bg-ink border-r border-line flex flex-col">
          <div className="h-12 px-3.5 flex items-center border-b border-line">
            <div className="w-[18px] h-[3px] rounded-sm mr-2.5" style={{ background: "var(--accent)" }} />
            <span className="text-[13.5px] font-semibold tracking-tight">DevWorkstation</span>
          </div>

          <div className="flex-1 overflow-auto py-2">
            {Object.entries(grouped).map(([group, items]) => (
              <div key={group} className="mb-1.5">
                {group && (
                  <div className="px-3.5 py-1.5 text-[11px] text-muted">{group}</div>
                )}
                {items.map((item) => {
                  const active = view === item.id;
                  const dimmed = item.needsNet && !online;
                  return (
                    <button
                      key={item.id}
                      onClick={() => setView(item.id)}
                      className="w-full text-left px-3.5 py-[7px] text-[13px] flex items-center gap-2 border-l-2"
                      style={{
                        borderLeftColor: active ? "var(--accent)" : "transparent",
                        background: active ? "var(--surface)" : "transparent",
                        color: active ? "var(--text)" : dimmed ? "var(--muted)" : "var(--text)",
                        opacity: dimmed && !active ? 0.6 : 1,
                      }}
                    >
                      <span className="truncate">{item.label}</span>
                      {dimmed && <span className="ml-auto text-[10.5px]">offline</span>}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>

          <div className="border-t border-line p-2.5">
            <div className="text-[12.5px] truncate">{user.display_name || user.username}</div>
            <div className="mono text-[11px] text-muted truncate">{user.username}</div>
            <div className="flex gap-1.5 mt-2">
              <button className="btn flex-1 !py-1 !text-[12px]" onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
                {theme === "dark" ? "Light" : "Dark"}
              </button>
              <button className="btn flex-1 !py-1 !text-[12px]" onClick={signOut}>Sign out</button>
            </div>
          </div>
        </nav>

        {/* work area */}
        <main className="flex-1 min-w-0 flex flex-col">
          <header className="h-12 shrink-0 border-b border-line flex items-center gap-3 px-4 bg-surface">
            <h1 className="text-[14px] font-semibold">
              {NAV.find((n) => n.id === view)?.label}
            </h1>
            <button
              className="ml-auto btn !py-1 !text-[12px] text-muted"
              onClick={() => setPalette(true)}
            >
              Go to… <span className="mono ml-1.5 text-[11px]">Ctrl K</span>
            </button>
          </header>

          <div className="flex-1 min-h-0 overflow-auto p-4">
            {view === "dashboard" && (
              <Dashboard
                user={user}
                onNavigate={(v) => setView(v as ViewId)}
                onSummary={setSummary}
              />
            )}
            {view === "cv" && <CvBuilder />}
            {view === "pdf" && <PdfStudio />}
            {view === "website" && <WebsiteLab online={online} />}
            {view === "servers" && <ServerCenter online={online} />}
            {view === "analyzer" && <CodeAnalyzer initialPath={openProject} onOpenInEditor={openInEditor} />}
            {view === "editor" && (
              <Suspense fallback={<Busy label="Loading editor…" />}>
                <CodeEditor initialPath={editorFile} />
              </Suspense>
            )}
            {view === "database" && <DatabaseLab />}
            {view === "tests" && <TestLab />}
            {view === "terminal" && <Terminal />}
            {view === "projects" && <Projects onAnalyze={openInAnalyzer} />}
            {view === "files" && <Files onAnalyze={openInAnalyzer} onEdit={openInEditor} />}
            {view === "vault" && <Vault />}
            {view === "logs" && <Logs />}
            {view === "settings" && <Settings user={user} onUser={setUser} summary={summary} />}
          </div>
        </main>
      </div>

      {/* status strip — the one live element on screen */}
      <footer className="h-[26px] shrink-0 bg-ink border-t border-line flex items-center gap-3 px-3 text-[11.5px]">
        <span className="flex items-center gap-1.5 shrink-0">
          <span
            className={online ? "live-dot w-1.5 h-1.5 rounded-full" : "w-1.5 h-1.5 rounded-full"}
            style={{ background: online ? "var(--ok)" : "var(--muted)" }}
          />
          <span className="text-muted">{online ? "Online" : "Offline"}</span>
        </span>
        <span className="text-line">|</span>
        {tail ? (
          <span className="mono truncate text-muted min-w-0">
            <span style={{ color: tail.level === "error" ? "var(--err)" : tail.level === "warn" ? "var(--warn)" : "var(--muted)" }}>
              {tail.at.slice(11)}
            </span>{" "}
            {tail.module} — {tail.message}
          </span>
        ) : (
          <span className="text-muted">Ready</span>
        )}
        <button className="ml-auto text-muted hover:text-body shrink-0" onClick={() => setView("logs")}>
          Logs
        </button>
      </footer>

      {palette && (
        <CommandPalette
          items={NAV}
          onPick={(id) => { setView(id); setPalette(false); }}
          onClose={() => setPalette(false)}
        />
      )}
    </div>
  );
}

function CommandPalette({
  items, onPick, onClose,
}: { items: NavItem[]; onPick: (id: ViewId) => void; onClose: () => void }) {
  const [q, setQ] = useState("");
  const matches = items.filter((i) => i.label.toLowerCase().includes(q.toLowerCase()));
  return (
    <div
      className="fixed inset-0 flex items-start justify-center pt-[16vh] px-4"
      style={{ background: "rgba(0,0,0,.45)" }}
      onClick={onClose}
    >
      <div className="panel w-full max-w-[460px]" onClick={(e) => e.stopPropagation()}>
        <input
          autoFocus
          className="field !border-0 !rounded-none !border-b !border-line !py-2.5"
          placeholder="Go to a module"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && matches[0]) onPick(matches[0].id); }}
        />
        <div className="max-h-[320px] overflow-auto py-1">
          {matches.length === 0 && (
            <p className="px-3 py-3 text-[12.5px] text-muted">Nothing matches that.</p>
          )}
          {matches.map((m) => (
            <button
              key={m.id}
              className="w-full text-left px-3 py-1.5 text-[13px] hover:bg-raised flex items-center gap-2"
              onClick={() => onPick(m.id)}
            >
              {m.label}
              {m.group && <span className="ml-auto text-[11px] text-muted">{m.group}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
