// Extra completions for the SQL editor: the user's custom functions, with
// their signature and description (DuckDB's own functions and the tables
// come from @codemirror/lang-sql).
import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";

/** A custom function as the SQL editor offers it. */
export interface SqlFunctionInfo {
  name: string;
  /** Shown beside the name ("custom function"). */
  detail: string;
  signature: string;
  description: string;
}

/** A word being typed at the cursor that isn't inside a string, a quoted
 * name or a comment. */
export function inCode(before: string): boolean {
  const line = before.slice(before.lastIndexOf("\n") + 1);
  if (/--/.test(line.replace(/'[^']*'/g, "''"))) return false;
  const singles = (before.match(/'/g) ?? []).length;
  const doubles = (before.match(/"/g) ?? []).length;
  return singles % 2 === 0 && doubles % 2 === 0;
}

export function sqlCustomCompletions(getFunctions: () => SqlFunctionInfo[]) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const fns = getFunctions();
    if (!fns.length) return null;
    const word = ctx.matchBefore(/[A-Za-z_][A-Za-z0-9_]*/);
    if (!word || (word.from === word.to && !ctx.explicit)) return null;
    // After a dot is a column of a table, not a function.
    const before = ctx.state.sliceDoc(Math.max(0, word.from - 2000), word.from);
    if (before.endsWith(".") || !inCode(before)) return null;
    const options: Completion[] = fns.map((f) => ({
      label: f.name,
      type: "function",
      detail: f.detail,
      boost: 1,
      info: () => {
        const el = document.createElement("div");
        el.className = "fn-doc";
        const sig = document.createElement("code");
        sig.textContent = f.signature;
        el.append(sig);
        if (f.description) {
          const desc = document.createElement("div");
          desc.textContent = f.description;
          el.append(desc);
        }
        return el;
      },
      apply: `${f.name}(`,
    }));
    return { from: word.from, options, validFor: /^[A-Za-z0-9_]*$/ };
  };
}
