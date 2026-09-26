import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { call, describe, AppError, blankCv, CvDocument, CvSection } from "../lib/api";
import { Panel, ErrorNotice, Field, Empty } from "../components/ui";
import { usePersistedState } from "../lib/persist";

interface Summary { id: number; name: string; updated_at: string }

export default function CvBuilder() {
  const [list, setList] = useState<Summary[]>([]);
  const [id, setId] = usePersistedState<number | null>("cv.id", null);
  const [name, setName] = usePersistedState<string>("cv.name", "Untitled CV");
  const [doc, setDoc] = usePersistedState<CvDocument>("cv.doc", blankCv());
  const [error, setError] = useState<AppError | null>(null);
  const [status, setStatus] = useState("");

  const refresh = async () => {
    try { setList(await call<Summary[]>("cv_list")); } catch (e) { setError(describe(e)); }
  };
  useEffect(() => { refresh(); }, []);

  const open = async (cvId: number) => {
    try {
      setDoc(await call<CvDocument>("cv_load", { id: cvId }));
      setId(cvId);
      setName(list.find((l) => l.id === cvId)?.name ?? "CV");
      setStatus("");
    } catch (e) { setError(describe(e)); }
  };

  const store = async () => {
    setError(null);
    try {
      const newId = await call<number>("cv_save", { id, name, document: doc });
      setId(newId);
      setStatus("Saved");
      refresh();
    } catch (e) { setError(describe(e)); }
  };

  const exportPdf = async () => {
    setError(null);
    try {
      const target = await save({
        defaultPath: `${(doc.full_name || name).replace(/[^\w -]/g, "")}.pdf`,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!target) return;
      await call<string>("cv_export_pdf", { document: doc, destination: target });
      setStatus("Exported to PDF");
    } catch (e) { setError(describe(e)); }
  };

  const patch = (p: Partial<CvDocument>) => setDoc({ ...doc, ...p });
  const patchStyle = (p: Partial<CvDocument["style"]>) => setDoc({ ...doc, style: { ...doc.style, ...p } });

  const setSection = (i: number, s: CvSection) => {
    const sections = [...doc.sections];
    sections[i] = s;
    patch({ sections });
  };

  const move = (i: number, by: number) => {
    const j = i + by;
    if (j < 0 || j >= doc.sections.length) return;
    const sections = [...doc.sections];
    [sections[i], sections[j]] = [sections[j], sections[i]];
    patch({ sections });
  };

  return (
    <div className="grid grid-cols-1 xl:grid-cols-[230px_1fr_250px] gap-4 max-w-[1400px]">
      <Panel
        title="Your CVs"
        actions={
          <button className="btn !py-1 !text-[12px]" onClick={() => { setId(null); setDoc(blankCv()); setName("Untitled CV"); }}>
            New
          </button>
        }
        className="max-h-[560px]"
      >
        {list.length === 0 ? (
          <Empty title="No CVs saved yet" hint="Fill in the form and save to keep it on this computer." />
        ) : (
          <ul className="text-[12.5px] divide-y divide-line">
            {list.map((c) => (
              <li key={c.id} className="py-1.5 flex items-center gap-2">
                <button
                  className="flex-1 text-left truncate"
                  style={{ color: c.id === id ? "var(--accent)" : undefined }}
                  onClick={() => open(c.id)}
                >
                  {c.name}
                  <span className="block mono text-[11px] text-muted">{c.updated_at}</span>
                </button>
                <button
                  className="btn !px-2 !py-0.5 !text-[11px]"
                  onClick={async () => { await call("cv_duplicate", { id: c.id }); refresh(); }}
                >
                  Copy
                </button>
                <button
                  className="btn btn-danger !px-2 !py-0.5 !text-[11px]"
                  onClick={async () => {
                    await call("cv_delete", { id: c.id });
                    if (c.id === id) { setId(null); setDoc(blankCv()); }
                    refresh();
                  }}
                >
                  Delete
                </button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <div className="space-y-4 min-w-0">
        {error && <ErrorNotice error={error} />}

        <Panel
          title={<input className="field !py-1 max-w-[260px]" value={name} onChange={(e) => setName(e.target.value)} />}
          actions={
            <>
              {status && <span className="text-[12px]" style={{ color: "var(--ok)" }}>{status}</span>}
              <button className="btn" onClick={store}>Save</button>
              <button className="btn btn-primary" onClick={exportPdf}>Export PDF</button>
            </>
          }
        >
          <div className="grid grid-cols-2 gap-3">
            <Field label="Full name"><input className="field" value={doc.full_name} onChange={(e) => patch({ full_name: e.target.value })} /></Field>
            <Field label="Headline"><input className="field" value={doc.headline} onChange={(e) => patch({ headline: e.target.value })} placeholder="Full-stack developer" /></Field>
            <Field label="Email"><input className="field" value={doc.email} onChange={(e) => patch({ email: e.target.value })} /></Field>
            <Field label="Phone"><input className="field" value={doc.phone} onChange={(e) => patch({ phone: e.target.value })} /></Field>
            <Field label="Location"><input className="field" value={doc.location} onChange={(e) => patch({ location: e.target.value })} /></Field>
            <Field label="Website"><input className="field" value={doc.website} onChange={(e) => patch({ website: e.target.value })} /></Field>
          </div>
          <div className="mt-3">
            <Field label="Profile">
              <textarea className="field h-[76px] resize-y" value={doc.summary} onChange={(e) => patch({ summary: e.target.value })} />
            </Field>
          </div>
        </Panel>

        {doc.sections.map((section, i) => (
          <Panel
            key={section.id}
            title={
              <input
                className="field !py-1 max-w-[220px]"
                value={section.heading}
                onChange={(e) => setSection(i, { ...section, heading: e.target.value })}
              />
            }
            actions={
              <>
                <select
                  className="field !py-1 !w-auto"
                  value={section.layout}
                  onChange={(e) => setSection(i, { ...section, layout: e.target.value as CvSection["layout"] })}
                >
                  <option value="items">Entries with bullets</option>
                  <option value="list">Simple list</option>
                  <option value="text">Paragraph</option>
                </select>
                <button className="btn !px-2 !py-0.5" onClick={() => move(i, -1)}>↑</button>
                <button className="btn !px-2 !py-0.5" onClick={() => move(i, 1)}>↓</button>
                <button
                  className="btn btn-danger !px-2 !py-0.5 !text-[11px]"
                  onClick={() => patch({ sections: doc.sections.filter((_, k) => k !== i) })}
                >
                  Remove
                </button>
              </>
            }
          >
            {section.layout === "items" && (
              <div className="space-y-3">
                {section.items.map((item, j) => (
                  <div key={j} className="border border-line rounded p-2.5">
                    <div className="grid grid-cols-2 gap-2">
                      <input className="field" placeholder="Role or qualification" value={item.title}
                        onChange={(e) => { const items = [...section.items]; items[j] = { ...item, title: e.target.value }; setSection(i, { ...section, items }); }} />
                      <input className="field" placeholder="Organisation" value={item.subtitle}
                        onChange={(e) => { const items = [...section.items]; items[j] = { ...item, subtitle: e.target.value }; setSection(i, { ...section, items }); }} />
                      <input className="field" placeholder="2022 – present" value={item.period}
                        onChange={(e) => { const items = [...section.items]; items[j] = { ...item, period: e.target.value }; setSection(i, { ...section, items }); }} />
                      <input className="field" placeholder="City" value={item.location}
                        onChange={(e) => { const items = [...section.items]; items[j] = { ...item, location: e.target.value }; setSection(i, { ...section, items }); }} />
                    </div>
                    <textarea
                      className="field mt-2 h-[68px] resize-y"
                      placeholder="One achievement per line"
                      value={item.bullets.join("\n")}
                      onChange={(e) => {
                        const items = [...section.items];
                        items[j] = { ...item, bullets: e.target.value.split("\n") };
                        setSection(i, { ...section, items });
                      }}
                    />
                    <button
                      className="btn btn-danger !py-0.5 !text-[11px] mt-2"
                      onClick={() => setSection(i, { ...section, items: section.items.filter((_, k) => k !== j) })}
                    >
                      Remove entry
                    </button>
                  </div>
                ))}
                <button
                  className="btn"
                  onClick={() => setSection(i, { ...section, items: [...section.items, { title: "", subtitle: "", period: "", location: "", bullets: [""] }] })}
                >
                  Add entry
                </button>
              </div>
            )}

            {section.layout === "list" && (
              <textarea
                className="field h-[90px] resize-y"
                placeholder="One item per line"
                value={section.entries.join("\n")}
                onChange={(e) => setSection(i, { ...section, entries: e.target.value.split("\n") })}
              />
            )}

            {section.layout === "text" && (
              <textarea
                className="field h-[90px] resize-y"
                value={section.text}
                onChange={(e) => setSection(i, { ...section, text: e.target.value })}
              />
            )}
          </Panel>
        ))}

        <button
          className="btn"
          onClick={() => patch({
            sections: [...doc.sections, {
              id: `s${Date.now()}`, heading: "New section", layout: "items",
              items: [], entries: [], text: "",
            }],
          })}
        >
          Add section
        </button>
      </div>

      <Panel title="Page design" className="max-h-[560px]">
        <div className="space-y-3">
          <Field label="Typeface">
            <select className="field" value={doc.style.font} onChange={(e) => patchStyle({ font: e.target.value })}>
              <option value="Helvetica">Helvetica</option>
              <option value="Times">Times</option>
              <option value="Courier">Courier</option>
            </select>
          </Field>
          <Field label="Page size">
            <select className="field" value={doc.style.page} onChange={(e) => patchStyle({ page: e.target.value })}>
              <option value="A4">A4</option>
              <option value="Letter">US Letter</option>
              <option value="Legal">US Legal</option>
            </select>
          </Field>
          <Field label={`Name size — ${doc.style.name_size}pt`}>
            <input type="range" min={14} max={34} value={doc.style.name_size} className="w-full"
              onChange={(e) => patchStyle({ name_size: +e.target.value })} />
          </Field>
          <Field label={`Heading size — ${doc.style.heading_size}pt`}>
            <input type="range" min={9} max={18} value={doc.style.heading_size} className="w-full"
              onChange={(e) => patchStyle({ heading_size: +e.target.value })} />
          </Field>
          <Field label={`Body size — ${doc.style.body_size}pt`}>
            <input type="range" min={8} max={13} step={0.5} value={doc.style.body_size} className="w-full"
              onChange={(e) => patchStyle({ body_size: +e.target.value })} />
          </Field>
          <Field label={`Line spacing — ${doc.style.line_spacing.toFixed(2)}`}>
            <input type="range" min={1.05} max={1.8} step={0.05} value={doc.style.line_spacing} className="w-full"
              onChange={(e) => patchStyle({ line_spacing: +e.target.value })} />
          </Field>
          <Field label={`Margin — ${doc.style.margin_mm}mm`}>
            <input type="range" min={10} max={32} value={doc.style.margin_mm} className="w-full"
              onChange={(e) => patchStyle({ margin_mm: +e.target.value })} />
          </Field>
          <Field label="Accent colour" hint="Used for your name, headings and the rule beneath the header.">
            <input type="color" className="field !p-1 h-9" value={doc.style.accent}
              onChange={(e) => patchStyle({ accent: e.target.value })} />
          </Field>
        </div>
      </Panel>
    </div>
  );
}
