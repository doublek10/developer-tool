import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { call, describe, AppError, CheckResult, FsEntry } from "../lib/api";
import { Busy, Empty, ErrorNotice, Severity } from "../components/ui";
import CodeMirrorEditor from "../components/CodeMirrorEditor";
import { usePersistedState } from "../lib/persist";

interface Tab {
  path: string;
  name: string;
  content: string;
  saved: string; // last-saved (on-disk) content, to know if dirty and to revert to
}

const RUNNABLE = new Set(["js", "mjs", "cjs", "py", "pyw", "php", "rb", "java"]);
const OPENS_INSTEAD = new Set(["html", "htm"]);

const extOf = (name: string) => {
  const i = name.lastIndexOf(".");
  return i === -1 ? "" : name.slice(i + 1).toLowerCase();
};

export default function CodeEditor({
  initialPath, initialRoot,
}: { initialPath?: string; initialRoot?: string }) {
  // Everything below is restored automatically the next time this page
  // opens — including this session's app restart — via usePersistedState.
  // Open tabs keep their unsaved content too, not just which files were open.
  const [root, setRoot] = usePersistedState<string>("editor.root", initialRoot ?? "");
  const [expandedDirs, setExpandedDirs] = usePersistedState<string[]>("editor.expandedDirs", []);
  const [tabs, setTabs] = usePersistedState<Tab[]>("editor.tabs", []);
  const [active, setActive] = usePersistedState<string | null>("editor.active", null);

  const [tree, setTree] = useState<Record<string, FsEntry[]>>({});
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);

  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CheckResult | null>(null);
  const [runError, setRunError] = useState<AppError | null>(null);
  const [outputOpen, setOutputOpen] = useState(true);

  const openedInitial = useRef(false);
  const restored = useRef(false);

  // Restores the folder tree for whatever was open last time, without
  // resetting the session (openRoot below is the "start fresh" path).
  useEffect(() => {
    (async () => {
      if (restored.current) return;
      restored.current = true;
      if (root) {
        await loadDir(root);
        for (const dir of expandedDirs) if (dir !== root) await loadDir(dir);
      } else {
        const base = initialRoot || (await call<string>("fs_home").catch(() => ""));
        if (base) await openRoot(base);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (initialPath && !openedInitial.current) {
      openedInitial.current = true;
      openFile(initialPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialPath]);

  /** Fetches and caches one folder's listing without touching root/expansion state. */
  const loadDir = async (path: string) => {
    setError(null);
    try {
      const entries = await call<FsEntry[]>("fs_list", { path });
      setTree((t) => ({ ...t, [path]: entries }));
    } catch (e) { setError(describe(e)); }
  };

  /** Starts a new session at this folder — used when the user explicitly picks one. */
  const openRoot = async (path: string) => {
    setError(null);
    try {
      const entries = await call<FsEntry[]>("fs_list", { path });
      setTree((t) => ({ ...t, [path]: entries }));
      setRoot(path);
      setExpandedDirs([path]);
    } catch (e) { setError(describe(e)); }
  };

  const pickFolder = async () => {
    const chosen = await openDialog({ directory: true, multiple: false });
    if (typeof chosen === "string") await openRoot(chosen);
  };

  const toggleDir = async (path: string) => {
    setExpandedDirs((prev) => (prev.includes(path) ? prev.filter((p) => p !== path) : [...prev, path]));
    if (!tree[path]) await loadDir(path);
  };

  const openFile = async (path: string) => {
    const existing = tabs.find((t) => t.path === path);
    if (existing) { setActive(path); return; }
    setBusy(true); setError(null);
    try {
      const content = await call<string>("fs_read_text", { path });
      const name = path.split(/[\\/]/).pop() ?? path;
      setTabs((t) => [...t, { path, name, content, saved: content }]);
      setActive(path);
      setResult(null); setRunError(null);
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const activeTab = useMemo(() => tabs.find((t) => t.path === active) ?? null, [tabs, active]);
  const dirty = activeTab ? activeTab.content !== activeTab.saved : false;

  const updateContent = (next: string) => {
    if (!active) return;
    setTabs((ts) => ts.map((t) => (t.path === active ? { ...t, content: next } : t)));
  };

  const closeTab = (path: string) => {
    setTabs((ts) => ts.filter((t) => t.path !== path));
    if (active === path) {
      const remaining = tabs.filter((t) => t.path !== path);
      setActive(remaining.length ? remaining[remaining.length - 1].path : null);
    }
  };

  const save = async (tab: Tab | null) => {
    if (!tab) return;
    try {
      await call("fs_write_text", { path: tab.path, content: tab.content });
      setTabs((ts) => ts.map((t) => (t.path === tab.path ? { ...t, saved: tab.content } : t)));
    } catch (e) { setError(describe(e)); }
  };

  const run = async () => {
    if (!activeTab) return;
    await save(activeTab);
    const ext = extOf(activeTab.name);

    if (OPENS_INSTEAD.has(ext)) {
      try { await openPath(activeTab.path); }
      catch (e) { setRunError(describe(e)); }
      return;
    }

    setRunning(true); setRunError(null); setResult(null); setOutputOpen(true);
    try {
      setResult(await call<CheckResult>("code_run_file", { path: activeTab.path }));
    } catch (e) { setRunError(describe(e)); }
    finally { setRunning(false); }
  };

  const canRun = activeTab ? RUNNABLE.has(extOf(activeTab.name)) || OPENS_INSTEAD.has(extOf(activeTab.name)) : false;

  return (
    <div className="h-full min-h-0 flex flex-col gap-3">
      <div className="flex-1 min-h-0 flex gap-0 panel overflow-hidden">
        {/* file tree — the "Explorer" pane */}
        <aside className="w-[230px] shrink-0 border-r border-line flex flex-col min-h-0" style={{ background: "var(--surface)" }}>
          <div className="h-9 px-2.5 flex items-center justify-between border-b border-line shrink-0">
            <span className="text-[11px] uppercase tracking-wide text-muted">Explorer</span>
            <button className="btn !py-0.5 !px-2 !text-[11px]" onClick={pickFolder}>Open folder</button>
          </div>
          <div className="flex-1 overflow-auto py-1">
            {!root && <p className="px-3 py-3 text-[12px] text-muted">Pick a folder to browse its files.</p>}
            {root && (
              <TreeNode
                path={root}
                depth={0}
                entries={tree[root] ?? []}
                tree={tree}
                expanded={expandedDirs}
                onToggleDir={toggleDir}
                onOpenFile={openFile}
                activePath={active}
                rootLabel={root.split(/[\\/]/).filter(Boolean).pop() ?? root}
              />
            )}
          </div>
        </aside>

        {/* tabs + editor */}
        <div className="flex-1 min-w-0 flex flex-col min-h-0">
          <div className="h-9 shrink-0 flex items-stretch border-b border-line overflow-x-auto" style={{ background: "var(--ink)" }}>
            {tabs.map((t) => {
              const isActive = t.path === active;
              const isDirty = t.content !== t.saved;
              return (
                <div
                  key={t.path}
                  onClick={() => setActive(t.path)}
                  className="flex items-center gap-2 px-3 text-[12.5px] cursor-pointer border-r border-line shrink-0"
                  style={{ background: isActive ? "var(--bg)" : "transparent", color: isActive ? "var(--text)" : "var(--muted)" }}
                >
                  <span className="truncate max-w-[140px]">{t.name}</span>
                  {isDirty && <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: "var(--accent)" }} />}
                  <button
                    className="text-muted hover:text-body shrink-0"
                    onClick={(e) => { e.stopPropagation(); closeTab(t.path); }}
                  >
                    ×
                  </button>
                </div>
              );
            })}
            {tabs.length === 0 && (
              <div className="flex items-center px-3 text-[12.5px] text-muted">No file open</div>
            )}
            <div className="ml-auto flex items-center gap-1.5 px-2 shrink-0">
              {activeTab && (
                <button className="btn !py-1 !text-[12px]" onClick={() => save(activeTab)} disabled={!dirty}>
                  Save{dirty ? " •" : ""}
                </button>
              )}
              {activeTab && canRun && (
                <button className="btn btn-primary !py-1 !text-[12px]" onClick={run} disabled={running}>
                  {running ? "Running…" : extOf(activeTab.name) && OPENS_INSTEAD.has(extOf(activeTab.name)) ? "Open in browser" : "Run"}
                </button>
              )}
            </div>
          </div>

          <div className="flex-1 min-h-0">
            {busy && <Busy label="Opening…" />}
            {!busy && activeTab && (
              <CodeMirrorEditor
                key={activeTab.path}
                value={activeTab.content}
                extension={extOf(activeTab.name)}
                onChange={updateContent}
                onSaveKey={() => save(activeTab)}
              />
            )}
            {!busy && !activeTab && (
              <Empty
                title="Nothing open"
                hint="Open a folder on the left, or send a file here from Code Analyzer or Files."
              />
            )}
          </div>
        </div>
      </div>

      {error && <ErrorNotice error={error} onRetry={() => setError(null)} />}

      {/* run output — the "integrated terminal" panel */}
      {(result || runError || running) && (
        <section className="panel shrink-0">
          <header
            className="h-9 px-3 flex items-center justify-between border-b border-line cursor-pointer"
            onClick={() => setOutputOpen((o) => !o)}
          >
            <div className="flex items-center gap-2.5">
              <span className="text-[13px] font-semibold">Output</span>
              {running && <span className="live-dot w-1.5 h-1.5 rounded-full" style={{ background: "var(--accent)" }} />}
              {result && <Severity level={result.outcome} />}
              {result && <span className="mono text-[11.5px] text-muted">{(result.duration_ms / 1000).toFixed(1)}s</span>}
            </div>
            <span className="text-[11.5px] text-muted">{outputOpen ? "Hide" : "Show"}</span>
          </header>
          {outputOpen && (
            <div className="p-3 max-h-[260px] overflow-auto">
              {runError && <ErrorNotice error={runError} />}
              {running && <Busy label="Running…" />}
              {result && (
                <>
                  {result.problems.length > 0 && (
                    <ul className="mb-3 space-y-1.5">
                      {result.problems.slice(0, 40).map((p, i) => (
                        <li key={i} className="border-l-2 pl-2.5 py-0.5" style={{ borderColor: "var(--err)" }}>
                          <p className="mono text-[11.5px] selectable">
                            {p.file}{p.line ? `:${p.line}` : ""}{p.column ? `:${p.column}` : ""}
                          </p>
                          <p className="text-[12.5px]">{p.message}</p>
                          <p className="text-[12px] text-muted">{p.cause}</p>
                          <p className="text-[12px]" style={{ color: "var(--info)" }}>{p.suggestion}</p>
                        </li>
                      ))}
                    </ul>
                  )}
                  {result.stdout && (
                    <pre className="mono text-[12px] whitespace-pre-wrap selectable">{result.stdout}</pre>
                  )}
                  {result.stderr && (
                    <pre className="mono text-[12px] whitespace-pre-wrap selectable mt-2" style={{ color: "var(--err)" }}>
                      {result.stderr}
                    </pre>
                  )}
                  {!result.stdout && !result.stderr && result.problems.length === 0 && (
                    <p className="text-[12.5px] text-muted">No output.</p>
                  )}
                </>
              )}
            </div>
          )}
        </section>
      )}
    </div>
  );
}

function TreeNode({
  path, depth, entries, tree, expanded, onToggleDir, onOpenFile, activePath, rootLabel,
}: {
  path: string;
  depth: number;
  entries: FsEntry[];
  tree: Record<string, FsEntry[]>;
  expanded: string[];
  onToggleDir: (path: string) => void;
  onOpenFile: (path: string) => void;
  activePath: string | null;
  rootLabel?: string;
}) {
  const isOpen = expanded.includes(path);
  const sorted = [...entries].sort((a, b) => (a.is_dir === b.is_dir ? a.name.localeCompare(b.name) : a.is_dir ? -1 : 1));
  return (
    <div>
      {depth === 0 && (
        <div
          className="px-2.5 py-1 text-[12.5px] font-semibold cursor-pointer truncate flex items-center gap-1.5"
          onClick={() => onToggleDir(path)}
        >
          <span className="text-muted">{isOpen ? "▾" : "▸"}</span>{rootLabel}
        </div>
      )}
      {isOpen && sorted.map((e) => (
        <div key={e.path}>
          <div
            className="text-[12.5px] cursor-pointer truncate flex items-center gap-1.5 hover:bg-raised"
            style={{
              paddingLeft: `${12 + (depth + 1) * 12}px`,
              paddingTop: "3px", paddingBottom: "3px",
              background: activePath === e.path ? "var(--raised)" : "transparent",
            }}
            onClick={() => (e.is_dir ? onToggleDir(e.path) : onOpenFile(e.path))}
          >
            {e.is_dir ? <span className="text-muted shrink-0">{expanded.includes(e.path) ? "▾" : "▸"}</span> : <span className="w-2.5 shrink-0" />}
            <span className="truncate">{e.name}</span>
          </div>
          {e.is_dir && expanded.includes(e.path) && (
            <TreeNode
              path={e.path}
              depth={depth + 1}
              entries={tree[e.path] ?? []}
              tree={tree}
              expanded={expanded}
              onToggleDir={onToggleDir}
              onOpenFile={onOpenFile}
              activePath={activePath}
            />
          )}
        </div>
      ))}
    </div>
  );
}
