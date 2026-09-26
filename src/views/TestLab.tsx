import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, Check, CheckResult, Detection } from "../lib/api";
import { Panel, ErrorNotice, Severity, Busy, Empty, Row } from "../components/ui";
import { usePersistedState } from "../lib/persist";

export default function TestLab() {
  const [root, setRoot] = usePersistedState<string>("testlab.root", "");
  const [detection, setDetection] = usePersistedState<Detection | null>("testlab.detection", null);
  const [results, setResults] = usePersistedState<Record<string, CheckResult>>("testlab.results", {});
  const [running, setRunning] = useState<string | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [allowNetwork, setAllowNetwork] = usePersistedState<boolean>("testlab.allowNetwork", false);
  const [expanded, setExpanded] = useState<string | null>(null);

  const pick = async () => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === "string") { setRoot(chosen); detect(chosen); }
  };

  const detect = async (path?: string) => {
    const target = path ?? root;
    if (!target) return;
    setError(null); setResults({}); setDetection(null);
    try {
      setDetection(await call<Detection>("test_detect", { root: target }));
    } catch (e) { setError(describe(e)); }
  };

  const run = async (check: Check) => {
    setRunning(check.id); setError(null);
    try {
      const result = await call<CheckResult>("test_run", { root, check, allowNetwork });
      setResults((r) => ({ ...r, [check.id]: result }));
      setExpanded(check.id);
    } catch (e) { setError(describe(e)); }
    finally { setRunning(null); }
  };

  const runAll = async () => {
    for (const c of detection?.available_checks ?? []) await run(c);
  };

  return (
    <div className="max-w-[1150px] space-y-4">
      <Panel title="Choose a project">
        <div className="flex gap-2">
          <input className="field mono" placeholder="C:\Projects\orbit" value={root} onChange={(e) => setRoot(e.target.value)} />
          <button className="btn" onClick={pick}>Choose folder</button>
          <button className="btn btn-primary" onClick={() => detect()} disabled={!root}>Detect</button>
        </div>
        <label className="flex items-center gap-2 text-[12px] text-muted mt-2">
          <input type="checkbox" checked={allowNetwork} onChange={(e) => setAllowNetwork(e.target.checked)} />
          Let these checks reach the internet
        </label>
        <p className="text-[11.5px] text-muted mt-2 max-w-[80ch]">
          Only the detected build and test tools are started, with install hooks
          switched off, a clean environment, no keyboard input and a time limit.
          That prevents a project's setup scripts from running the moment you
          open it. It is not a virtual machine: what does run has your account's
          access to this computer.
        </p>
      </Panel>

      {error && <ErrorNotice error={error} />}
      {!detection && !error && (
        <Empty title="No project selected" hint="Pick a folder and DevWorkstation will work out what it is and what can be checked." />
      )}

      {detection && (
        <>
          <Panel
            title="What this project is"
            actions={
              <button className="btn btn-primary !py-1 !text-[12px]" onClick={runAll}
                disabled={!!running || detection.available_checks.length === 0}>
                Run all checks
              </button>
            }
          >
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3 text-[12.5px]">
              <Row label="Type" value={detection.project_type} />
              <Row label="Package manager" value={detection.package_manager} />
              <Row label="Test framework" value={detection.test_framework} />
              <Row label="Build" value={detection.build_system} />
            </div>
            {detection.warnings.map((w, i) => (
              <p key={i} className="text-[12.5px] mt-2" style={{ color: "var(--warn)" }}>{w}</p>
            ))}
          </Panel>

          <Panel title="Checks">
            {detection.available_checks.length === 0 ? (
              <Empty title="Nothing to run" hint="No recognised build or test setup was found in this folder." />
            ) : (
              <ul className="divide-y divide-line">
                {detection.available_checks.map((c) => {
                  const r = results[c.id];
                  return (
                    <li key={c.id} className="py-2">
                      <div className="flex items-center gap-3">
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2.5">
                            <span className="text-[13px]">{c.label}</span>
                            {r && <Severity level={r.outcome} />}
                            {running === c.id && (
                              <span className="live-dot w-1.5 h-1.5 rounded-full" style={{ background: "var(--accent)" }} />
                            )}
                          </div>
                          <p className="text-[12px] text-muted">{c.description}</p>
                          <p className="mono text-[11px] text-muted mt-0.5 truncate">
                            {c.program} {c.args.join(" ")}
                          </p>
                        </div>
                        {r && (
                          <span className="mono text-[11.5px] text-muted shrink-0">
                            {(r.duration_ms / 1000).toFixed(1)}s
                          </span>
                        )}
                        <button className="btn !py-1 !text-[12px] shrink-0" onClick={() => run(c)} disabled={!!running}>
                          {r ? "Re-run" : "Run"}
                        </button>
                        {r && (
                          <button className="btn !py-1 !text-[12px] shrink-0"
                            onClick={() => setExpanded(expanded === c.id ? null : c.id)}>
                            Output
                          </button>
                        )}
                      </div>

                      {r && r.problems.length > 0 && (
                        <ul className="mt-2 space-y-1.5">
                          {r.problems.slice(0, 40).map((p, i) => (
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

                      {r && expanded === c.id && (
                        <pre className="mono text-[11.5px] mt-2 p-2.5 rounded bg-bg border border-line max-h-[280px] overflow-auto whitespace-pre-wrap selectable">
                          {r.stdout || "(no output)"}
                          {r.stderr && `\n${r.stderr}`}
                        </pre>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </Panel>
        </>
      )}
      {running && <Busy label={`Running ${running}`} />}
    </div>
  );
}
