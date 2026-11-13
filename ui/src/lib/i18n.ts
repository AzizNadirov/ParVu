// Translation lookup and a Python str.format-compatible formatter for the
// placeholder forms the locale files use: {name}, {name:,}, {name:.1f}.

export type TArgs = Record<string, string | number | boolean | null | undefined>;
export type Translate = (key: string, args?: TArgs) => string;

function formatValue(value: TArgs[string], spec: string): string {
  if (value === null || value === undefined) return "None";
  if (!spec) return String(value);
  const n = typeof value === "number" ? value : Number(value);
  if (Number.isNaN(n)) return String(value);
  const m = /^(,)?(?:\.(\d+))?([fd%])?$/.exec(spec);
  if (!m) return String(value);
  const [, comma, precision, type] = m;
  let num = type === "%" ? n * 100 : n;
  const digits = precision !== undefined ? Number(precision) : type === "f" ? 6 : undefined;
  let text: string;
  if (comma) {
    text = num.toLocaleString("en-US", {
      minimumFractionDigits: digits ?? 0,
      maximumFractionDigits: digits ?? (Number.isInteger(num) ? 0 : 20),
    });
  } else if (digits !== undefined) {
    text = num.toFixed(digits);
  } else {
    text = String(num);
  }
  return type === "%" ? `${text}%` : text;
}

/** Substitute `{name}` / `{name:spec}` placeholders; `{{` and `}}` are literal braces. */
export function format(template: string, args?: TArgs): string {
  if (!args) return template;
  return template.replace(/\{\{|\}\}|\{([A-Za-z_][A-Za-z0-9_]*)(?::([^}]*))?\}/g, (whole, name, spec) => {
    if (whole === "{{") return "{";
    if (whole === "}}") return "}";
    if (!(name in args)) return whole;
    return formatValue(args[name], spec ?? "");
  });
}

/** Translator over the current language, falling back to English, then the key. */
export function makeTranslator(current: Record<string, string>, english: Record<string, string>): Translate {
  return (key, args) => format(current[key] ?? english[key] ?? key, args);
}
