// The expression language in CodeMirror: highlighting shared by the query
// editor and the formula bar.
import { HighlightStyle, StreamLanguage } from "@codemirror/language";
import { tags } from "@lezer/highlight";

export const highlight = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--pv-colors-editor-keyword)", fontWeight: "600" },
  { tag: [tags.string, tags.special(tags.string)], color: "var(--pv-colors-editor-string)" },
  { tag: [tags.number, tags.bool, tags.null], color: "var(--pv-colors-editor-number)" },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--pv-colors-editor-comment)", fontStyle: "italic" },
  { tag: [tags.function(tags.variableName), tags.standard(tags.name)], color: "var(--pv-colors-accent-primary)" },
  { tag: tags.operator, color: "var(--pv-colors-editor-keyword)" },
]);

/** Tokenizer for the expression language (for highlighting only). */
export const exprLanguage = StreamLanguage.define({
  token(stream) {
    if (stream.eatSpace()) return null;
    if (stream.match(/^#.*/)) return "comment";
    if (stream.match(/^'[^']*'?/) || stream.match(/^"[^"]*"?/)) return "string";
    if (stream.match(/^-?\d+(\.\d+)?/)) return "number";
    if (stream.match(/^(TRUE|FALSE|NULL)\b/i)) return "bool";
    if (stream.match(/^(AND|OR|NOT)\b/i)) return "keyword";
    if (stream.match(/^[A-Za-z_][A-Za-z0-9_]*(?=\s*\()/)) return "function";
    if (stream.match(/^[A-Za-z_][A-Za-z0-9_]*/)) return "variableName";
    if (stream.match(/^(==|!=|<>|<=|>=|&&|\|\||[-+*/<>=!.])/)) return "operator";
    stream.next();
    return null;
  },
  tokenTable: { function: tags.function(tags.variableName) },
});

