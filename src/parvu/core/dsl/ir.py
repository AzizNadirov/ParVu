"""
Intermediate Representation (IR) for the ParVu DSL.

Thin wrapper around sqlglot expressions. All DSL operations compile
down to these node types, which are then emitted as DuckDB SQL.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from parvu.core.dsl.types import LogicalType


@dataclass(kw_only=True)
class Expr:
    """Base class for all IR expressions."""
    logical_type: LogicalType = LogicalType.UNKNOWN


@dataclass(kw_only=True)
class ColumnRef(Expr):
    """Reference to a table column: table[col]."""
    table: str
    column: str


@dataclass(kw_only=True)
class Literal(Expr):
    """Constant literal value."""
    value: Any


@dataclass(kw_only=True)
class Call(Expr):
    """Function call: FUNC(arg1, arg2, ...)."""
    func_name: str
    args: list[Expr]


@dataclass(kw_only=True)
class BinaryOp(Expr):
    """Binary operator: left + right, left - right, etc."""
    op: str   # +, -, *, /, =, <>, <, <=, >, >=, AND, OR
    left: Expr
    right: Expr


@dataclass(kw_only=True)
class UnaryOp(Expr):
    """Unary operator: +expr, -expr, NOT expr."""
    op: str   # +, -, NOT
    operand: Expr


@dataclass(kw_only=True)
class MethodCall(Expr):
    """Method call on an expression: expr.method(arg1, ...)."""
    receiver: Expr
    method_name: str
    args: list[Expr]


@dataclass(kw_only=True)
class Assignment(Expr):
    """Virtual column assignment: table[new_col] = expr."""
    table: str
    column: str
    value: Expr


@dataclass(kw_only=True)
class DropDuplicates(Expr):
    """Table-level deduplication: drop_duplicates(table, cols..., keep)."""
    table: str
    columns: list[str]
    keep: str  # "first" or "last"


@dataclass(kw_only=True)
class Replace(Expr):
    """String replacement: REPLACE(text, pattern, with_value, case_sensitive, regex)."""
    text: Expr
    pattern: Expr
    with_value: Expr
    case_sensitive: bool = True
    regex: bool = False


@dataclass(kw_only=True)
class DropNull(Expr):
    """Filter rows where a column equals the null sentinel.

    ``null_value`` semantics:
    - ``None`` (default): drop rows where ``column IS NULL``.
    - any other value:    drop rows where ``column = null_value``.

    Real SQL NULLs are NOT also dropped when a sentinel is supplied; chain
    another DropNull call (with no sentinel) to remove them too.
    """
    table: str
    column: str
    null_value: Any = None
