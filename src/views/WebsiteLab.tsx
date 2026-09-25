import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, WebsiteReport } from "../lib/api";
import { Panel, ErrorNotice, Row, Stat, Severity, Busy, Empty, OfflineNotice } from "../components/ui";
import { usePersistedState } from "../lib/persist";

export default function WebsiteLab({ online }: { online: boolean }) {
  const [target, setTarget] = usePersistedState<string>("website.target", "");
  const [report, setReport] = usePersistedState<WebsiteReport | null>("website.report", null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    if (!target.trim()) return;
    setBusy(true); setError(null); setReport(null);
    try {
      setReport(await call<WebsiteReport>("website_scan", { target }));
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const exportJson = async () => {
    if (!report) return;
    const dest = await save({ defaultPath: "website-report.json", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!dest) return;
    await call("export_report", { payload: JSON.stringify(report, null, 2), format: "json", destination: dest });
  };

  const counts = report
    ? {
        error: report.findings.filter((f) => f.severity === "error").length,
        warning: report.findings.filter((f) => f.severity === "warning").length,
        info: report.findings.filter((f) => f.severity === "info").length,
      }
    : null;

  return (
    <div className="max-w-[1100px] space-y-4">
      {!online && <OfflineNotice what="Website lab" />}

      <Panel title="Check a site">
        <div className="flex gap-2">
          <input
            className="field mono"
            placeholder="https://example.com"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && run()}
          />
          <button className="btn btn-primary" onClick={run} disabled={busy || !online}>
            {busy ? "Checking…" : "Run checks"}
          </button>
        </div>
        <p className="text-[11.5px] text-muted mt-2">
          Reads what the site publishes to any visitor: name resolution, connection timing,
          response headers, cookie settings and page structure. It sends one ordinary
          request — it does not attempt logins or probe for weaknesses.
        </p>
      </Panel>

      {busy && <Busy label={`Contacting ${target}`} />}
      {error && <ErrorNotice error={error} onRetry={run} />}
      {!report && !busy && !error && (
        <Empty title="No report yet" hint="Enter an address above to see how the site responds." />
      )}

      {report && (
        <>
          <Panel
            title="Summary"
            actions={<button className="btn !py-1 !text-[12px]" onClick={exportJson}>Export JSON</button>}
          >
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
              <Stat label="Status" value={report.status}
                tone={report.status >= 400 ? "var(--err)" : "var(--ok)"} />
              <Stat label="Protocol" value={`${report.scheme.toUpperCase()} · ${report.http_version}`} />
              <Stat label="Total time" value={`${report.timings.total_ms} ms`} />
              <Stat label="Findings"
                value={`${counts!.error} error · ${counts!.warning} warning`}
                tone={counts!.error ? "var(--err)" : counts!.warning ? "var(--warn)" : "var(--ok)"} />
            </div>
            <Row label="Requested" value={report.url} />
            <Row label="Ended at" value={report.final_url} />
            <Row label="Addresses" value={report.addresses.join(", ")} />
            <Row label="Page title" value={report.page.title || "—"} />
          </Panel>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <Panel title="Timing">
              <Row label="Name lookup" value={`${report.timings.dns_ms} ms`} />
              <Row label="Connection" value={`${report.timings.connect_ms} ms`} />
              <Row label="First response" value={`${report.timings.first_response_ms} ms`} />
              <Row label="Complete" value={`${report.timings.total_ms} ms`} />
            </Panel>

            <Panel title="Page contents">
              <Row label="HTML size" value={`${(report.page.html_bytes / 1024).toFixed(1)} KB`} />
              <Row label="Stylesheets / scripts" value={`${report.page.stylesheets} / ${report.page.scripts}`} />
              <Row label="Images" value={report.page.images} />
              <Row label="Links" value={`${report.page.links_internal} internal, ${report.page.links_external} external`} />
              {report.page.external_hosts.length > 0 && (
                <Row label="Outside hosts" value={report.page.external_hosts.join(", ")} />
              )}
            </Panel>
          </div>

          <Panel title="Protection headers">
            {report.security_headers.map(([name, value]) => (
              <div key={name} className="flex gap-3 py-1 border-b border-line last:border-0 text-[12.5px]">
                <span className="mono text-muted w-[240px] shrink-0">{name}</span>
                {value ? (
                  <span className="mono truncate selectable" style={{ color: "var(--ok)" }}>{value}</span>
                ) : (
                  <span className="text-muted">not sent</span>
                )}
              </div>
            ))}
          </Panel>

          {report.redirects.length > 0 && (
            <Panel title="Redirects">
              {report.redirects.map((r, i) => (
                <div key={i} className="mono text-[12px] py-1 border-b border-line last:border-0 truncate">
                  <span className="text-muted">{r.status}</span> {r.from} → {r.to}
                </div>
              ))}
            </Panel>
          )}

          <Panel title="Findings">
            <ul className="divide-y divide-line">
              {report.findings.map((f, i) => (
                <li key={i} className="py-2">
                  <div className="flex items-center gap-2.5">
                    <Severity level={f.severity} />
                    <span className="text-[13px]">{f.title}</span>
                    <span className="ml-auto text-[11.5px] text-muted">{f.area}</span>
                  </div>
                  <p className="text-[12.5px] text-muted mt-0.5">{f.detail}</p>
                </li>
              ))}
            </ul>
          </Panel>

          <Panel title="All response headers">
            <div className="mono text-[11.5px] space-y-0.5 selectable">
              {report.headers.map(([k, v], i) => (
                <div key={i} className="truncate">
                  <span className="text-muted">{k}:</span> {v}
                </div>
              ))}
            </div>
          </Panel>
        </>
      )}
    </div>
  );
}
