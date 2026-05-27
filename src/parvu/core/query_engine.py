"""
DuckDB Query Engine with Pagination Support.

Uses lazy loading to handle huge files (8GB+) efficiently:
- Files are NOT loaded into memory as tables
- Queries directly access files using read_parquet/read_csv/read_json
- DuckDB's lazy evaluation only processes data when needed
- Only the current page is materialized in memory
- Memory usage remains constant regardless of file size
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any

import duckdb
import pandas as pd
from loguru import logger

from parvu.core.interfaces import IQueryEngine
from parvu.core.pagination import Paginator
from parvu.core.file_adapters import FileAdapterRegistry, default_registry
from parvu.core.exceptions import QueryError, BadQueryError


class QueryEngine(IQueryEngine):
    """
    Manages DuckDB queries with efficient pagination for large datasets.
    """

    def __init__(
        self,
        file_path: Path,
        page_size: int = 100,
        table_name: str = "data",
        adapter_registry: FileAdapterRegistry | None = None,
        conn: duckdb.DuckDBPyConnection | None = None,
    ):
        """
        Initialize query engine.

        Args:
            file_path: Path to parquet/csv/json file.
            page_size: Number of rows per page.
            table_name: Virtual table name used in SQL queries.
            adapter_registry: Registry of file format adapters.
            conn: Optional shared DuckDB connection. If provided, the engine
                  uses it instead of creating its own in-memory connection.
        """
        self._file_path = Path(file_path)
        self._page_size = page_size
        self._table_name = table_name
        self._adapter_registry = adapter_registry or default_registry

        # DuckDB connection
        if conn is not None:
            self._conn = conn
            self._owns_connection = False
            # In shared mode the caller creates a view; just reference it
            self._file_reader_query = f"SELECT * FROM {table_name}"
        else:
            self._conn = duckdb.connect(":memory:")
            self._owns_connection = True
            # Build base query that reads file directly (lazy evaluation)
            self._file_reader_query = self._build_file_reader_query()

        # Query state
        self._current_query = self._file_reader_query
        self._paginator = self._create_paginator()
        self._history: list[str] = []

        logger.info(
            f"QueryEngine initialized: {file_path}, "
            f"{self._paginator.total_rows} rows, "
            f"{self._paginator.total_pages} pages"
        )

    def _build_file_reader_query(self) -> str:
        """Build query that reads file directly without materializing it."""
        adapter = self._adapter_registry.get_adapter(self._file_path)
        query = adapter.build_reader_query(self._file_path)
        logger.debug(f"Built lazy file reader query for {self._file_path}")
        return query

    def _create_paginator(self) -> Paginator:
        """Create paginator based on current query row count."""
        total_rows = self._count_rows()
        return Paginator(total_rows, self._page_size)

    def _count_rows(self, query: str | None = None) -> int:
        """Count total rows in current or given query result."""
        if query is None:
            query = self._current_query
        count_query = f"SELECT COUNT(*) as cnt FROM ({query})"
        result = self._conn.execute(count_query).fetchone()
        return result[0] if result else 0

    def _substitute_table_name(self, query: str) -> str:
        """
        Replace table variable name with actual file reader query.
        Skips matches inside single-quoted string literals to avoid
        corrupting file paths that happen to contain the table name.
        """
        pattern = re.compile(
            rf"\b{re.escape(self._table_name)}\b", re.IGNORECASE
        )
        result: list[str] = []
        i = 0
        while i < len(query):
            q = query.find("'", i)
            if q == -1:
                result.append(pattern.sub(f"({self._file_reader_query})", query[i:]))
                break
            # Substitute only in the non-string part
            result.append(pattern.sub(f"({self._file_reader_query})", query[i:q]))
            # Find closing quote
            q2 = query.find("'", q + 1)
            if q2 == -1:
                result.append(query[q:])
                break
            result.append(query[q:q2 + 1])
            i = q2 + 1
        return "".join(result)

    def _wrap_query(self, query: str) -> str:
        """Wrap query in a subquery to avoid LIMIT conflicts."""
        return f"SELECT * FROM ({query})"

    # ------------------------------------------------------------------
    # Properties
    # ------------------------------------------------------------------

    @property
    def total_rows(self) -> int:
        return self._paginator.total_rows

    @property
    def total_pages(self) -> int:
        return self._paginator.total_pages

    @property
    def page_size(self) -> int:
        return self._page_size

    @property
    def file_path(self) -> Path:
        return self._file_path

    @property
    def current_query(self) -> str:
        return self._current_query

    @property
    def is_base_query(self) -> bool:
        """Return True if viewing the original file without custom queries."""
        return self._current_query == self._file_reader_query

    @property
    def can_undo(self) -> bool:
        """Return True if there is at least one step to undo."""
        return bool(self._history)

    # ------------------------------------------------------------------
    # IQueryEngine implementation
    # ------------------------------------------------------------------

    def get_columns(self) -> list[str]:
        """Get column names from current query result."""
        try:
            wrapped = self._wrap_query(self._current_query)
            result = self._conn.execute(f"{wrapped} LIMIT 0")
            return [desc[0] for desc in result.description]
        except Exception as e:
            logger.error(f"Error getting columns: {e}")
            return []

    def get_column_types(self) -> list[tuple[str, str]]:
        """Get column names and types."""
        try:
            wrapped = self._wrap_query(self._current_query)
            result = self._conn.execute(f"{wrapped} LIMIT 0")
            return [(desc[0], str(desc[1])) for desc in result.description]
        except Exception as e:
            logger.error(f"Error getting column types: {e}")
            return []

    def execute_query(self, query: str) -> tuple[bool, str]:
        """
        Execute a SQL query and update pagination state.

        Returns:
            Tuple of (success: bool, error_message: str or empty)
        """
        try:
            # For shared connections the view already exists in DuckDB,
            # so substitution (which breaks table-qualified columns) is
            # unnecessary and harmful.
            if self._owns_connection:
                substituted = self._substitute_table_name(query)
            else:
                substituted = query
            wrapped = self._wrap_query(substituted)
            # Validate query by executing with LIMIT 0
            self._conn.execute(f"{wrapped} LIMIT 0")

            self._history.append(self._current_query)
            self._current_query = substituted
            self._paginator = self._create_paginator()

            logger.info(
                f"Query executed: {self._paginator.total_rows} rows, "
                f"{self._paginator.total_pages} pages"
            )
            return True, ""

        except Exception as e:
            error_msg = str(e)
            logger.error(f"Query execution failed: {error_msg}")
            return False, error_msg

    def get_page(self, page_num: int) -> pd.DataFrame:
        """Get a specific page of results (1-indexed)."""
        page_num = self._paginator.clamp_page(page_num)
        offset = self._paginator.page_offset(page_num)
        wrapped = self._wrap_query(self._current_query)
        paginated_query = f"{wrapped} LIMIT {self._page_size} OFFSET {offset}"

        try:
            return self._conn.execute(paginated_query).df()
        except Exception as e:
            logger.error(f"Error fetching page {page_num}: {e}")
            return pd.DataFrame()

    def get_unique_values(self, column: str) -> list[Any]:
        """Get unique values for a column (limited to 10000 for performance)."""
        try:
            if self.total_rows > 1_000_000:
                logger.warning(
                    f"Large dataset ({self.total_rows} rows) - "
                    f"unique value calculation may be slow"
                )
            query = f"SELECT DISTINCT {column} FROM ({self._current_query}) LIMIT 10000"
            result = self._conn.execute(query).fetchall()
            return [row[0] for row in result]
        except Exception as e:
            logger.error(f"Error getting unique values for {column}: {e}")
            return []

    def apply_transform(self, query: str) -> tuple[bool, str]:
        """Apply a transformation query that wraps the current query."""
        try:
            wrapped = self._wrap_query(query)
            self._conn.execute(f"{wrapped} LIMIT 0")
            self._history.append(self._current_query)
            self._current_query = query
            self._paginator = self._create_paginator()
            logger.info(
                f"Transform applied: {self._paginator.total_rows} rows, "
                f"{self._paginator.total_pages} pages"
            )
            return True, ""
        except Exception as e:
            error_msg = str(e)
            logger.error(f"Transform failed: {error_msg}")
            return False, error_msg

    def sort_by_column(self, column: str, ascending: bool = True) -> tuple[bool, str]:
        """Sort current results by column."""
        order = "ASC" if ascending else "DESC"
        new_query = f"SELECT * FROM ({self._current_query}) ORDER BY {column} {order}"
        return self.apply_transform(new_query)

    def export_results(self, output_path: Path) -> bool:
        """Export current query results to file."""
        try:
            output_path = Path(output_path)
            suffix = output_path.suffix.lower()

            if suffix == ".csv":
                self._conn.execute(
                    f"COPY ({self._current_query}) TO '{output_path}' (FORMAT CSV, HEADER)"
                )
            elif suffix == ".parquet":
                self._conn.execute(
                    f"COPY ({self._current_query}) TO '{output_path}' (FORMAT PARQUET)"
                )
            elif suffix == ".json":
                self._conn.execute(
                    f"COPY ({self._current_query}) TO '{output_path}' (FORMAT JSON)"
                )
            else:
                logger.error(f"Unsupported export format: {suffix}")
                return False

            logger.info(f"Exported results to {output_path}")
            return True
        except Exception as e:
            logger.error(f"Export failed: {e}")
            return False

    def undo(self) -> tuple[bool, str]:
        """Revert the last transformation by restoring the previous query.

        Returns:
            Tuple of (success: bool, error_message: str or empty)
        """
        if not self._history:
            return False, "Nothing to undo"
        previous_query = self._history.pop()
        try:
            wrapped = self._wrap_query(previous_query)
            self._conn.execute(f"{wrapped} LIMIT 0")
            self._current_query = previous_query
            self._paginator = self._create_paginator()
            logger.info(f"Undo applied: {self._paginator.total_rows} rows")
            return True, ""
        except Exception as e:
            error_msg = str(e)
            logger.error(f"Undo failed: {error_msg}")
            return False, error_msg

    def reset_query(self) -> None:
        """Reset to original file reader query."""
        self._current_query = self._file_reader_query
        self._paginator = self._create_paginator()
        self._history.clear()
        logger.info("Query reset to original file")

    def get_table_info(self) -> dict[str, Any]:
        """Get table metadata."""
        return {
            "total_rows": self.total_rows,
            "total_pages": self.total_pages,
            "page_size": self.page_size,
            "columns": self.get_columns(),
            "file_path": str(self._file_path),
        }

    def close(self) -> None:
        """Close database connection if owned by this engine."""
        if self._owns_connection:
            self._conn.close()
            logger.info("QueryEngine closed")
        else:
            logger.debug("QueryEngine skipping close (shared connection)")
