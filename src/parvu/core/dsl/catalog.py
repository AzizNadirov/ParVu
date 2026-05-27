"""
Catalog — maps tab names to column names and DuckDB physical types.

Populated from DuckDB's information_schema. Caches metadata per tab
and invalidates on schema changes.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from loguru import logger

from parvu.core.query_engine import QueryEngine
from parvu.core.dsl.types import LogicalType, duckdb_type_to_logical


@dataclass
class ColumnInfo:
    """Metadata for a single column."""
    name: str
    physical_type: str
    logical_type: LogicalType


class Catalog:
    """Catalog of tables and columns known to the DSL."""

    def __init__(self):
        self._tabs: dict[str, dict[str, ColumnInfo]] = {}

    def register_tab(self, name: str, engine: QueryEngine) -> None:
        """Register a tab by reading column metadata from DuckDB."""
        columns: dict[str, ColumnInfo] = {}
        try:
            query = f"""
                SELECT column_name, data_type
                FROM information_schema.columns
                WHERE table_name = '{name}'
                ORDER BY ordinal_position
            """
            df = engine._conn.execute(query).df()
            for _, row in df.iterrows():
                col_name = row["column_name"]
                phys = row["data_type"]
                columns[col_name] = ColumnInfo(
                    name=col_name,
                    physical_type=phys,
                    logical_type=duckdb_type_to_logical(phys),
                )
        except Exception:
            try:
                result = engine._conn.execute(f"DESCRIBE {engine._wrap_query(engine.current_query)}")
                for row in result.fetchall():
                    col_name = row[0]
                    phys = str(row[1])
                    columns[col_name] = ColumnInfo(
                        name=col_name,
                        physical_type=phys,
                        logical_type=duckdb_type_to_logical(phys),
                    )
            except Exception as e:
                logger.warning(f"Catalog failed to read columns for '{name}': {e}")

        self._tabs[name] = columns
        logger.debug(f"Catalog registered tab '{name}' with {len(columns)} columns")

    def unregister_tab(self, name: str) -> None:
        """Remove a tab from the catalog."""
        self._tabs.pop(name, None)

    def clear(self) -> None:
        """Clear all cached metadata."""
        self._tabs.clear()

    def columns(self, tab: str) -> list[str]:
        """Return column names for a tab."""
        return list(self._tabs.get(tab, {}).keys())

    def column_info(self, tab: str, col: str) -> ColumnInfo | None:
        """Return metadata for a specific column."""
        return self._tabs.get(tab, {}).get(col)

    def resolve_column(self, tab: str, col: str) -> LogicalType:
        """Return the logical type of a column, or UNKNOWN."""
        info = self.column_info(tab, col)
        return info.logical_type if info else LogicalType.UNKNOWN

    def tables(self) -> list[str]:
        """Return all registered tab names."""
        return list(self._tabs.keys())
