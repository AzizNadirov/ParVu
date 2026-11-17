// SQL / Expression editor built on CodeMirror 6.
import { useEffect, useRef } from "react";
import { EditorState, Compartment } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, placeholder as cmPlaceholder, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { sql, PostgreSQL } from "@codemirror/lang-sql";
import { bracketMatching, syntaxHighlighting } from "@codemirror/language";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { exprCompletions, type ExprSchema } from "../lib/exprComplete";
import { exprLanguage, highlight } from "../lib/exprLanguage";

export type EditorMode = "sql" | "expression";

export interface QueryEditorProps {
  mode: EditorMode;
  value: string;
  onChange(value: string): void;
  onRun(): void;
  /** Tables and their columns, for SQL and expression autocomplete. */
  schema: ExprSchema;
  keywords: string[];
  fontFamily: string;
  fontSize: number;
  placeholder?: string;
}

export function QueryEditor(props: QueryEditorProps) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const lang = useRef(new Compartment());
  const font = useRef(new Compartment());
  const latest = useRef(props);
  latest.current = props;

  const languageFor = (mode: EditorMode, schema: ExprSchema) => {
    if (mode === "sql") {
      const sqlSchema: Record<string, string[]> = {};
      for (const t of schema.tables) sqlSchema[t.name] = t.columns;
      return [
        sql({ dialect: PostgreSQL, schema: sqlSchema, upperCaseKeywords: true }),
        autocompletion({ activateOnTyping: true }),
      ];
    }
    return [exprLanguage, autocompletion({ override: [exprCompletions(() => latest.current.schema)], activateOnTyping: true })];
  };

  const fontTheme = (family: string, size: number) =>
    EditorView.theme({
      "&": { fontSize: `${size}pt` },
      ".cm-content, .cm-gutters": { fontFamily: `${family}, var(--pv-layout-code-font-family), monospace` },
    });

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: props.value,
        extensions: [
          lineNumbers(),
          history(),
          bracketMatching(),
          closeBrackets(),
          highlightActiveLine(),
          highlightSelectionMatches(),
          syntaxHighlighting(highlight),
          EditorView.lineWrapping,
          cmPlaceholder(props.placeholder ?? ""),
          keymap.of([
            { key: "Mod-Enter", run: () => (latest.current.onRun(), true) },
            ...closeBracketsKeymap,
            ...completionKeymap,
            ...searchKeymap,
            ...historyKeymap,
            indentWithTab,
            ...defaultKeymap,
          ]),
          lang.current.of(languageFor(props.mode, props.schema)),
          font.current.of(fontTheme(props.fontFamily, props.fontSize)),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) latest.current.onChange(u.state.doc.toString());
          }),
        ],
      }),
    });
    view.current = v;
    return () => v.destroy();
    // Created once; later prop changes are applied by the effects below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    view.current?.dispatch({ effects: lang.current.reconfigure(languageFor(props.mode, props.schema)) });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.mode, props.schema]);

  useEffect(() => {
    view.current?.dispatch({ effects: font.current.reconfigure(fontTheme(props.fontFamily, props.fontSize)) });
  }, [props.fontFamily, props.fontSize]);

  // External value changes (switching tabs, reset) replace the document.
  useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== props.value) {
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: props.value } });
    }
  }, [props.value]);

  return <div className="query-editor" ref={host} />;
}
