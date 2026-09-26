import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, LogLine } from "../lib/api";
import { Panel, ErrorNotice, Empty } from "../components/ui";

const MODULES = ["all", "SYSTEM", "AUTH", "CV", "PDF", "WEBSITE", "ANALYZER", "DATABASE", "TESTLAB", "TERMINAL", "FILES", "VAULT", "SERVERS", "PROJECTS"];

export default function Logs() {
  const [lines, setLines] = useState<LogLine[]>([]);
  const [module, setModule] = useState("all");
  const [level, setLevel] = useState("all");
  const [search, setSearch] = useState("");
  const [error, setError] = useState<AppError | null>(null);
  const [live, setLive] = useState(true);

  const load = async () => {
    try { setLines(await call<LogLine[]>("logs_query", { module, level, search, limit: 400 })); }
    catch (e) { setError(describe(e)); }
  };

  useEffect(() => { load(); }, [module, level, search]);
  useEffect(() => {
    if (!live) return;
    const t = setInterval(load, 4000);
    return () => clearInterval(t);
  }, [live, module, level, search]);

  const exportAll = async () => {
    const dest = await save({ defaultPath: "devworkstation.log", filters: [{ name: "Log", extensions: ["log", "txt"] }] });
    if (dest) await call("logs_export", { destination: dest });
  };

  const colour = (l: string) =>
    l === "error" ? "var(--err)" : l === "warn" ? "var(--warn)" : "var(--muted)";

  return (
    <div className="max-w-[1250px] space-y-3 h-full flex flex-col">
      <Panel
        title="Activity log"
        actions={
          <>
            <label className="flex items-center gap-1.5 text-[12px] text-muted">
              <input type="checkbox" checked={live} onChange={(e) => setLive(e.target.checked)} />
              Follow
            </label>
            <button className="btn !py-1 !text-[12px]" onClick={exportAll}>Export</button>
            <button className="btn btn-danger !py-1 !text-[12px]"
              onClick={async () => { if (confirm("Clear the log history?")) { await call("logs_clear"); load(); } }}>
              Clear
            </button>
          </>
        }
      >
        <div className="flex gap-2 flex-wrap">
          <select className="field !w-auto" value={module} onChange={(e) => setModule(e.target.value)}>
            {MODULES.map((m) => <option key={m} value={m}>{m === "all" ? "All modules" : m}</option>)}
          </select>
          <select className="field !w-auto" value={level} onChange={(e) => setLevel(e.target.value)}>
            <option value="all">All levels</option>
            <option value="info">Information</option>
            <option value="warn">Warnings</option>
            <option value="error">Errors</option>
          </select>
          <input className="field !w-[260px]" placeholder="search messages" value={search} onChange={(e) => setSearch(e.target.value)} />
        </div>
        <p className="text-[11.5px] text-muted mt-2">
          Passwords, keys and tokens are stripped before anything is written here.
        </p>
      </Panel>

      {error && <ErrorNotice error={error} onRetry={load} />}

      <Panel title={`${lines.length} entries`} className="flex-1 min-h-[300px]">
        {lines.length === 0 ? (
          <Empty title="Nothing matches" hint="Widen the filters to see more." />
        ) : (
          <div className="mono text-[11.5px] selectable">
            {lines.map((l) => (
              <div key={l.id} className="flex gap-2.5 py-[2px] border-b border-line last:border-0">
                <span className="text-muted shrink-0">{l.at}</span>
                <span className="shrink-0 w-[48px]" style={{ color: colour(l.level) }}>{l.level}</span>
                <span className="shrink-0 w-[84px] text-muted">{l.module}</span>
                <span className="min-w-0">
                  {l.message}
                  {l.detail && <span className="text-muted"> — {l.detail}</span>}
                </span>
              </div>
            ))}
          </div>
        )}
      </Panel>
    </div>
  );
}
