// The data table: one page of results drawn by Glide Data Grid (canvas,
// virtualised). Pages come pre-formatted from Rust; this component only
// displays them, reports edits, and raises menu events.
import { forwardRef, memo, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import {
  CompactSelection,
  DataEditor,
  GridCellKind,
  type DataEditorRef,
  type EditableGridCell,
  type GridCell,
  type GridColumn,
  type GridSelection,
  type Item,
  type Theme,
} from "@glideapps/glide-data-grid";
import "@glideapps/glide-data-grid/dist/index.css";
import type { Cell, ColumnMeta, Page } from "../lib/api";
import { cssVar } from "../lib/theme";
import { cellBackground, type FormatColor, type FormatRule } from "../lib/format";

const NUMERIC = /^(TINYINT|SMALLINT|INTEGER|BIGINT|HUGEINT|UTINYINT|USMALLINT|UINTEGER|UBIGINT|UHUGEINT|FLOAT|DOUBLE|REAL|DECIMAL)(\(|$)/i;
export const isNumericType = (dtype: string) => NUMERIC.test(dtype.trim());

export interface GridHandle {
  /** Select and scroll to a cell of the current page. */
  selectCell(row: number, col: number): void;
  /** Currently selected cell (page row, column), if any. */
  currentCell(): { row: number; col: number } | null;
  /** Rows of the current rectangular selection, plus their column names. */
  selection(): { rows: Cell[][]; columns: string[] } | null;
  /** The selected rectangle in page coordinates. */
  selectionRange(): { x: number; y: number; width: number; height: number } | null;
  focus(): void;
}

export interface DataGridProps {
  page: Page | null;
  sort: { column: string; ascending: boolean } | null;
  /** Bumped when the theme changes so colours are re-read. */
  themeKey: string;
  nullText: string;
  /** Select and scroll to this cell once the page has rendered. */
  select?: { row: number; col: number; seq: number } | null;
  onHeaderClick(column: string): void;
  onHeaderMenu(column: string, x: number, y: number): void;
  onCellMenu(row: number, col: number, x: number, y: number): void;
  onEdit(row: number, column: string, value: string): void;
  /** Number of columns kept in view on the left. */
  frozen: number;
  /** Column widths set by the user, by column name. */
  widths: Record<string, number>;
  onResize(column: string, width: number): void;
  /** Conditional formatting rules. */
  formats: FormatRule[];
  /** Minimum and maximum of the columns with a colour scale. */
  ranges: Record<string, { min: number; max: number }>;
}

const NO_RULES: FormatRule[] = [];

function estimateWidth(meta: ColumnMeta, rows: Cell[][], col: number, charPx: number): number {
  let longest = meta.name.length + 3;
  const sample = Math.min(rows.length, 200);
  for (let r = 0; r < sample; r++) {
    const v = rows[r][col];
    if (v && v.length > longest) longest = v.length;
  }
  return Math.max(60, Math.min(420, Math.round(longest * charPx + 24)));
}

/** Highlight for cells with a pending edit; readable on light and dark themes. */
const EDITED_OVERRIDE: Partial<Theme> = { bgCell: "rgba(255, 213, 79, 0.45)" };

const DataGridInner = forwardRef<GridHandle, DataGridProps>(function DataGrid(props, ref) {
  const { page, sort, themeKey, nullText, widths, formats, ranges } = props;
  // Event handlers from the parent change identity on every render. Keep the
  // latest ones in a ref so the callbacks passed to the grid stay stable and
  // the canvas is not redrawn for unrelated parent updates.
  const handlers = useRef(props);
  handlers.current = props;
  const editor = useRef<DataEditorRef>(null);
  const [selection, setSelection] = useState<GridSelection>({
    columns: CompactSelection.empty(),
    rows: CompactSelection.empty(),
  });

  const theme = useMemo<Partial<Theme>>(() => {
    const font = cssVar("--pv-layout-table-font-family", "sans-serif").replace(/"/g, "");
    const size = cssVar("--pv-layout-table-font-size", "10pt");
    return {
      accentColor: cssVar("--pv-colors-accent-primary", "#4F5DFF"),
      accentLight: cssVar("--pv-colors-table-selection", "rgba(62,116,253,0.15)"),
      textDark: cssVar("--pv-colors-table-foreground", "#313139"),
      textMedium: cssVar("--pv-colors-table-foreground", "#737383"),
      textLight: cssVar("--pv-colors-status-foreground", "#999"),
      textHeader: cssVar("--pv-colors-table-header-foreground", "#313139"),
      textHeaderSelected: "#ffffff",
      bgCell: cssVar("--pv-colors-table-background", "#ffffff"),
      bgCellMedium: cssVar("--pv-derived-table-alternate-row", "#fafafb"),
      bgHeader: cssVar("--pv-colors-table-header-background", "#f7f7f8"),
      bgHeaderHasFocus: cssVar("--pv-colors-button-hover", "#e9e9eb"),
      bgHeaderHovered: cssVar("--pv-colors-button-hover", "#efeff1"),
      borderColor: cssVar("--pv-derived-table-grid", "rgba(115,116,131,0.16)"),
      horizontalBorderColor: cssVar("--pv-derived-table-grid", "rgba(115,116,131,0.16)"),
      fontFamily: `${font}, system-ui, sans-serif`,
      baseFontStyle: size,
      headerFontStyle: `600 ${size}`,
      markerFontStyle: size,
      editorFontSize: size,
    };
    // themeKey changes when the CSS variables change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [themeKey]);

  // Theme values read once per theme. Reading CSS variables forces a style
  // recalculation, so it must never happen per cell while drawing.
  const styling = useMemo(() => {
    const px = (name: string, fallback: number) =>
      Number(cssVar(name, `${fallback}px`).replace("px", "")) || fallback;
    const alt = cssVar("--pv-derived-table-alternate-row", "");
    return {
      altOverride: alt ? ({ bgCell: alt } as Partial<Theme>) : undefined,
      nullOverride: { textDark: cssVar("--pv-colors-editor-comment", "#999") } as Partial<Theme>,
      rowHeight: px("--pv-layout-table-row-height", 26),
      headerHeight: px("--pv-layout-table-header-height", 30),
      colors: {
        primary: cssVar("--pv-colors-accent-primary", "#2196F3"),
        secondary: cssVar("--pv-colors-accent-secondary", "#4CAF50"),
        warning: cssVar("--pv-colors-accent-warning", "#FF9800"),
        error: cssVar("--pv-colors-accent-error", "#F44336"),
      } as Record<FormatColor, string>,
    };
    // themeKey changes when the CSS variables change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [themeKey]);

  const editedSet = useMemo(() => new Set((page?.edited ?? []).map(([r, c]) => `${r}:${c}`)), [page]);

  const columns = useMemo<GridColumn[]>(() => {
    if (!page) return [];
    const charPx = 7.2;
    return page.columns.map((c, i) => {
      const arrow = sort && sort.column === c.name ? (sort.ascending ? " ▲" : " ▼") : "";
      return {
        id: c.name,
        title: c.name + arrow,
        width: widths[c.name] ?? estimateWidth(c, page.rows, i, charPx),
      };
    });
  }, [page, sort, widths]);

  const numeric = useMemo(() => (page?.columns ?? []).map((c) => isNumericType(c.dtype)), [page]);

  // Formatting rules per column index, so drawing a cell looks at its own rules only.
  const rulesByCol = useMemo(
    () => (page?.columns ?? []).map((c) => formats.filter((f) => f.column === c.name)),
    [page, formats],
  );

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const v = page?.rows[row]?.[col] ?? null;
      const edited = editedSet.has(`${row}:${col}`);
      const isNull = v === null;
      const rules = rulesByCol[col] ?? NO_RULES;
      const bg = !edited && rules.length ? cellBackground(rules, v, ranges, styling.colors) : null;
      const override = edited
        ? EDITED_OVERRIDE
        : bg
          ? { bgCell: bg, ...(isNull ? styling.nullOverride : {}) }
          : isNull
            ? styling.nullOverride
            : undefined;
      return {
        kind: GridCellKind.Text,
        data: v ?? "",
        displayData: isNull ? nullText : v,
        allowOverlay: true,
        readonly: !page?.editable,
        contentAlign: numeric[col] ? "right" : "left",
        themeOverride: override,
      };
    },
    [page, editedSet, numeric, nullText, styling, rulesByCol, ranges],
  );

  const getRowThemeOverride = useCallback(
    (row: number) => (row % 2 === 1 ? styling.altOverride : undefined),
    [styling],
  );

  const onCellEdited = useCallback(
    ([col, row]: Item, value: EditableGridCell) => {
      if (!page || value.kind !== GridCellKind.Text) return;
      const before = page.rows[row]?.[col] ?? null;
      if ((before ?? "") === value.data && before !== null) return;
      handlers.current.onEdit(row, page.columns[col].name, value.data);
    },
    [page],
  );

  const getSelectionRows = useCallback(() => {
    const range = selection.current?.range;
    if (!page || !range) return null;
    const rows: Cell[][] = [];
    for (let r = range.y; r < range.y + range.height; r++) {
      rows.push(page.rows[r]?.slice(range.x, range.x + range.width) ?? []);
    }
    const cols = page.columns.slice(range.x, range.x + range.width).map((c) => c.name);
    return { rows, columns: cols };
  }, [page, selection]);

  const selectCell = useCallback((row: number, col: number) => {
    setSelection({
      columns: CompactSelection.empty(),
      rows: CompactSelection.empty(),
      current: { cell: [col, row], range: { x: col, y: row, width: 1, height: 1 }, rangeStack: [] },
    });
    editor.current?.scrollTo(col, row, "both", 0, 0, { vAlign: "center", hAlign: "center" });
  }, []);

  // Apply a requested selection after the grid has rendered the new page.
  useEffect(() => {
    const sel = props.select;
    if (!sel || !page || sel.row >= page.rows.length) return;
    const timer = window.setTimeout(() => selectCell(sel.row, sel.col), 30);
    return () => window.clearTimeout(timer);
    // Only when a new request arrives.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.select?.seq]);

  useImperativeHandle(
    ref,
    () => ({
      selectCell,
      currentCell() {
        const c = selection.current?.cell;
        return c ? { row: c[1], col: c[0] } : null;
      },
      selection: getSelectionRows,
      selectionRange() {
        return selection.current?.range ?? null;
      },
      focus() {
        editor.current?.focus();
      },
    }),
    [selection, getSelectionRows, selectCell],
  );

  const onHeaderClicked = useCallback(
    (col: number) => {
      const name = page?.columns[col]?.name;
      if (name) handlers.current.onHeaderClick(name);
    },
    [page],
  );

  const onHeaderContextMenu = useCallback(
    (col: number, e: { preventDefault(): void; bounds: { x: number; y: number }; localEventX: number; localEventY: number }) => {
      e.preventDefault();
      const name = page?.columns[col]?.name;
      if (name) handlers.current.onHeaderMenu(name, e.bounds.x + e.localEventX, e.bounds.y + e.localEventY);
    },
    [page],
  );

  if (!page) return <div className="grid-empty" />;

  return (
    <div className="grid-host">
      <DataEditor
        key={`${page.page}:${page.total_rows}:${page.columns.length}`}
        ref={editor}
        columns={columns}
        rows={page.rows.length}
        getCellContent={getCellContent}
        onCellEdited={onCellEdited}
        getCellsForSelection={true}
        gridSelection={selection}
        onGridSelectionChange={setSelection}
        rowMarkers="number"
        rowMarkerStartIndex={page.offset + 1}
        rowMarkerWidth={Math.max(44, String(page.offset + page.rows.length).length * 8 + 18)}
        smoothScrollX
        smoothScrollY
        width="100%"
        height="100%"
        theme={theme}
        rowHeight={styling.rowHeight}
        headerHeight={styling.headerHeight}
        getRowThemeOverride={getRowThemeOverride}
        onColumnResize={(col, w) => handlers.current.onResize(col.id as string, w)}
        freezeColumns={Math.min(props.frozen, page.columns.length)}
        onHeaderClicked={onHeaderClicked}
        onHeaderContextMenu={onHeaderContextMenu}
        onCellContextMenu={([col, row], e) => {
          e.preventDefault();
          if (col < 0) return;
          const inSelection = selection.current?.range;
          const inside =
            inSelection &&
            col >= inSelection.x &&
            col < inSelection.x + inSelection.width &&
            row >= inSelection.y &&
            row < inSelection.y + inSelection.height;
          if (!inside) {
            setSelection({
              columns: CompactSelection.empty(),
              rows: CompactSelection.empty(),
              current: { cell: [col, row], range: { x: col, y: row, width: 1, height: 1 }, rangeStack: [] },
            });
          }
          handlers.current.onCellMenu(row, col, e.bounds.x + e.localEventX, e.bounds.y + e.localEventY);
        }}
      />
    </div>
  );
});

/** Re-render only when the data, sort, theme or a selection request changes. */
export const DataGrid = memo(
  DataGridInner,
  (a, b) =>
    a.page === b.page &&
    a.sort?.column === b.sort?.column &&
    a.sort?.ascending === b.sort?.ascending &&
    a.themeKey === b.themeKey &&
    a.nullText === b.nullText &&
    a.select?.seq === b.select?.seq &&
    a.frozen === b.frozen &&
    a.widths === b.widths &&
    a.formats === b.formats &&
    a.ranges === b.ranges,
);
