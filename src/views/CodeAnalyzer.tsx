import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, Analysis } from "../lib/api";
import { Panel, ErrorNotice, Stat, Severity, Busy, Empty, Row } from "../components/ui";
import { usePersistedState } from "../lib/persist";

export default function CodeAnalyzer({
  initialPath, onOpenInEditor,
}: { initialPath?: string; onOpenInEditor?: (path: string) => void }) {
  const [root, setRoot] = usePersistedState<string>("analyzer.root", initialPath ?? "");
  const [heavy, setHeavy] = useState(false);
  const [result, setResult] = usePersistedState<Analysis | null>("analyzer.result", null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    if (initialPath) { setRoot(initialPath); scan(initialPath); }
  }, [initialPath]);

  const pick = async () => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === "string") { setRoot(chosen); scan(chosen); }
  };

  const scan = async (path?: string) => {
    const target = path ?? root;
    if (!target) return;
    setBusy(true); setError(null); setResult(null);
    try {
      setResult(await call<Analysis>("analyze_project", { root: target, includeHeavyDirs: heavy }));
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const exportJson = async () => {
    if (!result) return;
    const dest = await save({ defaultPath: "code-analysis.json", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!dest) return;
    await call("export_report", { payload: JSON.stringify(result, null, 2), format: "json", destination: dest });
  };

  const bySeverity = (s: string) => result?.insights.filter((i) => i.severity === s).length ?? 0;
  const visible = result?.files.filter((f) => f.path.toLowerCase().includes(filter.toLowerCase())) ?? [];

  return (
    <div className="max-w-[1250px] space-y-4">
      <Panel title="Analyze a project">
        <div className="flex gap-2">
          <input className="field mono" placeholder="C:\Projects\orbit" value={root} onChange={(e) => setRoot(e.target.value)} />
          <button className="btn" onClick={pick}>Choose folder</button>
          <button className="btn btn-primary" onClick={() => scan()} disabled={busy || !root}>
            {busy ? "Scanning…" : "Scan"}
          </button>
        </div>
        <label className="flex items-center gap-2 text-[12px] text-muted mt-2">
          <input type="checkbox" checked={heavy} onChange={(e) => setHeavy(e.target.checked)} />
          Include node_modules, vendor, build output and other generated folders
        </label>
        <p className="text-[11.5px] text-muted mt-1.5">
          Source files are read as text. Nothing in the project is executed.
        </p>
      </Panel>

      {busy && <Busy label="Reading the project" />}
      {error && <ErrorNotice error={error} onRetry={() => scan()} />}
      {!result && !busy && !error && (
        <Empty title="No project scanned" hint="Point the analyzer at a folder to see what the project is built from and how its pieces connect." />
      )}

      {result && (
        <>
          <Panel
            title="Overview"
            actions={<button className="btn !py-1 !text-[12px]" onClick={exportJson}>Export JSON</button>}
          >
            <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-3">
              <Stat label="Source files" value={result.file_count} />
              <Stat label="Dependencies" value={Object.keys(result.dependencies).length} />
              <Stat label="Routes found" value={result.routes.length} />
              <Stat label="Errors" value={bySeverity("error")} tone={bySeverity("error") ? "var(--err)" : undefined} />
              <Stat label="Warnings" value={bySeverity("warning")} tone={bySeverity("warning") ? "var(--warn)" : undefined} />
            </div>
            <Row label="Built with" value={result.technologies.join(", ") || "not identified"} />
            <Row label="Entry points" value={result.entry_points.join(", ") || "none found"} />
            <Row label="Database use" value={result.database_hints.slice(0, 4).join(" · ") || "none detected"} />
            <Row label="Settings read from the environment" value={result.env_vars.join(", ") || "none"} />
            {result.skipped_count > 0 && (
              <Row label="Skipped" value={`${result.skipped_count} files that were too large or not text`} />
            )}
          </Panel>

          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            <Panel title="How the pieces connect">
              <ArchitectureView result={result} />
            </Panel>

            <Panel title="Languages">
              {Object.entries(result.language_totals)
                .sort((a, b) => b[1] - a[1])
                .map(([lang, lines]) => {
                  const total = Object.values(result.language_totals).reduce((a, b) => a + b, 0) || 1;
                  const pct = (lines / total) * 100;
                  return (
                    <div key={lang} className="py-1">
                      <div className="flex justify-between text-[12.5px]">
                        <span>{lang}</span>
                        <span className="mono text-muted">{lines.toLocaleString()} lines</span>
                      </div>
                      <div className="h-[3px] bg-line rounded-full mt-1 overflow-hidden">
                        <div className="h-full rounded-full" style={{ width: `${pct}%`, background: "var(--accent)" }} />
                      </div>
                    </div>
                  );
                })}
            </Panel>
          </div>

          <Panel title={`What the scan found (${result.insights.length})`} className="max-h-[420px]">
            {result.insights.length === 0 ? (
              <Empty title="Nothing flagged" />
            ) : (
              <ul className="divide-y divide-line">
                {result.insights.slice(0, 250).map((ins, i) => (
                  <li key={i} className="py-2">
                    <div className="flex items-center gap-2.5">
                      <Severity level={ins.severity} />
                      <span className="text-[13px]">{ins.title}</span>
                      <span className="ml-auto text-[11.5px] text-muted">{ins.category}</span>
                    </div>
                    <p className="text-[12.5px] text-muted mt-0.5">{ins.detail}</p>
                    {ins.path && (
                      <p className="mono text-[11.5px] text-muted mt-0.5 truncate selectable">
                        {ins.path}{ins.line ? `:${ins.line}` : ""}
                      </p>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          <Panel
            title={`Files (${visible.length})`}
            actions={
              <input className="field !py-1 !w-[200px] mono" placeholder="filter by path"
                value={filter} onChange={(e) => setFilter(e.target.value)} />
            }
            className="max-h-[420px]"
          >
            <table className="w-full text-[12.5px]">
              <thead className="text-muted">
                <tr className="border-b border-line">
                  <th className="text-left font-normal py-1">Path</th>
                  <th className="text-left font-normal">Language</th>
                  <th className="text-right font-normal">Lines</th>
                  {onOpenInEditor && <th className="w-8" />}
                </tr>
              </thead>
              <tbody>
                {visible.slice(0, 500).map((f) => (
                  <tr key={f.path} className="border-b border-line last:border-0">
                    <td className="mono py-1 truncate max-w-[520px] selectable">{f.path}</td>
                    <td className="text-muted">{f.language}</td>
                    <td className="mono text-right">{f.lines}</td>
                    {onOpenInEditor && (
                      <td className="text-right">
                        <button
                          className="btn !py-0.5 !px-2 !text-[11px]"
                          onClick={() => onOpenInEditor(`${result.root.replace(/[\\/]+$/, "")}/${f.path}`)}
                        >
                          Edit
                        </button>
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </Panel>
        </>
      )}
    </div>
  );
}

/** Groups import edges by folder so a large project reads as layers, not hairball. */
function ArchitectureView({ result }: { result: Analysis }) {
  const layers = new Map<string, number>();
  for (const f of result.files) {
    const top = f.path.includes("/") ? f.path.split("/")[0] : "(root)";
    layers.set(top, (layers.get(top) ?? 0) + 1);
  }
  const crossings = new Map<string, number>();
  for (const e of result.edges) {
    if (e.kind !== "import") continue;
    const a = e.from.split("/")[0];
    const b = e.to.split("/")[0];
    if (a === b) continue;
    const key = `${a} → ${b}`;
    crossings.set(key, (crossings.get(key) ?? 0) + 1);
  }
  const apiEdges = result.edges.filter((e) => e.kind === "api").length;
  const dbEdges = result.edges.filter((e) => e.kind === "database").length;

  return (
    <div className="text-[12.5px]">
      <div className="mb-3">
        <p className="text-muted mb-1.5">Folders, by how much code each holds</p>
        {[...layers.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([name, n]) => (
          <div key={name} className="flex items-center gap-2 py-0.5">
            <span className="mono truncate w-[160px]">{name}</span>
            <div className="flex-1 h-[3px] bg-line rounded-full overflow-hidden">
              <div className="h-full" style={{ width: `${(n / result.file_count) * 100}%`, background: "var(--info)" }} />
            </div>
            <span className="mono text-muted w-[40px] text-right">{n}</span>
          </div>
        ))}
      </div>

      <p className="text-muted mb-1.5">Imports that cross a folder boundary</p>
      {crossings.size === 0 ? (
        <p className="text-muted">No cross-folder imports were detected.</p>
      ) : (
        [...crossings.entries()].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([edge, n]) => (
          <div key={edge} className="flex justify-between py-0.5">
            <span className="mono truncate">{edge}</span>
            <span className="mono text-muted">{n}</span>
          </div>
        ))
      )}

      <div className="flex gap-4 mt-3 pt-3 border-t border-line">
        <span className="text-muted">Calls out to an API: <span className="mono text-body">{apiEdges}</span></span>
        <span className="text-muted">Touches a database: <span className="mono text-body">{dbEdges}</span></span>
      </div>
    </div>
  );
}
