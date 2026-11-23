// SQL shown read-only (the AI panel): split into spans with token classes
// (`tok-keyword`, `tok-string`…) by the SQL editor's own parser. The text
// stays text; styles.css colours the classes with the editor's theme colours.
import { PostgreSQL, sql } from "@codemirror/lang-sql";
import { classHighlighter, highlightTree } from "@lezer/highlight";

const parser = sql({ dialect: PostgreSQL }).language.parser;

export interface SqlSpan {
  text: string;
  cls: string;
}

export function sqlSpans(text: string): SqlSpan[] {
  const out: SqlSpan[] = [];
  let at = 0;
  const push = (to: number, cls: string) => {
    if (to > at) out.push({ text: text.slice(at, to), cls });
    at = Math.max(at, to);
  };
  try {
    highlightTree(parser.parse(text), classHighlighter, (from, to, classes) => {
      push(from, "");
      push(to, classes);
    });
  } catch {
    // Unstyled is fine.
  }
  push(text.length, "");
  return out;
}
