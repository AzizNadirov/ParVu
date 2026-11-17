// Autocomplete for the expression language (port of the Python
// ExpressionEditor completer): table names, `table[column]`, functions with
// their signatures and docs, and type methods after a dot.
import type { CompletionContext, CompletionResult, Completion } from "@codemirror/autocomplete";
import type { EditorView } from "@codemirror/view";
import type { FunctionDef } from "./api";

export interface ExprSchema {
  tables: { name: string; columns: string[] }[];
  functions: FunctionDef[];
  methods: string[];
  /** Columns a bare `[` offers: the formula's table, or a custom
   * function's parameters in an expression body. */
  bare?: string[];
}

export function signature(f: FunctionDef): string {
  const params = f.params
    .map((p) => {
      const name = p.required ? p.name : `${p.name}?`;
      return p.accepts?.length ? `${name}: ${p.accepts.join(" | ")}` : name;
    })
    .join(", ");
  return `${f.name}(${params}) → ${f.return_type}`;
}

/** What a function's completion shows beside its name. */
export function functionDetail(f: FunctionDef): string {
  if (!f.custom) return f.return_type;
  return f.aggregate ? `custom · total · ${f.return_type}` : `custom · ${f.return_type}`;
}

/** The text a bare `[` completes to: `[col]`, or `["a b"]` with an unusual name. */
export function bareRef(column: string): string {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(column) ? `${column}]` : `"${column.replace(/"/g, "")}"]`;
}

/** The partial column name after a bare `[` (not `table[`) at the end of `before`. */
export function bareRefPrefix(before: string): string | null {
  const m = /(^|[^A-Za-z0-9_\]\[])\[\s*"?([^\]":]*)$/.exec(before);
  return m ? m[2] : null;
}

function quoteColumn(c: string): string {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(c) ? c : `'${c.replace(/'/g, "")}'`;
}

export function exprCompletions(getSchema: () => ExprSchema) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const schema = getSchema();
    const before = ctx.state.sliceDoc(Math.max(0, ctx.pos - 200), ctx.pos);

    // table[ col  -> columns of that table
    const colMatch = /([A-Za-z_][A-Za-z0-9_]*)\[\s*['"]?([^\]'"]*)$/.exec(before);
    if (colMatch) {
      const table = schema.tables.find((t) => t.name === colMatch[1]);
      if (!table) return null;
      const from = ctx.pos - colMatch[2].length;
      return {
        from,
        options: table.columns.map((c) => ({ label: c, type: "property", apply: quoteColumn(c) + "]" })),
        validFor: /^[^\]'"]*$/,
      };
    }

    // [ col  -> the bare columns (formula's table, function parameters)
    const bare = schema.bare ? bareRefPrefix(before) : null;
    if (bare !== null && schema.bare) {
      const quoted = /"[^\]"]*$/.test(before);
      return {
        from: ctx.pos - bare.length - (quoted ? 1 : 0),
        options: schema.bare.map((c) => ({
          label: c,
          type: "property",
          // Over the `]` the editor closed the bracket with, if any.
          apply: (view: EditorView, _c: Completion, from: number, to: number) => {
            const insert = bareRef(c);
            const end = view.state.sliceDoc(to, to + 1) === "]" ? to + 1 : to;
            view.dispatch({ changes: { from, to: end, insert }, selection: { anchor: from + insert.length } });
          },
        })),
        validFor: /^"?[^\]":]*$/,
      };
    }

    // expr.meth -> methods
    const methMatch = /\.\s*([A-Za-z_]*)$/.exec(before);
    if (methMatch && /[\])A-Za-z0-9_'"]\s*\.\s*[A-Za-z_]*$/.test(before)) {
      return {
        from: ctx.pos - methMatch[1].length,
        options: schema.methods.map((m) => ({ label: m, type: "method", apply: `${m}(` })),
        validFor: /^[A-Za-z_]*$/,
      };
    }

    const word = ctx.matchBefore(/[A-Za-z_][A-Za-z0-9_]*/);
    if (!word || (word.from === word.to && !ctx.explicit)) return null;
    const options: Completion[] = [
      ...schema.tables.map((t) => ({ label: t.name, type: "class", detail: "table", apply: `${t.name}[` })),
      ...schema.functions.map((f) => ({
        label: f.name,
        type: "function",
        detail: functionDetail(f),
        info: () => {
          const el = document.createElement("div");
          el.className = "fn-doc";
          const sig = document.createElement("code");
          sig.textContent = signature(f);
          const desc = document.createElement("div");
          desc.textContent = f.description;
          el.append(sig, desc);
          if (f.example) {
            const ex = document.createElement("pre");
            ex.textContent = f.example;
            el.append(ex);
          }
          return el;
        },
        apply: `${f.name}(`,
      })),
      ...["TRUE", "FALSE", "NULL", "AND", "OR", "NOT"].map((k) => ({ label: k, type: "keyword" })),
    ];
    return { from: word.from, options, validFor: /^[A-Za-z0-9_]*$/ };
  };
}
