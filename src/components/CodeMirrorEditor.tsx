import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, basicSetup } from "codemirror";
import { keymap } from "@codemirror/view";
import { indentWithTab } from "@codemirror/commands";
import { oneDark } from "@codemirror/theme-one-dark";
import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { java } from "@codemirror/lang-java";
import { php } from "@codemirror/lang-php";
import { StreamLanguage } from "@codemirror/language";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";

/** Picks a CodeMirror language extension from a file extension. Falls back
 * to no highlighting (still a perfectly usable plain editor) for anything
 * not in this list rather than guessing wrong. */
function languageFor(ext: string) {
  switch (ext) {
    case "js": case "jsx": case "mjs": case "cjs":
      return javascript({ jsx: ext === "jsx" });
    case "ts": case "tsx":
      return javascript({ jsx: ext === "tsx", typescript: true });
    case "py": case "pyw": return python();
    case "html": case "htm": return html();
    case "css": return css();
    case "java": return java();
    case "php": return php();
    case "rb": return StreamLanguage.define(ruby);
    default: return null;
  }
}

export default function CodeEditor({
  value, extension, onChange, onSaveKey,
}: {
  value: string;
  extension: string;
  onChange: (next: string) => void;
  /** Ctrl/Cmd+S inside the editor — parent decides what "save" means. */
  onSaveKey: () => void;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onSaveKeyRef = useRef(onSaveKey);
  onChangeRef.current = onChange;
  onSaveKeyRef.current = onSaveKey;

  // Rebuilt whenever the open file (and so its language) changes — cheap
  // enough for source files and far simpler than reconfiguring compartments.
  useEffect(() => {
    if (!hostRef.current) return;
    const lang = languageFor(extension);
    const state = EditorState.create({
      doc: value,
      extensions: [
        basicSetup,
        oneDark,
        keymap.of([
          indentWithTab,
          { key: "Mod-s", run: () => { onSaveKeyRef.current(); return true; } },
        ]),
        ...(lang ? [lang] : []),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onChangeRef.current(update.state.doc.toString());
        }),
        EditorView.theme({
          "&": { height: "100%", fontSize: "13px" },
          ".cm-scroller": { fontFamily: "var(--mono, ui-monospace, monospace)", overflow: "auto" },
        }),
      ],
    });
    const view = new EditorView({ state, parent: hostRef.current });
    viewRef.current = view;
    return () => view.destroy();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [extension, hostRef.current]);

  // External changes (switching tabs, reverting) replace the document
  // without tearing the view down, so cursor/scroll only reset when the
  // file itself changes (handled by the effect above via `extension`+key).
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    const current = view.state.doc.toString();
    if (current !== value) {
      view.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value]);

  return <div ref={hostRef} className="h-full min-h-0" />;
}
