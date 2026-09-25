import { useEffect, useState } from "react";
import { call, describe, AppError, Server } from "../lib/api";
import { Panel, ErrorNotice, Field, Empty, OfflineNotice } from "../components/ui";
import { usePersistedState } from "../lib/persist";

const EMPTY: Server = {
  name: "", host: "", port: 22, username: "", auth_kind: "password",
  key_path: "", vault_ref: "", kind: "ssh",
};

export default function ServerCenter({ online }: { online: boolean }) {
  const [list, setList] = useState<Server[]>([]);
  // The draft's own fields never include a plaintext credential (only a
  // vault reference), so it's safe to persist. `secret` below — the raw
  // password/key the user is about to store — never goes through this.
  const [draft, setDraft] = usePersistedState<Server>("servers.draft", EMPTY);
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<AppError | null>(null);
  const [pings, setPings] = useState<Record<string, string>>({});

  const refresh = async () => {
    try { setList(await call<Server[]>("servers_list")); } catch (e) { setError(describe(e)); }
  };
  useEffect(() => { refresh(); }, []);

  const store = async () => {
    setError(null);
    try {
      const id = await call<number>("server_save", { server: draft });
      if (secret) {
        const reference = `server:${id}:${draft.auth_kind}`;
        await call("vault_store", {
          reference, label: draft.name || draft.host, kind: "ssh",
          username: draft.username, secret,
        });
        await call("server_save", { server: { ...draft, id, vault_ref: reference } });
      }
      setDraft(EMPTY); setSecret(""); refresh();
    } catch (e) { setError(describe(e)); }
  };

  const check = async (s: Server) => {
    setError(null);
    try {
      const ms = await call<number>("server_reachable", { host: s.host, port: s.port });
      setPings((p) => ({ ...p, [String(s.id)]: `answered in ${ms} ms` }));
    } catch (e) {
      const err = describe(e);
      setPings((p) => ({ ...p, [String(s.id)]: err.message }));
    }
  };

  return (
    <div className="max-w-[1100px] space-y-4">
      {!online && <OfflineNotice what="Server center" />}

      <Panel title="How far this goes today">
        <p className="text-[12.5px] text-muted max-w-[86ch]">
          You can save server profiles, keep their passwords and keys in Windows
          Credential Manager, and check whether a machine answers on its port.
          Opening an SSH or SFTP session, browsing remote files and reading
          cPanel is not built yet — the profiles and vault entries you create now
          are what that work will connect to. See the roadmap for where it sits.
        </p>
      </Panel>

      {error && <ErrorNotice error={error} />}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel title={draft.id ? "Edit server" : "Add a server"}>
          <div className="grid grid-cols-2 gap-3">
            <Field label="Name"><input className="field" value={draft.name} onChange={(e) => setDraft({ ...draft, name: e.target.value })} placeholder="Orbit production" /></Field>
            <Field label="Kind">
              <select className="field" value={draft.kind} onChange={(e) => setDraft({ ...draft, kind: e.target.value })}>
                <option value="ssh">Linux over SSH</option>
                <option value="cpanel">cPanel</option>
              </select>
            </Field>
            <Field label="Host"><input className="field mono" value={draft.host} onChange={(e) => setDraft({ ...draft, host: e.target.value })} placeholder="203.0.113.10" /></Field>
            <Field label="Port"><input className="field mono" type="number" value={draft.port} onChange={(e) => setDraft({ ...draft, port: +e.target.value })} /></Field>
            <Field label="Username"><input className="field mono" value={draft.username} onChange={(e) => setDraft({ ...draft, username: e.target.value })} /></Field>
            <Field label="Sign in with">
              <select className="field" value={draft.auth_kind} onChange={(e) => setDraft({ ...draft, auth_kind: e.target.value })}>
                <option value="password">Password</option>
                <option value="key">Private key</option>
              </select>
            </Field>
          </div>

          {draft.auth_kind === "key" && (
            <div className="mt-3">
              <Field label="Key file" hint="The path to the key. The key itself stays where it is.">
                <input className="field mono" value={draft.key_path} onChange={(e) => setDraft({ ...draft, key_path: e.target.value })} />
              </Field>
            </div>
          )}

          <div className="mt-3">
            <Field
              label={draft.auth_kind === "key" ? "Key passphrase" : "Password"}
              hint="Saved to Windows Credential Manager, not to any file DevWorkstation writes."
            >
              <input className="field mono" type="password" value={secret} onChange={(e) => setSecret(e.target.value)} />
            </Field>
          </div>

          <div className="flex gap-2 mt-3">
            <button className="btn btn-primary" onClick={store} disabled={!draft.host}>Save server</button>
            {draft.id && <button className="btn" onClick={() => { setDraft(EMPTY); setSecret(""); }}>Cancel</button>}
          </div>
        </Panel>

        <Panel title="Saved servers" className="max-h-[440px]">
          {list.length === 0 ? (
            <Empty title="No servers saved" hint="Add one on the left to keep its details together in one place." />
          ) : (
            <ul className="divide-y divide-line">
              {list.map((s) => (
                <li key={s.id} className="py-2">
                  <div className="flex items-center gap-2">
                    <span className="text-[13px]">{s.name || s.host}</span>
                    <span className="mono text-[11.5px] text-muted">{s.username}@{s.host}:{s.port}</span>
                    <span className="ml-auto flex gap-1.5">
                      <button className="btn !py-0.5 !px-2 !text-[11px]" onClick={() => check(s)}>Check</button>
                      <button className="btn !py-0.5 !px-2 !text-[11px]" onClick={() => setDraft(s)}>Edit</button>
                      <button className="btn btn-danger !py-0.5 !px-2 !text-[11px]"
                        onClick={async () => { await call("server_delete", { id: s.id }); refresh(); }}>
                        Delete
                      </button>
                    </span>
                  </div>
                  <div className="text-[11.5px] text-muted mt-0.5">
                    {s.kind === "cpanel" ? "cPanel" : "SSH"} · {s.auth_kind === "key" ? "private key" : "password"}
                    {s.vault_ref ? " · credential saved" : " · no credential saved"}
                  </div>
                  {pings[String(s.id)] && (
                    <div className="mono text-[11.5px] mt-0.5" style={{ color: pings[String(s.id)].includes("answered") ? "var(--ok)" : "var(--err)" }}>
                      {pings[String(s.id)]}
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
    </div>
  );
}
