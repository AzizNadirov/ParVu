// The data table: one page of results drawn by Glide Data Grid (canvas,
// virtualised). Pages come pre-formatted from Rust; this component only
// displays them, reports edits, and raises menu events.
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
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

const NUMERIC = /^(TINYINT|SMALLINT|INTEGER|BIGINT|HUGEINT|UTINYINT|USMALLINT|UINTEGER|UBIGINT|UHUGEINT|FLOAT|DOUBLE|REAL|DECIMAL)(\(|$)/i;
export const isNumericType = (dtype: string) => NUMERIC.test(dtype.trim());

export interface GridHandle {
  /** Select and scroll to a cell of the current page. */
  selectCell(row: number, col: number): void;
  /** Currently selected cell (page row, column), if any. */
  currentCell(): { row: number; col: number } | null;
  /** Rows of the current rectangular selection, plus their column names. */
  selection(): { rows: Cell[][]; columns: string[] } | null;
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
}

function estimateWidth(meta: ColumnMeta, rows: Cell[][], col: number, charPx: number): number {
  let longest = meta.name.length + 3;
  const sample = Math.min(rows.length, 200);
  for (let r = 0; r < sample; r++) {
    const v = rows[r][col];
    if (v && v.length > longest) longest = v.length;
  }
  return Math.max(60, Math.min(420, Math.round(longest * charPx + 24)));
}

export const DataGrid = forwardRef<GridHandle, DataGridProps>(function DataGrid(props, ref) {
  const { page, sort, themeKey, nullText } = props;
  const editor = useRef<DataEditorRef>(null);
  const [widths, setWidths] = useState<Record<string, number>>({});
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

  const altRow = cssVar("--pv-derived-table-alternate-row", "");
  const editedBg = "rgba(255, 213, 79, 0.45)";

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

  const getCellContent = useCallback(
    ([col, row]: Item): GridCell => {
      const v = page?.rows[row]?.[col] ?? null;
      const edited = editedSet.has(`${row}:${col}`);
      const isNull = v === null;
      return {
        kind: GridCellKind.Text,
        data: v ?? "",
        displayData: isNull ? nullText : v,
        allowOverlay: true,
        readonly: !page?.editable,
        contentAlign: numeric[col] ? "right" : "left",
        themeOverride: edited
          ? { bgCell: editedBg }
          : isNull
            ? { textDark: cssVar("--pv-colors-editor-comment", "#999") }
            : undefined,
      };
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [page, editedSet, numeric, nullText, editedBg, themeKey],
  );

  const onCellEdited = useCallback(
    ([col, row]: Item, value: EditableGridCell) => {
      if (!page || value.kind !== GridCellKind.Text) return;
      const before = page.rows[row]?.[col] ?? null;
      if ((before ?? "") === value.data && before !== null) return;
      props.onEdit(row, page.columns[col].name, value.data);
    },
    [page, props],
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
      focus() {
        editor.current?.focus();
      },
    }),
    [selection, getSelectionRows, selectCell],
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
        rowHeight={Number(cssVar("--pv-layout-table-row-height", "26px").replace("px", "")) || 26}
        headerHeight={Number(cssVar("--pv-layout-table-header-height", "30px").replace("px", "")) || 30}
        getRowThemeOverride={(row) => (row % 2 === 1 && altRow ? { bgCell: altRow } : undefined)}
        onColumnResize={(col, w) => setWidths((prev) => ({ ...prev, [col.id as string]: w }))}
        onHeaderClicked={(col) => props.onHeaderClick(page.columns[col].name)}
        onHeaderContextMenu={(col, e) => {
          e.preventDefault();
          props.onHeaderMenu(page.columns[col].name, e.bounds.x + e.localEventX, e.bounds.y + e.localEventY);
        }}
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
          props.onCellMenu(row, col, e.bounds.x + e.localEventX, e.bounds.y + e.localEventY);
        }}
      />
    </div>
  );
});
