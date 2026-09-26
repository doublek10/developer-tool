import { useEffect, useState } from "react";
import { call, describe, AppError, VaultItem } from "../lib/api";
import { Panel, ErrorNotice, Field, Empty } from "../components/ui";
import { usePersistedState } from "../lib/persist";

export default function Vault() {
  const [items, setItems] = useState<VaultItem[]>([]);
  const [error, setError] = useState<AppError | null>(null);
  // reference/label/kind/username are just labels for a vault entry — fine
  // to persist. `secret` is the actual credential value and must never sit
  // in local storage, so it stays a plain (non-persisted) useState.
  const [reference, setReference] = usePersistedState<string>("vault.reference", "");
  const [label, setLabel] = usePersistedState<string>("vault.label", "");
  const [kind, setKind] = usePersistedState<string>("vault.kind", "api");
  const [username, setUsername] = usePersistedState<string>("vault.username", "");
  const [secret, setSecret] = useState("");
  const [checks, setChecks] = useState<Record<string, boolean>>({});

  const refresh = async () => {
    try { setItems(await call<VaultItem[]>("vault_list")); } catch (e) { setError(describe(e)); }
  };
  useEffect(() => { refresh(); }, []);

  const store = async () => {
    setError(null);
    try {
      await call("vault_store", { reference, label, kind, username, secret });
      setReference(""); setLabel(""); setUsername(""); setSecret("");
      refresh();
    } catch (e) { setError(describe(e)); }
  };

  return (
    <div className="max-w-[1000px] space-y-4">
      <Panel title="Where your credentials live">
        <p className="text-[12.5px] text-muted max-w-[86ch]">
          Values are handed to Windows Credential Manager, the same place Windows
          keeps its own saved passwords. DevWorkstation's database stores only the
          label you see below. Nothing here can be read back into this screen — a
          credential leaves the vault only when a module is opening the connection
          it belongs to.
        </p>
      </Panel>

      {error && <ErrorNotice error={error} />}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel title="Save a credential">
          <div className="space-y-3">
            <Field label="Reference" hint="How modules find this entry. For example api:openai or database:orbit.">
              <input className="field mono" value={reference} onChange={(e) => setReference(e.target.value)} />
            </Field>
            <Field label="Label"><input className="field" value={label} onChange={(e) => setLabel(e.target.value)} /></Field>
            <Field label="Kind">
              <select className="field" value={kind} onChange={(e) => setKind(e.target.value)}>
                <option value="ssh">SSH</option>
                <option value="cpanel">cPanel</option>
                <option value="api">API key</option>
                <option value="database">Database</option>
                <option value="token">Token</option>
                <option value="website">Website</option>
              </select>
            </Field>
            <Field label="Username"><input className="field mono" value={username} onChange={(e) => setUsername(e.target.value)} /></Field>
            <Field label="Value"><input className="field mono" type="password" value={secret} onChange={(e) => setSecret(e.target.value)} /></Field>
            <button className="btn btn-primary" onClick={store} disabled={!reference || !secret}>Save to vault</button>
          </div>
        </Panel>

        <Panel title={`Stored (${items.length})`} className="max-h-[440px]">
          {items.length === 0 ? (
            <Empty title="Vault is empty" hint="Add server passwords, API keys or database logins so no module needs them written into a file." />
          ) : (
            <ul className="divide-y divide-line">
              {items.map((i) => (
                <li key={i.reference} className="py-2">
                  <div className="flex items-center gap-2">
                    <span className="text-[13px]">{i.label || i.reference}</span>
                    <span className="text-[11px] text-muted">{i.kind}</span>
                    <span className="ml-auto flex gap-1.5">
                      <button className="btn !py-0.5 !px-2 !text-[11px]"
                        onClick={async () => {
                          const ok = await call<boolean>("vault_verify", { reference: i.reference });
                          setChecks((c) => ({ ...c, [i.reference]: ok }));
                        }}>Check</button>
                      <button className="btn btn-danger !py-0.5 !px-2 !text-[11px]"
                        onClick={async () => { await call("vault_delete", { reference: i.reference }); refresh(); }}>
                        Remove
                      </button>
                    </span>
                  </div>
                  <p className="mono text-[11.5px] text-muted">{i.reference}{i.username ? ` · ${i.username}` : ""}</p>
                  {i.reference in checks && (
                    <p className="text-[11.5px]" style={{ color: checks[i.reference] ? "var(--ok)" : "var(--err)" }}>
                      {checks[i.reference] ? "Present and readable" : "Missing from the credential store"}
                    </p>
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
