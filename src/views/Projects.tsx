import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, Project, Server, DatabaseProfile } from "../lib/api";
import { Panel, ErrorNotice, Field, Empty } from "../components/ui";
import { usePersistedState } from "../lib/persist";

const EMPTY: Project = {
  name: "", path: "", technology: "", repository: "",
  server_id: null, database_id: null, notes: "", status: "development",
};

export default function Projects({ onAnalyze }: { onAnalyze: (path: string) => void }) {
  const [list, setList] = useState<Project[]>([]);
  const [servers, setServers] = useState<Server[]>([]);
  const [databases, setDatabases] = useState<DatabaseProfile[]>([]);
  const [draft, setDraft] = usePersistedState<Project>("projects.draft", EMPTY);
  const [error, setError] = useState<AppError | null>(null);

  const refresh = async () => {
    try {
      const [p, s, d] = await Promise.all([
        call<Project[]>("projects_list"),
        call<Server[]>("servers_list"),
        call<DatabaseProfile[]>("databases_list"),
      ]);
      setList(p); setServers(s); setDatabases(d);
    } catch (e) { setError(describe(e)); }
  };
  useEffect(() => { refresh(); }, []);

  const pickFolder = async () => {
    const chosen = await open({ directory: true, multiple: false });
    if (typeof chosen === "string") {
      const name = draft.name || chosen.split(/[\\/]/).pop() || "";
      setDraft({ ...draft, path: chosen, name });
    }
  };

  const store = async () => {
    setError(null);
    try {
      await call("project_save", { project: draft });
      setDraft(EMPTY);
      refresh();
    } catch (e) { setError(describe(e)); }
  };

  return (
    <div className="max-w-[1100px] space-y-4">
      {error && <ErrorNotice error={error} />}

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_360px] gap-4">
        <Panel title={`Projects (${list.length})`}>
          {list.length === 0 ? (
            <Empty title="No projects registered" hint="Register a folder to keep its server, database and scan history together." />
          ) : (
            <ul className="divide-y divide-line">
              {list.map((p) => (
                <li key={p.id} className="py-2.5">
                  <div className="flex items-center gap-2.5">
                    <span className="text-[13.5px]">{p.name}</span>
                    <span
                      className="text-[11px] px-1.5 py-[1px] rounded border"
                      style={{
                        color: p.status === "online" ? "var(--ok)" : "var(--muted)",
                        borderColor: p.status === "online" ? "var(--ok)" : "var(--line)",
                      }}
                    >
                      {p.status}
                    </span>
                    <span className="ml-auto flex gap-1.5">
                      <button className="btn !py-0.5 !px-2 !text-[11px]" onClick={() => onAnalyze(p.path)}>Analyze</button>
                      <button className="btn !py-0.5 !px-2 !text-[11px]" onClick={() => setDraft(p)}>Edit</button>
                      <button className="btn btn-danger !py-0.5 !px-2 !text-[11px]"
                        onClick={async () => { await call("project_delete", { id: p.id }); refresh(); }}>Delete</button>
                    </span>
                  </div>
                  <p className="mono text-[11.5px] text-muted mt-0.5 truncate selectable">{p.path}</p>
                  <p className="text-[11.5px] text-muted">
                    {[p.technology, p.repository,
                      servers.find((s) => s.id === p.server_id)?.name,
                      databases.find((d) => d.id === p.database_id)?.name,
                      p.last_scan ? `scanned ${p.last_scan}` : "never scanned",
                    ].filter(Boolean).join(" · ")}
                  </p>
                  {p.notes && <p className="text-[12px] text-muted mt-1">{p.notes}</p>}
                </li>
              ))}
            </ul>
          )}
        </Panel>

        <Panel title={draft.id ? "Edit project" : "Register a project"}>
          <div className="space-y-3">
            <Field label="Name"><input className="field" value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} /></Field>
            <Field label="Folder">
              <div className="flex gap-2">
                <input className="field mono" value={draft.path} onChange={(e) => setDraft({ ...draft, path: e.target.value })} />
                <button className="btn" onClick={pickFolder}>Pick</button>
              </div>
            </Field>
            <Field label="Technology"><input className="field" value={draft.technology} onChange={(e) => setDraft({ ...draft, technology: e.target.value })} placeholder="Next.js, PostgreSQL" /></Field>
            <Field label="Repository"><input className="field mono" value={draft.repository} onChange={(e) => setDraft({ ...draft, repository: e.target.value })} placeholder="github.com/you/orbit" /></Field>
            <Field label="Server">
              <select className="field" value={draft.server_id ?? ""} onChange={(e) => setDraft({ ...draft, server_id: e.target.value ? +e.target.value : null })}>
                <option value="">None</option>
                {servers.map((s) => <option key={s.id} value={s.id!}>{s.name || s.host}</option>)}
              </select>
            </Field>
            <Field label="Database">
              <select className="field" value={draft.database_id ?? ""} onChange={(e) => setDraft({ ...draft, database_id: e.target.value ? +e.target.value : null })}>
                <option value="">None</option>
                {databases.map((d) => <option key={d.id} value={d.id!}>{d.name}</option>)}
              </select>
            </Field>
            <Field label="Status">
              <select className="field" value={draft.status} onChange={(e) => setDraft({ ...draft, status: e.target.value })}>
                <option value="development">Development</option>
                <option value="staging">Staging</option>
                <option value="online">Online</option>
                <option value="archived">Archived</option>
              </select>
            </Field>
            <Field label="Notes"><textarea className="field h-[64px] resize-y" value={draft.notes} onChange={(e) => setDraft({ ...draft, notes: e.target.value })} /></Field>
            <div className="flex gap-2">
              <button className="btn btn-primary" onClick={store} disabled={!draft.name}>Save project</button>
              {draft.id && <button className="btn" onClick={() => setDraft(EMPTY)}>Cancel</button>}
            </div>
          </div>
        </Panel>
      </div>
    </div>
  );
}
