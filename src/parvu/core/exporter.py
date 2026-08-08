"""
Chunked dataset export.

Streams the current query result out of DuckDB in Arrow record batches and
writes them with pyarrow, so exporting a huge file never materializes it in
memory and can report row-level progress.

Per-column type casting and temporal formatting are pushed into SQL; the
remaining knobs (delimiter, compression, JSON layout) go to the writer.
"""
from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from collections.abc import Callable

import duckdb
import pyarrow as pa
import pyarrow.csv as pacsv
import pyarrow.parquet as pq
from loguru import logger

CHUNK_ROWS = 50_000

#: Suffix -> export format.
FORMAT_BY_SUFFIX = {
    ".csv": "csv",
    ".tsv": "csv",
    ".parquet": "parquet",
    ".pq": "parquet",
    ".json": "json",
    ".jsonl": "json",
    ".ndjson": "json",
}

#: Default suffix per format (first choice in the dialog).
SUFFIX_BY_FORMAT = {"csv": ".csv", "parquet": ".parquet", "json": ".json"}

#: Cast targets offered per column ("" = keep source type).
CAST_TYPES = [
    "",
    "VARCHAR",
    "BIGINT",
    "INTEGER",
    "DOUBLE",
    "DECIMAL(18,4)",
    "BOOLEAN",
    "DATE",
    "TIMESTAMP",
]


class ExportCancelled(Exception):
    """Raised when the caller cancels an in-flight export."""


@dataclass(slots=True)
class ColumnSpec:
    """Export settings for a single column."""

    name: str
    source_type: str
    include: bool = True
    cast_to: str = ""  # DuckDB type name; "" keeps the source type


@dataclass(slots=True)
class ExportOptions:
    """Everything the export needs beyond the source query."""

    path: Path
    format: str  # "csv" | "parquet" | "json"
    columns: list[ColumnSpec] = field(default_factory=list)
    date_format: str = ""  # strftime pattern for DATE columns
    timestamp_format: str = ""  # strftime pattern for TIMESTAMP/TIME columns
    # CSV
    delimiter: str = ","
    include_header: bool = True
    quoting: str = "needed"  # needed | all_valid | none
    # Parquet
    compression: str = "snappy"
    # JSON
    json_lines: bool = False


def format_for_path(path: Path) -> str:
    """Return the export format for a path's suffix (defaults to csv)."""
    return FORMAT_BY_SUFFIX.get(path.suffix.lower(), "csv")


def is_temporal(duckdb_type: str) -> bool:
    """True for DuckDB DATE/TIME/TIMESTAMP column types."""
    t = duckdb_type.upper()
    return t.startswith(("DATE", "TIMESTAMP", "TIME"))


def _quote_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def _quote_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def _temporal_format(spec: ColumnSpec, options: ExportOptions) -> str:
    """Pick the strftime pattern that applies to this column, if any."""
    if not is_temporal(spec.source_type):
        return ""
    if spec.source_type.upper().startswith("DATE"):
        return options.date_format
    return options.timestamp_format


def build_export_query(source_query: str, options: ExportOptions) -> str:
    """Wrap ``source_query`` with column selection, casts and date formatting.

    An explicit cast wins over a temporal format string, except when casting
    to VARCHAR — that is what formatting produces anyway.
    """
    included = [c for c in options.columns if c.include]
    if not included:
        raise ValueError("Select at least one column to export")

    parts = []
    for spec in included:
        expr = _quote_ident(spec.name)
        fmt = _temporal_format(spec, options)
        if fmt and spec.cast_to in ("", "VARCHAR"):
            expr = f"strftime({expr}, {_quote_literal(fmt)})"
        elif spec.cast_to:
            expr = f"CAST({expr} AS {spec.cast_to})"
        parts.append(f"{expr} AS {_quote_ident(spec.name)}")

    return f"SELECT {', '.join(parts)} FROM ({source_query})"


class _JsonWriter:
    """Minimal record-batch writer for JSON array / JSON lines output."""

    def __init__(self, path: Path, lines: bool):
        self._lines = lines
        self._fh = path.open("w", encoding="utf-8")
        self._first = True
        if not lines:
            self._fh.write("[\n")

    def write_batch(self, batch: pa.RecordBatch) -> None:
        for row in batch.to_pylist():
            text = json.dumps(row, default=str, ensure_ascii=False)
            if self._lines:
                self._fh.write(text + "\n")
            else:
                self._fh.write(("" if self._first else ",\n") + "  " + text)
                self._first = False

    def close(self) -> None:
        if not self._lines:
            self._fh.write("\n]\n" if not self._first else "]\n")
        self._fh.close()


def _make_writer(options: ExportOptions, schema: pa.Schema):
    if options.format == "parquet":
        return pq.ParquetWriter(options.path, schema, compression=options.compression)
    if options.format == "csv":
        # pyarrow quotes every header name under "needed"; only ask for bare
        # headers when no column name could break the format.
        specials = {options.delimiter, '"', "\n", "\r"}
        bare_header = all(not specials & set(name) for name in schema.names)
        return pacsv.CSVWriter(
            options.path,
            schema,
            write_options=pacsv.WriteOptions(
                include_header=options.include_header,
                delimiter=options.delimiter,
                quoting_style=options.quoting,
                quoting_header="none" if bare_header else "needed",
            ),
        )
    if options.format == "json":
        return _JsonWriter(options.path, options.json_lines)
    raise ValueError(f"Unsupported export format: {options.format}")


def export_dataset(
    conn: duckdb.DuckDBPyConnection,
    source_query: str,
    options: ExportOptions,
    on_progress: Callable[[int], None] | None = None,
    is_cancelled: Callable[[], bool] | None = None,
) -> int:
    """Stream ``source_query`` to ``options.path``. Returns rows written.

    ``conn`` must be usable from the calling thread — pass a dedicated
    ``connection.cursor()`` when exporting off the UI thread.
    """
    sql = build_export_query(source_query, options)
    logger.info(f"Export -> {options.path} ({options.format}): {sql}")

    reader = conn.execute(sql).fetch_record_batch(CHUNK_ROWS)
    writer = _make_writer(options, reader.schema)
    rows = 0
    try:
        for batch in reader:
            if is_cancelled is not None and is_cancelled():
                raise ExportCancelled()
            writer.write_batch(batch)
            rows += batch.num_rows
            if on_progress is not None:
                on_progress(rows)
    except BaseException:
        writer.close()
        options.path.unlink(missing_ok=True)
        raise
    writer.close()
    logger.info(f"Export complete: {rows} rows -> {options.path}")
    return rows
