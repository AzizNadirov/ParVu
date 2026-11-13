import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import type { Settings } from "../../lib/api";
import { format } from "../../lib/i18n";
import { RichText, qtLabel } from "../common";
import { maybeTranslate, shortcutRows } from "../ShortcutsDialog";
import { formToValues, invalidNumericFields, settingsToForm } from "../SettingsDialog";
import { pickRightKey } from "../JoinDialog";
import { formatFileSize } from "../TableInfoDialog";

describe("RichText", () => {
  it("renders <b>, <br> and &nbsp; and drops other tags", () => {
    const { container } = render(<RichText text={"Table: <b>t&amp;x</b><br>next&nbsp;<span style='x'>(INT)</span>"} />);
    expect(container.querySelector("b")?.textContent).toBe("t&x");
    expect(container.querySelector("br")).not.toBeNull();
    expect(container.querySelector("span")).toBeNull();
    expect(container.textContent).toBe("Table: t&xnext (INT)");
  });

  it("does not interpret other markup", () => {
    const { container } = render(<RichText text={"<img src=x onerror=alert(1)>"} />);
    expect(container.querySelector("img")).toBeNull();
  });

  it("unescapes Qt's && in labels", () => {
    expect(qtLabel("Save && Close")).toBe("Save & Close");
  });
});

describe("shortcuts", () => {
  const t = (k: string) => `T(${k})`;
  it("translates keys but not key names", () => {
    expect(maybeTranslate(t, "shortcuts.value.sort")).toBe("T(shortcuts.value.sort)");
    expect(maybeTranslate(t, "Ctrl+N")).toBe("Ctrl+N");
    expect(maybeTranslate(t, "Esc")).toBe("Esc");
  });
  it("includes Shift+Wheel horizontal scrolling", () => {
    const rows = shortcutRows(t);
    expect(rows).toContainEqual(["T(shortcuts.group.table)", "T(shortcuts.action.hscroll)", "T(shortcuts.value.hscroll)"]);
    expect(rows).toHaveLength(16);
  });
});

describe("settings form", () => {
  const settings = {
    default_data_var_name: "data",
    default_limit: "500",
    default_sql_font_size: "10",
    default_result_font_size: 11,
    default_sql_query: "SELECT 1",
    default_sql_font: "Courier",
    sql_keywords: ["SELECT"],
    result_pagination_rows_per_page: "500",
    save_file_history: "true",
    max_rows: "1000",
    current_theme: "ParVu Light",
    current_language: "en",
    enable_large_dataset_warning: true,
    warning_criteria: "cells",
    warn_on_exit_with_transforms: true,
    copy_tuple_sample_size: 500,
    warning_threshold_rows: 1_000_000,
    warning_threshold_cells: 10_000_000,
    warning_threshold_filesize_mb: 100,
    bug_report_email: "x@y.z",
    enable_crash_reporting: false,
  } as Settings;

  it("round-trips settings through the form", () => {
    const f = settingsToForm(settings, "en", "ParVu Light");
    expect(f.saveHistory).toBe(true);
    expect(f.tableFontSize).toBe("11");
    expect(invalidNumericFields(f)).toEqual([]);
    const v = formToValues({ ...f, rowsPerPage: "250", saveHistory: false, tableVar: " tbl " });
    expect(v.result_pagination_rows_per_page).toBe("250");
    expect(v.save_file_history).toBe("False");
    expect(v.default_data_var_name).toBe("tbl");
    expect(v.warning_criteria).toBe("cells");
    expect(v.copy_tuple_sample_size).toBe(500);
    expect(v).not.toHaveProperty("current_theme");
  });

  it("flags out-of-range numbers", () => {
    const f = settingsToForm(settings, "en", "x");
    expect(invalidNumericFields({ ...f, rowsPerPage: "5", sqlFontSize: "abc" })).toEqual(["rowsPerPage", "sqlFontSize"]);
  });
});

describe("misc", () => {
  it("picks a matching right key", () => {
    expect(pickRightKey("id", ["x", "id"])).toBe("id");
    expect(pickRightKey("id", ["x", "y"])).toBe("x");
    expect(pickRightKey("id", [])).toBe("");
  });
  it("formats file sizes", () => {
    expect(formatFileSize(0)).toBe("0 B");
    expect(formatFileSize(1536)).toBe("1.5 KB");
    expect(formatFileSize(5 * 1024 * 1024)).toBe("5.0 MB");
  });
  it("locale placeholders format", () => {
    expect(format("{count} of {total} selected", { count: 2, total: 5 })).toBe("2 of 5 selected");
  });
});
