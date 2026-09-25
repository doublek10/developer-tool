import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, DbAnalysis } from "../lib/api";
import { Panel, ErrorNotice, Stat, Severity, Busy, Empty, Row } from "../components/ui";
import { usePersistedState } from "../lib/persist";

type Stage = "idle" | "analysed" | "backed-up" | "applied";

export default function DatabaseLab() {
  const [path, setPath] = usePersistedState<string>("dblab.path", "");
  const [analysis, setAnalysis] = usePersistedState<DbAnalysis | null>("dblab.analysis", null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [stage, setStage] = usePersistedState<Stage>("dblab.stage", "idle");
  const [backup, setBackup] = usePersistedState<string>("dblab.backup", "");
  const [chosen, setChosen] = useState<Set<number>>(new Set());
  const [applied, setApplied] = usePersistedState<string[]>("dblab.applied", []);
  const [table, setTable] = useState<string | null>(null);

  const pick = async () => {
    const f = await open({ multiple: false, filters: [{ name: "SQLite", extensions: ["sqlite", "sqlite3", "db", "db3"] }] });
    if (typeof f === "string") { setPath(f); analyse(f); }
  };

  const analyse = async (target?: string) => {
    const p = target ?? path;
    if (!p) return;
    setBusy(true); setError(null); setApplied([]); setBackup(""); setStage("idle");
    try {
      const a = await call<DbAnalysis>("db_analyze_sqlite", { path: p });
      setAnalysis(a);
      setTable(a.tables[0]?.name ?? null);
      setChosen(new Set());
      setStage("analysed");
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const takeBackup = async () => {
    setError(null);
    try {
      setBackup(await call<string>("db_backup_sqlite", { path }));
      setStage("backed-up");
    } catch (e) { setError(describe(e)); }
  };

  const apply = async () => {
    if (!analysis) return;
    setError(null);
    const statements = analysis.plan_sql.filter((_, i) => chosen.has(i));
    if (statements.length !== analysis.plan_sql.length) {
      setError({
        status: "error", code: "PARTIAL_PLAN_NOT_SUPPORTED",
        message: "Approve the whole plan, or none of it.",
        recovery: "The plan is checked as one unit so what you approved is exactly what runs. Clear the checkboxes you don't want and re-run the analysis to build a smaller plan.",
      });
      return;
    }
    setBusy(true);
    try {
      const done = await call<string[]>("db_apply_repair", {
        path, statements, planToken: analysis.plan_token, backupPath: backup,
      });
      setApplied(done);
      setStage("applied");
      analyse();
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const active = analysis?.tables.find((t) => t.name === table);
  const repairable = analysis?.plan_sql ?? [];

  return (
    <div className="max-w-[1250px] space-y-4">
      <Panel title="Open a database">
        <div className="flex gap-2">
          <input className="field mono" placeholder="C:\Projects\orbit\data.sqlite3" value={path} onChange={(e) => setPath(e.target.value)} />
          <button className="btn" onClick={pick}>Choose file</button>
          <button className="btn btn-primary" onClick={() => analyse()} disabled={busy || !path}>Analyze</button>
        </div>
        <p className="text-[11.5px] text-muted mt-2">
          SQLite databases work now. PostgreSQL, MySQL, MariaDB and SQL Server profiles
          can be saved under Servers, but their drivers are not built in yet — see the roadmap.
        </p>
      </Panel>

      {busy && <Busy label="Reading the schema" />}
      {error && <ErrorNotice error={error} />}
      {!analysis && !busy && !error && (
        <Empty title="No database open" hint="Open a SQLite file to see its tables, relationships and anything that looks wrong." />
      )}

      {analysis && (
        <>
          <Panel title="Health">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-3">
              <Stat label="Tables" value={analysis.tables.length} />
              <Stat label="Rows total" value={analysis.tables.reduce((a, t) => a + Math.max(t.rows, 0), 0).toLocaleString()} />
              <Stat label="Integrity check" value={analysis.integrity}
                tone={analysis.integrity === "ok" ? "var(--ok)" : "var(--err)"} />
              <Stat label="Repairable findings" value={repairable.length}
                tone={repairable.length ? "var(--warn)" : "var(--ok)"} />
            </div>
            <Row label="File" value={analysis.path} />
            <Row label="Views / triggers" value={`${analysis.views.length} / ${analysis.triggers.length}`} />
          </Panel>

          <Panel title="Findings">
            <ul className="divide-y divide-line">
              {analysis.diagnoses.map((d, i) => (
                <li key={i} className="py-2">
                  <div className="flex items-center gap-2.5">
                    <Severity level={d.severity} />
                    <span className="text-[13px]">{d.title}</span>
                    {d.table && <span className="mono text-[11.5px] text-muted ml-auto">{d.table}</span>}
                  </div>
                  <p className="text-[12.5px] text-muted mt-0.5">{d.detail}</p>
                  {d.suggested_sql && (
                    <pre className="mono text-[11.5px] mt-1.5 p-2 rounded bg-bg border border-line overflow-auto selectable whitespace-pre-wrap">
                      {d.suggested_sql}
                    </pre>
                  )}
                </li>
              ))}
            </ul>
          </Panel>

          {repairable.length > 0 && (
            <Panel title="Repair plan">
              <ol className="text-[12.5px] flex flex-wrap gap-x-3 gap-y-1 mb-3 text-muted">
                {["Analyze", "Back up", "Review", "Apply", "Verify"].map((s, i) => {
                  const done =
                    (i === 0 && stage !== "idle") ||
                    (i === 1 && (stage === "backed-up" || stage === "applied")) ||
                    (i === 2 && chosen.size === repairable.length) ||
                    (i >= 3 && stage === "applied");
                  return (
                    <li key={s} style={{ color: done ? "var(--ok)" : undefined }}>
                      {done ? "✓" : "○"} {s}
                    </li>
                  );
                })}
              </ol>

              <div className="space-y-2 mb-3">
                {repairable.map((sql, i) => (
                  <label key={i} className="flex gap-2.5 items-start p-2 border border-line rounded cursor-pointer">
                    <input
                      type="checkbox"
                      className="mt-1"
                      checked={chosen.has(i)}
                      onChange={(e) => {
                        const next = new Set(chosen);
                        e.target.checked ? next.add(i) : next.delete(i);
                        setChosen(next);
                      }}
                    />
                    <pre className="mono text-[11.5px] whitespace-pre-wrap flex-1 selectable">{sql}</pre>
                  </label>
                ))}
              </div>

              <div className="flex items-center gap-2 flex-wrap">
                <button className="btn" onClick={takeBackup} disabled={busy}>Back up now</button>
                <button
                  className="btn btn-primary"
                  onClick={apply}
                  disabled={busy || !backup || chosen.size === 0}
                >
                  Apply {chosen.size} statement{chosen.size === 1 ? "" : "s"}
                </button>
                {!backup && <span className="text-[12px] text-muted">A backup is required before anything is written.</span>}
                {backup && <span className="mono text-[11.5px]" style={{ color: "var(--ok)" }}>Backup: {backup}</span>}
              </div>

              {applied.length > 0 && (
                <p className="text-[12.5px] mt-3" style={{ color: "var(--ok)" }}>
                  {applied.length} statements applied and the database re-checked.
                </p>
              )}
            </Panel>
          )}

          <div className="grid grid-cols-1 lg:grid-cols-[220px_1fr] gap-4">
            <Panel title="Tables" className="max-h-[420px]">
              <ul className="text-[12.5px]">
                {analysis.tables.map((t) => (
                  <li key={t.name}>
                    <button
                      className="w-full text-left py-1 flex justify-between gap-2"
                      style={{ color: t.name === table ? "var(--accent)" : undefined }}
                      onClick={() => setTable(t.name)}
                    >
                      <span className="mono truncate">{t.name}</span>
                      <span className="mono text-muted">{t.rows}</span>
                    </button>
                  </li>
                ))}
              </ul>
            </Panel>

            <Panel title={active ? `${active.name} — structure` : "Structure"} className="max-h-[420px]">
              {!active ? <Empty title="Pick a table" /> : (
                <>
                  <table className="w-full text-[12.5px] mb-4">
                    <thead className="text-muted">
                      <tr className="border-b border-line">
                        <th className="text-left font-normal py-1">Column</th>
                        <th className="text-left font-normal">Type</th>
                        <th className="text-left font-normal">Rules</th>
                      </tr>
                    </thead>
                    <tbody>
                      {active.columns.map((c) => (
                        <tr key={c.name} className="border-b border-line last:border-0">
                          <td className="mono py-1">{c.name}</td>
                          <td className="text-muted">{c.data_type || "—"}</td>
                          <td className="text-muted text-[11.5px]">
                            {[c.primary_key && "primary key", c.not_null && "required",
                              c.default_value && `default ${c.default_value}`]
                              .filter(Boolean).join(", ") || "—"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>

                  {active.foreign_keys.length > 0 && (
                    <>
                      <p className="text-[12px] text-muted mb-1">Relationships</p>
                      {active.foreign_keys.map((fk, i) => (
                        <div key={i} className="mono text-[11.5px] py-0.5">
                          {fk.column} → {fk.references_table}.{fk.references_column}
                          <span className="text-muted"> on delete {fk.on_delete.toLowerCase()}</span>
                        </div>
                      ))}
                    </>
                  )}

                  {active.indexes.length > 0 && (
                    <>
                      <p className="text-[12px] text-muted mt-3 mb-1">Indexes</p>
                      {active.indexes.map((idx) => (
                        <div key={idx.name} className="mono text-[11.5px] py-0.5 truncate">
                          {idx.name} <span className="text-muted">({idx.columns.join(", ")}{idx.unique ? ", unique" : ""})</span>
                        </div>
                      ))}
                    </>
                  )}
                </>
              )}
            </Panel>
          </div>
        </>
      )}
    </div>
  );
}
