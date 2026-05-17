"""
Query service for ParVu.

High-level operations for executing queries, pagination, and export.
"""
from __future__ import annotations

from pathlib import Path

import pandas as pd
from loguru import logger

from parvu.core.interfaces import IQueryEngine
from parvu.core.models import QueryResult, ExportResult, Page
from parvu.core.exceptions import QueryError, ExportError


class QueryService:
    """Orchestrates query operations."""

    def __init__(self, query_engine: IQueryEngine | None = None):
        self._engine = query_engine

    @property
    def engine(self) -> IQueryEngine | None:
        return self._engine

    def attach_engine(self, engine: IQueryEngine) -> None:
        """Attach a query engine."""
        self._engine = engine

    def detach_engine(self) -> None:
        """Detach and close current engine."""
        if self._engine:
            self._engine.close()
            self._engine = None

    def execute(self, query: str) -> QueryResult:
        """Execute a SQL query."""
        if not self._engine:
            return QueryResult(success=False, message="No engine attached")
        success, message = self._engine.execute_query(query)
        return QueryResult(success=success, message=message)

    def get_page(self, page_num: int) -> Page:
        """Get a page of results."""
        if not self._engine:
            return Page(page_num=1, total_pages=1, page_size=0, total_rows=0, data=pd.DataFrame())

        data = self._engine.get_page(page_num)
        return Page(
            page_num=page_num,
            total_pages=self._engine.total_pages,
            page_size=self._engine.page_size,
            total_rows=self._engine.total_rows,
            data=data,
        )

    def sort(self, column: str, ascending: bool = True) -> QueryResult:
        """Sort by column."""
        if not self._engine:
            return QueryResult(success=False, message="No engine attached")
        success, message = self._engine.sort_by_column(column, ascending)
        return QueryResult(success=success, message=message)

    def get_unique_values(self, column: str) -> list:
        """Get unique values for a column."""
        if not self._engine:
            return []
        return self._engine.get_unique_values(column)

    def export(self, output_path: Path) -> ExportResult:
        """Export current results."""
        if not self._engine:
            return ExportResult(success=False, message="No engine attached")
        success = self._engine.export_results(output_path)
        return ExportResult(
            success=success,
            output_path=output_path,
            message="Export complete" if success else "Export failed",
        )

    def reset(self) -> None:
        """Reset to original file query."""
        if self._engine:
            self._engine.reset_query()

    def get_columns(self) -> list[str]:
        """Get column names."""
        if not self._engine:
            return []
        return self._engine.get_columns()

    def get_column_types(self) -> list[tuple[str, str]]:
        """Get column names and types."""
        if not self._engine:
            return []
        return self._engine.get_column_types()

    def get_table_info(self) -> dict:
        """Get table metadata."""
        if not self._engine:
            return {}
        return self._engine.get_table_info()
