"""
Unit tests for chunked export: SQL shaping (casts, date formats) and writers.
"""
import json

import duckdb
import pytest

from parvu.core.exporter import (
    ColumnSpec,
    ExportCancelled,
    ExportOptions,
    build_export_query,
    export_dataset,
    format_for_path,
)

SOURCE = (
    "SELECT * FROM (VALUES "
    "(1, DATE '2020-03-04', TIMESTAMP '2020-03-04 05:06:07', 'a'), "
    "(2, DATE '2021-12-31', TIMESTAMP '2021-12-31 23:59:00', 'b')"
    ") AS t(id, d, ts, name)"
)


@pytest.fixture
def conn():
    c = duckdb.connect(":memory:")
    yield c
    c.close()


def specs(**overrides) -> list[ColumnSpec]:
    base = [
        ColumnSpec("id", "INTEGER"),
        ColumnSpec("d", "DATE"),
        ColumnSpec("ts", "TIMESTAMP"),
        ColumnSpec("name", "VARCHAR"),
    ]
    for spec in base:
        for key, value in overrides.get(spec.name, {}).items():
            setattr(spec, key, value)
    return base


def options(tmp_path, fmt, **kwargs) -> ExportOptions:
    kwargs.setdefault("columns", specs())
    return ExportOptions(path=tmp_path / f"out.{fmt}", format=fmt, **kwargs)


def test_format_for_path():
    from pathlib import Path

    assert format_for_path(Path("a.parquet")) == "parquet"
    assert format_for_path(Path("a.jsonl")) == "json"
    assert format_for_path(Path("a.tsv")) == "csv"


def test_query_applies_cast_and_skips_excluded(tmp_path):
    opts = options(
        tmp_path, "csv", columns=specs(id={"cast_to": "VARCHAR"}, name={"include": False})
    )
    sql = build_export_query(SOURCE, opts)
    assert 'CAST("id" AS VARCHAR) AS "id"' in sql
    assert '"name"' not in sql


def test_query_formats_temporal_columns(tmp_path):
    opts = options(tmp_path, "csv", date_format="%d/%m/%Y", timestamp_format="%Y%m%d %H:%M")
    sql = build_export_query(SOURCE, opts)
    assert "strftime(\"d\", '%d/%m/%Y')" in sql
    assert "strftime(\"ts\", '%Y%m%d %H:%M')" in sql
    # An explicit non-text cast wins over formatting.
    opts.columns[1].cast_to = "TIMESTAMP"
    assert 'CAST("d" AS TIMESTAMP)' in build_export_query(SOURCE, opts)


def test_no_columns_selected_raises(tmp_path):
    opts = options(tmp_path, "csv", columns=specs(
        id={"include": False}, d={"include": False},
        ts={"include": False}, name={"include": False},
    ))
    with pytest.raises(ValueError):
        build_export_query(SOURCE, opts)


def test_csv_export_honours_delimiter_and_date_format(conn, tmp_path):
    opts = options(tmp_path, "csv", delimiter=";", date_format="%d/%m/%Y")
    rows = export_dataset(conn, SOURCE, opts)
    assert rows == 2
    lines = opts.path.read_text().splitlines()
    assert lines[0] == "id;d;ts;name"
    assert lines[1].startswith('1;"04/03/2020";')  # pyarrow quotes text values


def test_parquet_export_roundtrip(conn, tmp_path):
    opts = options(tmp_path, "parquet", compression="zstd")
    assert export_dataset(conn, SOURCE, opts) == 2
    back = conn.execute(f"SELECT id, name FROM read_parquet('{opts.path}')").fetchall()
    assert back == [(1, "a"), (2, "b")]


def test_json_array_and_lines(conn, tmp_path):
    opts = options(tmp_path, "json")
    export_dataset(conn, SOURCE, opts)
    assert [r["name"] for r in json.loads(opts.path.read_text())] == ["a", "b"]

    opts.json_lines = True
    export_dataset(conn, SOURCE, opts)
    records = [json.loads(line) for line in opts.path.read_text().splitlines()]
    assert [r["id"] for r in records] == [1, 2]


def test_progress_and_cancel(conn, tmp_path):
    seen: list[int] = []
    opts = options(tmp_path, "csv")
    export_dataset(conn, SOURCE, opts, on_progress=seen.append)
    assert seen == [2]

    with pytest.raises(ExportCancelled):
        export_dataset(conn, SOURCE, opts, is_cancelled=lambda: True)
    assert not opts.path.exists()  # partial output is removed
