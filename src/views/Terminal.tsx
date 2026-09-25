import { useEffect, useRef, useState } from "react";
import { call, describe, AppError, ShellResult } from "../lib/api";
import { Panel, ErrorNotice } from "../components/ui";
import { usePersistedState } from "../lib/persist";

interface Line { command: string; cwd: string; out: string; err: string; code: number | null; ms: number }

const BUILT_IN: Record<string, string> = {
  "dev scan": "Open Code analyzer and point it at a folder.",
  "dev test": "Open Test lab to detect and run a project's checks.",
  "dev analyze": "Same as dev scan.",
  "dev project": "Open Projects to see what is registered on this machine.",
  "dev server": "Open Servers to manage saved machines.",
  "dev database": "Open Database lab to inspect a SQLite file.",
  "dev help": "dev scan · dev test · dev project · dev server · dev database",
};

export default function Terminal() {
  const [cwd, setCwd] = usePersistedState<string>("terminal.cwd", "");
  const [input, setInput] = useState("");
  const [lines, setLines] = usePersistedState<Line[]>("terminal.lines", []);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [history, setHistory] = usePersistedState<string[]>("terminal.history", []);
  const [hIndex, setHIndex] = useState(-1);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!cwd) call<string>("fs_home").then(setCwd).catch(() => undefined);
  }, []);

  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [lines]);

  const submit = async () => {
    const command = input.trim();
    if (!command || busy) return;
    setInput(""); setError(null);
    setHistory((h) => [command, ...h].slice(0, 100));
    setHIndex(-1);

    const builtin = BUILT_IN[command.toLowerCase()];
    if (builtin) {
      setLines((l) => [...l, { command, cwd, out: builtin, err: "", code: 0, ms: 0 }]);
      return;
    }

    // Track cd ourselves, since each command runs in its own process.
    if (command.toLowerCase().startsWith("cd ")) {
      const next = command.slice(3).trim().replace(/^["']|["']$/g, "");
      setBusy(true);
      try {
        const probe = await call<ShellResult>("terminal_run", {
          command: `Set-Location -Path "${next}"; (Get-Location).Path`, cwd,
        });
        const resolved = probe.stdout.trim();
        if (probe.exit_code === 0 && resolved) {
          setCwd(resolved);
          setLines((l) => [...l, { command, cwd, out: resolved, err: "", code: 0, ms: probe.duration_ms }]);
        } else {
          setLines((l) => [...l, { command, cwd, out: "", err: probe.stderr || "That folder could not be opened.", code: 1, ms: probe.duration_ms }]);
        }
      } catch (e) { setError(describe(e)); }
      finally { setBusy(false); }
      return;
    }

    setBusy(true);
    try {
      const r = await call<ShellResult>("terminal_run", { command, cwd });
      setLines((l) => [...l, { command, cwd: r.cwd, out: r.stdout, err: r.stderr, code: r.exit_code, ms: r.duration_ms }]);
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") { e.preventDefault(); submit(); }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      const i = Math.min(hIndex + 1, history.length - 1);
      if (history[i] !== undefined) { setHIndex(i); setInput(history[i]); }
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      const i = hIndex - 1;
      setHIndex(i);
      setInput(i >= 0 ? history[i] ?? "" : "");
    }
  };

  return (
    <div className="max-w-[1100px] space-y-3 h-full flex flex-col">
      <Panel title="Terminal" actions={<button className="btn !py-1 !text-[12px]" onClick={() => setLines([])}>Clear</button>}>
        <p className="text-[12px] text-muted">
          Runs one command at a time in PowerShell, in the folder shown below.
          Commands have your account's full access to this computer — unlike Test
          lab, nothing here is contained. Type <span className="mono">dev help</span> for
          the shortcuts into DevWorkstation's own modules.
        </p>
      </Panel>

      {error && <ErrorNotice error={error} />}

      <div className="panel flex-1 min-h-[300px] flex flex-col">
        <div className="flex-1 overflow-auto p-3 mono text-[12px] selectable">
          {lines.length === 0 && <p className="text-muted">Nothing run yet.</p>}
          {lines.map((l, i) => (
            <div key={i} className="mb-2">
              <div className="flex gap-2">
                <span className="text-muted shrink-0">{shortPath(l.cwd)}</span>
                <span style={{ color: "var(--accent)" }}>&gt;</span>
                <span>{l.command}</span>
                {l.ms > 0 && <span className="ml-auto text-muted shrink-0">{l.ms} ms</span>}
              </div>
              {l.out && <pre className="whitespace-pre-wrap mt-0.5">{l.out}</pre>}
              {l.err && <pre className="whitespace-pre-wrap mt-0.5" style={{ color: "var(--err)" }}>{l.err}</pre>}
              {l.code !== null && l.code !== 0 && (
                <div className="text-[11px]" style={{ color: "var(--err)" }}>exit code {l.code}</div>
              )}
            </div>
          ))}
          <div ref={endRef} />
        </div>

        <div className="border-t border-line p-2 flex items-center gap-2">
          <span className="mono text-[12px] text-muted shrink-0">{shortPath(cwd)}</span>
          <span className="mono" style={{ color: "var(--accent)" }}>&gt;</span>
          <input
            className="field mono !border-0 !bg-transparent !p-0"
            value={input}
            placeholder={busy ? "running…" : "type a command"}
            disabled={busy}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={onKey}
          />
        </div>
      </div>
    </div>
  );
}

function shortPath(p: string) {
  if (p.length <= 34) return p;
  const parts = p.split(/[\\/]/);
  return `…${["", ...parts.slice(-2)].join("\\")}`;
}
