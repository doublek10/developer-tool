import React from "react";
import { AppError } from "../lib/api";

export function Panel({
  title, actions, children, className = "", bodyClass = "p-3",
}: {
  title?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  bodyClass?: string;
}) {
  return (
    <section className={`panel flex flex-col min-h-0 ${className}`}>
      {(title || actions) && (
        <header className="flex items-center justify-between gap-3 px-3 h-9 border-b border-line shrink-0">
          <h2 className="text-[13px] font-semibold text-body truncate">{title}</h2>
          <div className="flex items-center gap-2 shrink-0">{actions}</div>
        </header>
      )}
      <div className={`min-h-0 overflow-auto ${bodyClass}`}>{children}</div>
    </section>
  );
}

/** Spec §23: name what happened, then how to move forward. */
export function ErrorNotice({ error, onRetry }: { error: AppError; onRetry?: () => void }) {
  return (
    <div className="panel p-3" style={{ borderColor: "color-mix(in srgb, var(--err) 45%, transparent)" }}>
      <p className="font-semibold text-[13px]" style={{ color: "var(--err)" }}>{error.message}</p>
      {error.details && (
        <p className="mono text-[11.5px] text-muted mt-1.5 break-all selectable">{error.details}</p>
      )}
      {error.recovery && <p className="text-[12.5px] text-muted mt-2">{error.recovery}</p>}
      <div className="flex items-center gap-2 mt-2.5">
        {onRetry && <button className="btn" onClick={onRetry}>Try again</button>}
        <span className="mono text-[11px] text-muted">{error.code}</span>
      </div>
    </div>
  );
}

export function Empty({ title, hint, action }: { title: string; hint?: string; action?: React.ReactNode }) {
  return (
    <div className="h-full min-h-[180px] flex flex-col items-center justify-center text-center px-6 gap-2">
      <p className="text-[14px] text-body">{title}</p>
      {hint && <p className="text-[12.5px] text-muted max-w-[42ch]">{hint}</p>}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function Severity({ level }: { level: string }) {
  const map: Record<string, { c: string; t: string }> = {
    error: { c: "var(--err)", t: "Error" },
    failed: { c: "var(--err)", t: "Failed" },
    warning: { c: "var(--warn)", t: "Warning" },
    warn: { c: "var(--warn)", t: "Warning" },
    info: { c: "var(--info)", t: "Note" },
    passed: { c: "var(--ok)", t: "Passed" },
    ok: { c: "var(--ok)", t: "OK" },
    skipped: { c: "var(--muted)", t: "Skipped" },
  };
  const s = map[level] ?? { c: "var(--muted)", t: level };
  return (
    <span
      className="inline-flex items-center gap-1.5 text-[11.5px] shrink-0"
      style={{ color: s.c }}
    >
      <span className="w-1.5 h-1.5 rounded-full" style={{ background: s.c }} />
      {s.t}
    </span>
  );
}

export function Stat({ label, value, tone }: { label: string; value: React.ReactNode; tone?: string }) {
  return (
    <div>
      <div className="text-[11.5px] text-muted">{label}</div>
      <div className="mono text-[15px] mt-0.5" style={{ color: tone ?? "var(--text)" }}>{value}</div>
    </div>
  );
}

export function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex gap-3 py-1 border-b border-line last:border-0 text-[12.5px]">
      <span className="text-muted w-[150px] shrink-0">{label}</span>
      <span className="mono break-all selectable min-w-0">{value}</span>
    </div>
  );
}

export function Field({
  label, hint, children,
}: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="block text-[12px] text-muted mb-1">{label}</span>
      {children}
      {hint && <span className="block text-[11.5px] text-muted mt-1">{hint}</span>}
    </label>
  );
}

export function Busy({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 text-[12.5px] text-muted py-3">
      <span className="live-dot w-1.5 h-1.5 rounded-full" style={{ background: "var(--accent)" }} />
      {label}
    </div>
  );
}

export function OfflineNotice({ what }: { what: string }) {
  return (
    <div className="panel p-3 text-[12.5px]" style={{ borderColor: "color-mix(in srgb, var(--warn) 45%, transparent)" }}>
      <span style={{ color: "var(--warn)" }}>Internet connection required.</span>
      <span className="text-muted"> {what} needs to reach a remote machine. Everything else in DevWorkstation keeps working.</span>
    </div>
  );
}
