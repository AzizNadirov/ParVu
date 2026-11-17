// The formula bar above the grid, shown only while a column formula is
// written or changed (Column → New Formula Column, the header menu, the
// `+` header, a formula column's cell). The expression language with
// highlighting, autocomplete and coloured references.
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Compartment, EditorState, Prec, RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, keymap, placeholder as cmPlaceholder } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap, completionStatus } from "@codemirror/autocomplete";
import { bracketMatching, syntaxHighlighting } from "@codemirror/language";
import { exprCompletions, type ExprSchema } from "../lib/exprComplete";
import { exprLanguage, highlight } from "../lib/exprLanguage";
import { extendToRange, formulaRefs, refColors, refFor, refKey } from "../lib/formula";
import type { Translate } from "../lib/i18n";
import { Icon } from "./Icon";

export interface FormulaBarHandle {
  /** Insert text at the cursor (a column reference) and keep editing. */
  insert(text: string): void;
  /** Shift+click on a header: the reference before the cursor becomes a range up to `column`. */
  extendRange(column: string): void;
  focus(): void;
}

export interface FormulaBarProps {
  t: Translate;
  schema: ExprSchema;
  dark: boolean;
  /** A formula is being written. */
  editing: boolean;
  /** The column being edited, shown before the editor. */
  target: string | null;
  /** The column is new: its name can be typed. */
  isNew: boolean;
  onNameChange(name: string): void;
  /** The formula while editing; otherwise what the selected cell holds. */
  value: string;
  error: string;
  /** Where the error is in the text, when the compiler says. */
  errorAt?: number | null;
  onChange(value: string): void;
  onApply(): void;
  onCancel(): void;
}

/** Colours the column references, the same colours the grid shows. */
function refHighlighter(dark: () => boolean) {
  const build = (view: EditorView): DecorationSet => {
    const text = view.state.doc.toString();
    const refs = formulaRefs(text);
    const colors = refColors(refs, dark());
    const b = new RangeSetBuilder<Decoration>();
    for (const r of refs) {
      const c = colors.get(refKey(r));
      if (!c) continue;
      b.add(r.from, r.to, Decoration.mark({ attributes: { style: `color: ${c}; font-weight: 600; border-bottom: 2px solid ${c}` } }));
    }
    return b.finish();
  };
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = build(view);
      }
      update(u: ViewUpdate) {
        if (u.docChanged || u.viewportChanged) this.decorations = build(u.view);
      }
    },
    { decorations: (v) => v.decorations },
  );
}

/** The wavy underline at a compiler error; cleared by any edit. */
const setErrorAt = StateEffect.define<number | null>();
const errorMark = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    for (const e of tr.effects) {
      if (e.is(setErrorAt)) {
        const len = tr.state.doc.length;
        if (e.value === null || len === 0) return Decoration.none;
        const from = Math.min(e.value, len - 1);
        // The token there, or one character.
        const word = /^[\w"'\]\[]+/.exec(tr.state.doc.sliceString(from, Math.min(len, from + 40)));
        const to = from + Math.max(1, word ? word[0].length : 1);
        return Decoration.set([Decoration.mark({ class: "pv-formula-error" }).range(from, to)]);
      }
    }
    return tr.docChanged ? Decoration.none : marks;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/** Focus the editor now and again after the click that called us: the grid
 * takes the focus when its header click ends, and typing must go on in
 * the formula. */
function keepFocus(v: EditorView) {
  v.focus();
  window.setTimeout(() => v.focus(), 0);
  window.requestAnimationFrame(() => v.focus());
}

export const FormulaBar = forwardRef<FormulaBarHandle, FormulaBarProps>(function FormulaBar(props, ref) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const editable = useRef(new Compartment());
  const latest = useRef(props);
  latest.current = props;

  useEffect(() => {
    if (!host.current) return;
    const keys = Prec.highest(
      keymap.of([
        {
          key: "Enter",
          run: (v) => {
            if (!latest.current.editing || completionStatus(v.state) === "active") return false;
            latest.current.onApply();
            return true;
          },
        },
        { key: "Escape", run: () => (latest.current.editing ? (latest.current.onCancel(), true) : false) },
      ]),
    );
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: props.value,
        extensions: [
          keys,
          history(),
          bracketMatching(),
          closeBrackets(),
          exprLanguage,
          syntaxHighlighting(highlight),
          refHighlighter(() => latest.current.dark),
          errorMark,
          autocompletion({ override: [exprCompletions(() => latest.current.schema)], activateOnTyping: true }),
          EditorView.lineWrapping,
          cmPlaceholder(props.t("formula.placeholder")),
          keymap.of([...closeBracketsKeymap, ...completionKeymap, ...historyKeymap, ...defaultKeymap]),
          editable.current.of([EditorView.editable.of(props.editing), EditorState.readOnly.of(!props.editing)]),
          EditorView.updateListener.of((u) => {
            if (u.docChanged && latest.current.editing) latest.current.onChange(u.state.doc.toString());
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
    const v = view.current;
    if (!v) return;
    v.dispatch({ effects: editable.current.reconfigure([EditorView.editable.of(props.editing), EditorState.readOnly.of(!props.editing)]) });
    if (props.editing) {
      v.focus();
      v.dispatch({ selection: { anchor: v.state.doc.length } });
    }
  }, [props.editing]);

  useEffect(() => {
    const v = view.current;
    if (v && v.state.doc.toString() !== props.value) {
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: props.value } });
    }
  }, [props.value]);

  useEffect(() => {
    view.current?.dispatch({ effects: setErrorAt.of(props.errorAt ?? null) });
  }, [props.errorAt, props.error]);

  useImperativeHandle(ref, () => ({
    insert(text: string) {
      const v = view.current;
      if (!v) return;
      const { from, to } = v.state.selection.main;
      v.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length } });
      keepFocus(v);
    },
    extendRange(column: string) {
      const v = view.current;
      if (!v) return;
      const at = v.state.selection.main.head;
      const change = extendToRange(v.state.doc.toString(), at, column) ?? { from: at, to: at, insert: refFor(column) };
      v.dispatch({ changes: change, selection: { anchor: change.from + change.insert.length } });
      keepFocus(v);
    },
    focus() {
      view.current?.focus();
    },
  }));

  const t = props.t;
  return (
    <div className={`formula-bar${props.editing ? " editing" : ""}`}>
      <span className="formula-icon" aria-hidden="true">
        <Icon name="formula" />
      </span>
      {props.editing && props.isNew ? (
        <input
          className="formula-name"
          value={props.target ?? ""}
          aria-label={t("formula.name")}
          title={t("formula.name")}
          onChange={(e) => props.onNameChange(e.target.value)}
        />
      ) : (
        props.target && <span className="formula-target">{props.target}</span>
      )}
      {(props.editing || props.target) && <span className="formula-eq">=</span>}
      <div className="formula-editor" ref={host} />
      {props.editing && (
        <>
          <button className="icon" title={t("formula.apply")} aria-label={t("formula.apply")} onClick={props.onApply}>
            <Icon name="check" />
          </button>
          <button className="icon" title={t("formula.cancel")} aria-label={t("formula.cancel")} onClick={props.onCancel}>
            <Icon name="close" />
          </button>
        </>
      )}
      {props.editing && (
        <div className={`formula-hint${props.error ? " error-text" : ""}`}>{props.error || t("formula.hint")}</div>
      )}
    </div>
  );
});
