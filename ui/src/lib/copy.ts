// Text formats for copying grid selections (ports of DataTableView._copy_selection).
import type { Cell } from "./api";

export type CopyFormat = "tsv" | "csv" | "markdown";

const text = (c: Cell) => c ?? "";

function csvField(v: string): string {
  return /[",\n\r]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
}

/** Rows (and optional header) as TSV, CSV or a Markdown table. */
export function formatRows(rows: Cell[][], format: CopyFormat, header?: string[]): string {
  const all: string[][] = [...(header ? [header] : []), ...rows.map((r) => r.map(text))];
  switch (format) {
    case "tsv":
      return all.map((r) => r.map((v) => v.replace(/[\t\n\r]/g, " ")).join("\t")).join("\n");
    case "csv":
      return all.map((r) => r.map(csvField).join(",")).join("\n");
    case "markdown": {
      const esc = (v: string) => v.replace(/\|/g, "\\|").replace(/\n/g, " ");
      const head = header ?? all[0]?.map((_, i) => `col${i + 1}`) ?? [];
      const body = header ? all.slice(1) : all;
      const lines = [
        `| ${head.map(esc).join(" | ")} |`,
        `| ${head.map(() => "---").join(" | ")} |`,
        ...body.map((r) => `| ${r.map(esc).join(" | ")} |`),
      ];
      return lines.join("\n");
    }
  }
}

/** Python-style tuple of the given cells, as "copy values as tuple" does for one page. */
export function pythonTuple(values: Cell[], numeric: boolean): string {
  const repr = (v: Cell) => {
    if (v === null) return "None";
    if (numeric && v.trim() !== "" && !Number.isNaN(Number(v))) return v;
    const quote = v.includes("'") && !v.includes('"') ? '"' : "'";
    const body = v.replace(/\\/g, "\\\\").replace(/\n/g, "\\n").replace(/\t/g, "\\t").replace(new RegExp(quote, "g"), `\\${quote}`);
    return `${quote}${body}${quote}`;
  };
  return `(${values.map(repr).join(", ")})`;
}
