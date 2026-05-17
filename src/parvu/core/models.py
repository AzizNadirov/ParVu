"""
Domain data models for ParVu.

Pure data classes with no external dependencies.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import pandas as pd


@dataclass(frozen=True, slots=True)
class ColumnInfo:
    """Metadata about a single column."""
    name: str
    dtype: str
    nullable: bool = True


@dataclass(frozen=True, slots=True)
class FileInfo:
    """Metadata about a loaded file."""
    path: Path
    format: str
    total_rows: int
    total_columns: int
    columns: list[ColumnInfo] = field(default_factory=list)
    file_size_bytes: int = 0

    @property
    def file_size_mb(self) -> float:
        return self.file_size_bytes / (1024 * 1024)

    @property
    def total_cells(self) -> int:
        return self.total_rows * self.total_columns


@dataclass(frozen=True, slots=True)
class Page:
    """A single page of query results."""
    page_num: int
    total_pages: int
    page_size: int
    total_rows: int
    data: pd.DataFrame

    @property
    def is_first(self) -> bool:
        return self.page_num <= 1

    @property
    def is_last(self) -> bool:
        return self.page_num >= self.total_pages

    @property
    def row_count(self) -> int:
        return len(self.data)


@dataclass(frozen=True, slots=True)
class QueryResult:
    """Result of executing a query."""
    success: bool
    message: str = ""
    rows_affected: int = 0


@dataclass(frozen=True, slots=True)
class ExportResult:
    """Result of exporting data."""
    success: bool
    output_path: Path | None = None
    message: str = ""
    rows_exported: int = 0


@dataclass(frozen=True, slots=True)
class SortSpec:
    """Specification for sorting."""
    column: str
    ascending: bool = True


@dataclass(frozen=True, slots=True)
class FilterSpec:
    """Specification for filtering by column values."""
    column: str
    values: list[Any]
    operator: str = "IN"  # IN, EQ, etc.
