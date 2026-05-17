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
class MethodCall(Expr):
    """Method call on an expression: expr.method(arg1, ...)."""
    receiver: Expr
    method_name: str
    args: list[Expr]
