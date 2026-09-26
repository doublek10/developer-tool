import { useEffect, useState } from "react";
import { call, describe, AppError, DashboardData, SessionUser } from "../lib/api";
import { Panel, ErrorNotice, Stat, Busy, Empty } from "../components/ui";

const GREETING = () => {
  const h = new Date().getHours();
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
};

export default function Dashboard({
  user, onNavigate, onSummary,
}: {
  user: SessionUser;
  onNavigate: (v: string) => void;
  onSummary: (d: DashboardData) => void;
}) {
  const [data, setData] = useState<DashboardData | null>(null);
  const [error, setError] = useState<AppError | null>(null);

  const load = async () => {
    setError(null);
    try {
      const d = await call<DashboardData>("dashboard");
      setData(d);
      onSummary(d);
    } catch (e) {
      setError(describe(e));
    }
  };

  useEffect(() => { load(); }, []);

  if (error) return <ErrorNotice error={error} onRetry={load} />;
  if (!data) return <Busy label="Reading workstation state" />;

  const memPct = data.resources.memory_total_mb
    ? Math.round((data.resources.memory_used_mb / data.resources.memory_total_mb) * 100)
    : 0;

  return (
    <div className="max-w-[1180px] space-y-4">
      <div>
        <h2 className="text-[19px] font-semibold tracking-tight">
          {GREETING()}, {user.display_name || user.username}
        </h2>
        <p className="text-[12.5px] text-muted mt-0.5">
          {data.online ? "Connected." : "Working offline."} {data.project_count} projects,{" "}
          {data.server_count} servers, {data.cv_count} CVs on this machine.
        </p>
      </div>

      {!user.password_changed && (
        <div className="panel p-3 flex items-center gap-3" style={{ borderColor: "var(--accent)" }}>
          <div className="flex-1">
            <p className="text-[13px]" style={{ color: "var(--accent)" }}>
              This account is still using the password it was created with.
            </p>
            <p className="text-[12.5px] text-muted mt-0.5">
              Change it now so the only copy is one you chose.
            </p>
          </div>
          <button className="btn btn-primary" onClick={() => onNavigate("settings")}>
            Change password
          </button>
        </div>
      )}

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <Panel title="This computer">
          <div className="space-y-3">
            <Stat label="Processor load" value={`${data.resources.cpu_percent.toFixed(0)}%`} />
            <Stat
              label="Memory"
              value={`${data.resources.memory_used_mb.toLocaleString()} / ${data.resources.memory_total_mb.toLocaleString()} MB`}
              tone={memPct > 85 ? "var(--warn)" : undefined}
            />
            <div className="text-[11.5px] text-muted mono truncate">{data.resources.host}</div>
          </div>
        </Panel>

        <Panel title="Start something" className="col-span-2">
          <div className="grid grid-cols-2 gap-2">
            {[
              ["cv", "Build a CV"],
              ["pdf", "Open a PDF"],
              ["analyzer", "Analyze a project"],
              ["tests", "Run tests"],
              ["database", "Inspect a database"],
              ["website", "Check a website"],
            ].map(([id, label]) => (
              <button key={id} className="btn text-left" onClick={() => onNavigate(id)}>
                {label}
              </button>
            ))}
          </div>
        </Panel>

        <Panel title="Storage">
          <p className="text-[11.5px] text-muted">Everything is saved here:</p>
          <p className="mono text-[11.5px] mt-1.5 break-all selectable">{data.data_folder}</p>
          <p className="text-[11.5px] text-muted mt-2.5">
            Credentials are kept separately in Windows Credential Manager.
          </p>
        </Panel>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel title="Recent activity" className="max-h-[320px]">
          {data.activity.length === 0 ? (
            <Empty title="Nothing yet" hint="Scans, exports and repairs show up here as you work." />
          ) : (
            <ul className="text-[12.5px] divide-y divide-line">
              {data.activity.map((a, i) => (
                <li key={i} className="py-1.5 flex gap-3">
                  <span className="mono text-muted shrink-0">{a.at.slice(11, 16)}</span>
                  <span className="shrink-0">{a.label}</span>
                  <span className="mono text-muted truncate ml-auto">{a.target}</span>
                </li>
              ))}
            </ul>
          )}
        </Panel>

        <Panel
          title="Needs attention"
          actions={<button className="btn !py-1 !text-[12px]" onClick={() => onNavigate("logs")}>All logs</button>}
          className="max-h-[320px]"
        >
          {data.recent_errors.length === 0 ? (
            <Empty title="No errors or warnings" hint="Anything that goes wrong is recorded here first." />
          ) : (
            <ul className="text-[12.5px] divide-y divide-line">
              {data.recent_errors.map((e) => (
                <li key={e.id} className="py-1.5">
                  <div className="flex gap-2 items-baseline">
                    <span
                      className="mono text-[11px] shrink-0"
                      style={{ color: e.level === "error" ? "var(--err)" : "var(--warn)" }}
                    >
                      {e.module}
                    </span>
                    <span className="truncate">{e.message}</span>
                    <span className="mono text-[11px] text-muted ml-auto shrink-0">{e.at.slice(11, 16)}</span>
                  </div>
                  {e.detail && <p className="mono text-[11px] text-muted truncate">{e.detail}</p>}
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
    </div>
  );
}
