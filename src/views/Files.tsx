import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, FsEntry } from "../lib/api";
import { Panel, ErrorNotice, Empty, Busy } from "../components/ui";
import { usePersistedState } from "../lib/persist";

export default function Files({ onAnalyze, onEdit }: { onAnalyze: (path: string) => void; onEdit?: (path: string) => void }) {
  const [cwd, setCwd] = usePersistedState<string>("files.cwd", "");
  const [entries, setEntries] = useState<FsEntry[]>([]);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = usePersistedState<{ path: string; body: string } | null>("files.preview", null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string | null>(null);

  const go = async (path: string) => {
    setBusy(true); setError(null); setPreview(null); setSelected(null);
    try {
      setEntries(await call<FsEntry[]>("fs_list", { path }));
      setCwd(path);
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  // Restores the folder that was open last time (and any unsaved preview
  // edit sitting on top of it) rather than resetting to the home folder.
  useEffect(() => {
    (async () => {
      const target = cwd || (await call<string>("fs_home").catch(() => ""));
      if (!target) return;
      setBusy(true); setError(null);
      try { setEntries(await call<FsEntry[]>("fs_list", { path: target })); setCwd(target); }
      catch (e) { setError(describe(e)); }
      finally { setBusy(false); }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const up = () => {
    const parts = cwd.split(/[\\/]/);
    parts.pop();
    const parent = parts.join("\\");
    if (parent.length > 1) go(parent);
  };

  const openEntry = async (e: FsEntry) => {
    if (e.is_dir) return go(e.path);
    setSelected(e.path);
    if (["text", "code", "config"].includes(e.kind)) {
      try { setPreview({ path: e.path, body: await call<string>("fs_read_text", { path: e.path }) }); }
      catch (err) { setError(describe(err)); }
    } else {
      setPreview(null);
    }
  };

  const search = async () => {
    if (!query.trim()) return go(cwd);
    setBusy(true);
    try { setEntries(await call<FsEntry[]>("fs_search", { root: cwd, query })); }
    catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try { await fn(); go(cwd); } catch (e) { setError(describe(e)); }
  };

  return (
    <div className="max-w-[1250px] space-y-3">
      <Panel title="Files">
        <div className="flex gap-2 mb-2">
          <button className="btn" onClick={up}>Up</button>
          <input className="field mono" value={cwd} onChange={(e) => setCwd(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && go(cwd)} />
          <input className="field mono !w-[180px]" placeholder="search here" value={query}
            onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => e.key === "Enter" && search()} />
        </div>
        <div className="flex gap-2 flex-wrap">
          <button className="btn !py-1 !text-[12px]"
            onClick={() => act(async () => {
              const name = prompt("Folder name");
              if (name) await call("fs_create_dir", { path: `${cwd}\\${name}` });
            })}>New folder</button>
          <button className="btn !py-1 !text-[12px]" disabled={!selected}
            onClick={() => act(async () => {
              const base = selected!.split(/[\\/]/).pop() ?? "";
              const name = prompt("New name", base);
              if (name) await call("fs_rename", { from: selected, to: `${cwd}\\${name}` });
            })}>Rename</button>
          <button className="btn !py-1 !text-[12px]" disabled={!selected}
            onClick={() => act(async () => {
              const dest = await save({ defaultPath: "archive.zip", filters: [{ name: "ZIP", extensions: ["zip"] }] });
              if (dest) await call("fs_zip", { source: selected, destination: dest });
            })}>Compress</button>
          <button className="btn !py-1 !text-[12px]" disabled={!selected?.toLowerCase().endsWith(".zip")}
            onClick={() => act(async () => {
              await call("fs_unzip", { archive: selected, destination: selected!.replace(/\.zip$/i, "") });
            })}>Extract</button>
          <button className="btn !py-1 !text-[12px]" disabled={!selected} onClick={() => onAnalyze(selected!)}>
            Open in analyzer
          </button>
          {onEdit && (
            <button className="btn !py-1 !text-[12px]" disabled={!selected} onClick={() => onEdit(selected!)}>
              Open in editor
            </button>
          )}
          <button className="btn btn-danger !py-1 !text-[12px]" disabled={!selected}
            onClick={() => act(async () => {
              if (confirm(`Delete ${selected}? This cannot be undone.`)) await call("fs_delete", { path: selected });
            })}>Delete</button>
        </div>
      </Panel>

      {error && <ErrorNotice error={error} />}
      {busy && <Busy label="Reading folder" />}

      <div className="grid grid-cols-1 lg:grid-cols-[1fr_420px] gap-3">
        <Panel title={`${entries.length} items`} className="max-h-[520px]">
          {entries.length === 0 && !busy ? (
            <Empty title="This folder is empty" />
          ) : (
            <table className="w-full text-[12.5px]">
              <tbody>
                {entries.map((e) => (
                  <tr
                    key={e.path}
                    onClick={() => setSelected(e.path)}
                    onDoubleClick={() => openEntry(e)}
                    className="cursor-default border-b border-line last:border-0"
                    style={{ background: selected === e.path ? "var(--raised)" : undefined }}
                  >
                    <td className="py-1 w-[70px] text-[11px] text-muted">{e.kind}</td>
                    <td className="mono truncate max-w-[420px]">{e.name}</td>
                    <td className="text-right text-muted mono w-[90px]">
                      {e.is_dir ? "" : `${(e.size / 1024).toFixed(1)} KB`}
                    </td>
                    <td className="text-right text-muted mono w-[120px]">{e.modified}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Panel>

        <Panel
          title={preview ? preview.path.split(/[\\/]/).pop() : "Preview"}
          actions={preview && (
            <button className="btn !py-1 !text-[12px]"
              onClick={() => act(async () => { await call("fs_write_text", { path: preview.path, content: preview.body }); })}>
              Save
            </button>
          )}
          className="max-h-[520px]"
        >
          {preview ? (
            <textarea
              className="field mono !h-[440px] text-[11.5px] resize-none"
              value={preview.body}
              onChange={(e) => setPreview({ ...preview, body: e.target.value })}
            />
          ) : (
            <Empty title="Nothing to preview" hint="Double-click a text, code or settings file to read and edit it here." />
          )}
        </Panel>
      </div>
    </div>
  );
}
