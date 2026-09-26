import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, PdfInfo } from "../lib/api";
import { Panel, ErrorNotice, Stat, Busy, Empty, Row } from "../components/ui";
import { usePersistedState } from "../lib/persist";

export default function PdfStudio() {
  const [info, setInfo] = usePersistedState<PdfInfo | null>("pdf.info", null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [text, setText] = useState<{ page: number; body: string } | null>(null);
  const [done, setDone] = useState("");

  const openFile = async () => {
    const f = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (typeof f !== "string") return;
    setBusy(true); setError(null); setText(null); setDone("");
    try {
      setInfo(await call<PdfInfo>("pdf_inspect", { path: f }));
      setSelected(new Set());
    } catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const showText = async (page: number) => {
    setError(null);
    try {
      setText({ page, body: await call<string>("pdf_extract_text", { path: info!.path, page }) });
    } catch (e) { setError(describe(e)); }
  };

  const target = async (suffix: string) =>
    save({ defaultPath: info!.path.replace(/\.pdf$/i, `-${suffix}.pdf`), filters: [{ name: "PDF", extensions: ["pdf"] }] });

  const withResult = async (fn: () => Promise<string>, label: string) => {
    setBusy(true); setError(null);
    try { const out = await fn(); setDone(`${label} → ${out}`); }
    catch (e) { setError(describe(e)); }
    finally { setBusy(false); }
  };

  const pages = [...selected].sort((a, b) => a - b);

  return (
    <div className="max-w-[1150px] space-y-4">
      <Panel title="Open a document" actions={<button className="btn btn-primary !py-1 !text-[12px]" onClick={openFile}>Open PDF</button>}>
        <p className="text-[12.5px] text-muted">
          Everything here happens on this computer. A PDF describes a printed
          page rather than a document you type into, so DevWorkstation checks
          each page first and tells you what can actually be changed.
        </p>
        {done && <p className="text-[12.5px] mt-2 mono selectable" style={{ color: "var(--ok)" }}>{done}</p>}
      </Panel>

      {busy && <Busy label="Working on the document" />}
      {error && <ErrorNotice error={error} />}
      {!info && !busy && !error && <Empty title="No document open" hint="Open a PDF to see its pages, extract text or rearrange it." />}

      {info && (
        <>
          <Panel title="Document">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-3">
              <Stat label="Pages" value={info.page_count} />
              <Stat label="Size" value={`${(info.bytes / 1024 / 1024).toFixed(2)} MB`} />
              <Stat label="PDF version" value={info.version} />
              <Stat label="Protected" value={info.encrypted ? "Yes" : "No"} tone={info.encrypted ? "var(--warn)" : undefined} />
            </div>
            <Row label="File" value={info.path} />
            <Row label="Title" value={info.title || "—"} />
            <Row label="Author" value={info.author || "—"} />
            <Row label="Made with" value={info.producer || "—"} />
            <p className="text-[12.5px] mt-3" style={{ color: info.needs_ocr ? "var(--warn)" : "var(--muted)" }}>
              {info.note}
            </p>
          </Panel>

          <Panel
            title={`Pages — ${selected.size} selected`}
            actions={
              <>
                <button className="btn !py-1 !text-[12px]" onClick={() => setSelected(new Set(info.pages.map((p) => p.number)))}>Select all</button>
                <button className="btn !py-1 !text-[12px]" onClick={() => setSelected(new Set())}>Clear</button>
              </>
            }
          >
            <div className="grid grid-cols-[repeat(auto-fill,minmax(112px,1fr))] gap-2">
              {info.pages.map((p) => {
                const on = selected.has(p.number);
                const tone = p.classification === "text" ? "var(--ok)"
                  : p.classification === "image" ? "var(--warn)"
                  : p.classification === "mixed" ? "var(--info)" : "var(--muted)";
                return (
                  <button
                    key={p.number}
                    onClick={() => {
                      const next = new Set(selected);
                      on ? next.delete(p.number) : next.add(p.number);
                      setSelected(next);
                    }}
                    onDoubleClick={() => showText(p.number)}
                    className="border rounded p-2 text-left"
                    style={{ borderColor: on ? "var(--accent)" : "var(--line)", background: on ? "var(--raised)" : "transparent" }}
                  >
                    <div className="mono text-[13px]">{p.number}</div>
                    <div className="text-[11px]" style={{ color: tone }}>{p.classification}</div>
                    <div className="text-[10.5px] text-muted mono">
                      {p.characters > 0 ? `${p.characters} chars` : `${p.images} images`}
                    </div>
                  </button>
                );
              })}
            </div>
            <p className="text-[11.5px] text-muted mt-2">Double-click a page to read its text.</p>
          </Panel>

          <Panel title="Change the document">
            <div className="flex flex-wrap gap-2">
              <button className="btn" disabled={!pages.length}
                onClick={() => withResult(async () => {
                  const dest = await target("trimmed"); if (!dest) throw new Error("cancelled");
                  return call<string>("pdf_delete_pages", { path: info.path, pages, destination: dest });
                }, "Pages removed")}>
                Remove selected pages
              </button>

              <button className="btn" disabled={!pages.length}
                onClick={() => withResult(async () => {
                  const dest = await target("extract"); if (!dest) throw new Error("cancelled");
                  return call<string>("pdf_split", { path: info.path, keep: pages, destination: dest });
                }, "Pages extracted")}>
                Save selected as a new PDF
              </button>

              <button className="btn" disabled={!pages.length}
                onClick={() => withResult(async () => {
                  const dest = await target("rotated"); if (!dest) throw new Error("cancelled");
                  return call<string>("pdf_rotate_pages", { path: info.path, pages, degrees: 90, destination: dest });
                }, "Pages rotated")}>
                Rotate selected 90°
              </button>

              <button className="btn"
                onClick={() => withResult(async () => {
                  const more = await open({ multiple: true, filters: [{ name: "PDF", extensions: ["pdf"] }] });
                  const list = Array.isArray(more) ? more : more ? [more] : [];
                  if (!list.length) throw new Error("cancelled");
                  const dest = await save({ defaultPath: "combined.pdf", filters: [{ name: "PDF", extensions: ["pdf"] }] });
                  if (!dest) throw new Error("cancelled");
                  return call<string>("pdf_merge", { sources: [info.path, ...list], destination: dest });
                }, "Documents combined")}>
                Combine with other PDFs
              </button>
            </div>
            <p className="text-[11.5px] text-muted mt-2">
              Each operation writes a new file. The document you opened is never overwritten.
            </p>
          </Panel>

          {text && (
            <Panel title={`Text on page ${text.page}`} actions={<button className="btn !py-1 !text-[12px]" onClick={() => setText(null)}>Close</button>}>
              <pre className="mono text-[12px] whitespace-pre-wrap max-h-[340px] overflow-auto selectable">
                {text.body || "(this page holds no selectable text)"}
              </pre>
            </Panel>
          )}
        </>
      )}
    </div>
  );
}
