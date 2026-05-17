"""
Logical type system for the ParVu DSL.

Maps DuckDB physical types to a small set of logical types,
each with an associated method set.
"""
from __future__ import annotations

from enum import Enum, auto
from dataclasses import dataclass
from typing import Callable


class LogicalType(Enum):
    """Logical data types in the ParVu expression language."""

    UNKNOWN = auto()
    NUMERIC = auto()       # INTEGER, BIGINT, DOUBLE, DECIMAL
    TEXT = auto()          # VARCHAR
    BOOLEAN = auto()
    DATE = auto()
    TIMESTAMP = auto()
    LIST = auto()          # List<T>
    STRUCT = auto()        # Struct{...}


# DuckDB physical type → LogicalType mapping
_PHYSICAL_TO_LOGICAL: dict[str, LogicalType] = {
    "INTEGER": LogicalType.NUMERIC,
    "BIGINT": LogicalType.NUMERIC,
    "SMALLINT": LogicalType.NUMERIC,
    "TINYINT": LogicalType.NUMERIC,
    "DOUBLE": LogicalType.NUMERIC,
    "FLOAT": LogicalType.NUMERIC,
    "DECIMAL": LogicalType.NUMERIC,
    "VARCHAR": LogicalType.TEXT,
    "TEXT": LogicalType.TEXT,
    "BOOLEAN": LogicalType.BOOLEAN,
    "DATE": LogicalType.DATE,
    "TIMESTAMP": LogicalType.TIMESTAMP,
    "TIMESTAMP WITH TIME ZONE": LogicalType.TIMESTAMP,
    "LIST": LogicalType.LIST,
    "STRUCT": LogicalType.STRUCT,
}


def duckdb_type_to_logical(physical: str) -> LogicalType:
    """Convert a DuckDB physical type name to a LogicalType."""
    # Normalize: strip params like DECIMAL(10,2)
    base = physical.split("(")[0].strip().upper()
    return _PHYSICAL_TO_LOGICAL.get(base, LogicalType.UNKNOWN)


# Method sets per logical type
# Each entry is (method_name, function_name_in_registry)
_METHOD_SETS: dict[LogicalType, list[str]] = {
    LogicalType.TEXT: [
        "upper",
        "lower",
        "len",
        "trim",
        "contains",
        "startswith",
        "endswith",
    ],
    LogicalType.NUMERIC: [
        "abs",
        "round",
        "floor",
        "ceil",
        "power",
        "sqrt",
    ],
    LogicalType.DATE: [
        "year",
        "month",
        "day",
        "add_days",
    ],
    LogicalType.BOOLEAN: [
        "if_true",
    ],
}


def methods_for_type(logical_type: LogicalType) -> list[str]:
    """Return the list of method names available on a given logical type."""
    return list(_METHOD_SETS.get(logical_type, []))
