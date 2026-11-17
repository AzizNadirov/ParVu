# ParVu icon concepts

The ten app icon concepts from the 1.0 icon review, kept for reference. The
review compared each one from 512 px down to 16 px, on light and dark
desktops, in a dock, a taskbar and a file list.

**01 Column Chunks** was chosen. Its sources are in
`crates/parvu-app/icons/source/`: `parvu.svg` (the master), `parvu-24.svg` and
`parvu-16.svg` (hand-tuned small sizes). `build.sh` there regenerates every
icon file.

| File | Concept |
|---|---|
| icon-01-column-chunks.svg | Three columns cut into chunks, one amber (chosen) |
| icon-02-grid-lens.svg | A magnifier over a table |
| icon-03-v-columns.svg | Hanging columns forming a V |
| icon-04-query-rows.svg | A prompt chevron and result rows |
| icon-05-formula-cell.svg | A spreadsheet cell with "=" |
| icon-06-pv-monogram.svg | P and V, one stroke each |
| icon-07-data-sheet.svg | A file holding column chunks (a file-type icon) |
| icon-08-page-stack.svg | Stacked sheets with a table |
| icon-09-parquet-weave.svg | A parquet tile of row and column planks |
| icon-10-table-eye.svg | An eye with a table as the pupil |
