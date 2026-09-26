import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, ClonedSite, ClonePlacement } from "../lib/api";
import { Panel, ErrorNotice, Row, Busy, Empty, OfflineNotice } from "../components/ui";
import CodeMirrorEditor from "../components/CodeMirrorEditor";
import { usePersistedState } from "../lib/persist";

type Tab = "html" | "css" | "js";

const TABS: { id: Tab; label: string; ext: string }[] = [
  { id: "html", label: "index.html", ext: "html" },
  { id: "css", label: "styles.css", ext: "css" },
  { id: "js", label: "script.js", ext: "js" },
];

function hostOf(u: string): string {
  try { return new URL(u).hostname || "site"; } catch { return "site"; }
}

export default function WebCloner({
  online, onOpenInEditor,
}: { online: boolean; onOpenInEditor: (path: string, root?: string) => void }) {
  const [target, setTarget] = usePersistedState<string>("clone.target", "");
  const [site, setSite] = usePersistedState<ClonedSite | null>("clone.site", null);
  const [html, setHtml] = usePersistedState<string>("clone.html", "");
  const [css, setCss] = usePersistedState<string>("clone.css", "");
  const [js, setJs] = usePersistedState<string>("clone.js", "");
  const [tab, setTab] = useState<Tab>("html");
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  // Keeps the editable buffers empty if a fresh install restores an old
  // `site` without its matching buffers (older persisted sessions).
  useEffect(() => {
    if (site && !html && !css && !js) {
      setHtml(site.html); setCss(site.css); setJs(site.js);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async () => {
    if (!target.trim()) return;
    setBusy(true); setError(null); setNote(null);
    try {
      const result = await call<ClonedSite>("website_clone", { target });
      setSite(result);
      setHtml(result.html); setCss(result.css); setJs(result.js);
      setTab("html");
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const saveFiles = async () => {
    if (!site) return;
    const dest = await openDialog({ directory: true, multiple: false, title: "Choose a folder to save the clone into" });
    if (typeof dest !== "string") return;
    setError(null); setNote(null);
    try {
      await call("fs_create_dir", { path: dest });
      await call("fs_write_text", { path: `${dest}/index.html`, content: html });
      await call("fs_write_text", { path: `${dest}/styles.css`, content: css });
      await call("fs_write_text", { path: `${dest}/script.js`, content: js });
      setNote(`Saved index.html, styles.css and script.js to ${dest}`);
    } catch (e) { setError(describe(e)); }
  };

  const openEditor = async () => {
    if (!site) return;
    setError(null); setNote(null);
    try {
      const placed = await call<ClonePlacement>("website_clone_stage", {
        html, css, js, label: hostOf(site.final_url || site.source_url),
      });
      onOpenInEditor(placed.index_path, placed.root);
    } catch (e) { setError(describe(e)); }
  };

  const active = TABS.find((t) => t.id === tab)!;
  const value = tab === "html" ? html : tab === "css" ? css : js;
  const setValue = tab === "html" ? setHtml : tab === "css" ? setCss : setJs;
  const sizeOf = (s: string) => `${(new Blob([s]).size / 1024).toFixed(1)} KB`;

  return (
    <div className="max-w-[1100px] space-y-4">
      {!online && <OfflineNotice what="Web page clone" />}

      <Panel title="Clone a page">
        <div className="flex gap-2">
          <input
            className="field mono"
            placeholder="https://example.com"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && run()}
          />
          <button className="btn btn-primary" onClick={run} disabled={busy || !online}>
            {busy ? "Cloning…" : "Clone"}
          </button>
        </div>
        <p className="text-[11.5px] text-muted mt-2">
          Fetches the page, pulls its stylesheets and scripts out into their own files, and
          leaves everything else — including inline colors and layout — untouched, so the
          design comes out looking the same.
        </p>
      </Panel>

      {busy && <Busy label={`Fetching ${target}`} />}
      {error && <ErrorNotice error={error} onRetry={run} />}
      {!site && !busy && !error && (
        <Empty title="Nothing cloned yet" hint="Paste a page address above to pull down its HTML, CSS and JS." />
      )}

      {site && (
        <>
          <Panel
            title="Result"
            actions={
              <>
                <button className="btn !py-1 !text-[12px]" onClick={saveFiles}>Save files…</button>
                <button className="btn btn-primary !py-1 !text-[12px]" onClick={openEditor}>Open in Code Editor</button>
              </>
            }
          >
            <Row label="Title" value={site.title || "—"} />
            <Row label="Requested" value={site.source_url} />
            <Row label="Fetched from" value={site.final_url} />
            {note && <p className="text-[12.5px] mt-2" style={{ color: "var(--ok)" }}>{note}</p>}
          </Panel>

          {site.warnings.length > 0 && (
            <Panel title="Not included">
              <ul className="space-y-1">
                {site.warnings.map((w, i) => (
                  <li key={i} className="text-[12.5px] text-muted">{w}</li>
                ))}
              </ul>
            </Panel>
          )}

          <Panel title="Files" bodyClass="p-0">
            <div className="h-9 shrink-0 flex items-stretch border-b border-line" style={{ background: "var(--ink)" }}>
              {TABS.map((t) => {
                const isActive = t.id === tab;
                const content = t.id === "html" ? html : t.id === "css" ? css : js;
                return (
                  <button
                    key={t.id}
                    onClick={() => setTab(t.id)}
                    className="flex items-center gap-2 px-3 text-[12.5px] border-r border-line shrink-0"
                    style={{ background: isActive ? "var(--bg)" : "transparent", color: isActive ? "var(--text)" : "var(--muted)" }}
                  >
                    {t.label}
                    <span className="mono text-[10.5px] text-muted">{sizeOf(content)}</span>
                  </button>
                );
              })}
            </div>
            <div className="h-[420px]">
              <CodeMirrorEditor key={active.id} value={value} extension={active.ext} onChange={setValue} onSaveKey={() => undefined} />
            </div>
          </Panel>

          {site.assets.length > 0 && (
            <Panel title="Linked stylesheets & scripts">
              <div className="space-y-1">
                {site.assets.map((a, i) => (
                  <div key={i} className="flex gap-3 py-1 border-b border-line last:border-0 text-[12px]">
                    <span className="text-muted w-[76px] shrink-0">{a.kind}</span>
                    <span className="mono truncate selectable min-w-0 flex-1">{a.url}</span>
                    <span className="shrink-0" style={{ color: a.included ? "var(--ok)" : "var(--muted)" }}>
                      {a.included ? `${(a.bytes / 1024).toFixed(1)} KB` : a.note ?? "skipped"}
                    </span>
                  </div>
                ))}
              </div>
            </Panel>
          )}
        </>
      )}
    </div>
  );
}
