// Autocomplete for the expression language (port of the Python
// ExpressionEditor completer): table names, `table[column]`, functions with
// their signatures and docs, and type methods after a dot.
import type { CompletionContext, CompletionResult, Completion } from "@codemirror/autocomplete";
import type { FunctionDef } from "./api";

export interface ExprSchema {
  tables: { name: string; columns: string[] }[];
  functions: FunctionDef[];
  methods: string[];
}

export function signature(f: FunctionDef): string {
  const params = f.params.map((p) => (p.required ? p.name : `${p.name}?`)).join(", ");
  return `${f.name}(${params}) → ${f.return_type}`;
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
        detail: f.return_type,
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
