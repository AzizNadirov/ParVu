"""
Compiler — IR → sqlglot → DuckDB SQL.

The single entry point is `Compiler.compile(expr)` which returns a
DuckDB SQL string. sqlglot handles dialect-specific quoting, function
names, and edge cases.
"""
from __future__ import annotations

import sqlglot
from sqlglot import exp
from loguru import logger

from parvu.core.dsl.ir import Expr, ColumnRef, Literal, Call, BinaryOp, UnaryOp, MethodCall
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.types import LogicalType


class CompileError(Exception):
    """Raised when an expression cannot be compiled to SQL."""
    pass


class Compiler:
    """Compiles ParVu DSL IR to DuckDB SQL via sqlglot."""

    def __init__(self, registry: FunctionRegistry | None = None):
        self._registry = registry or FunctionRegistry()

    def compile(self, expr: Expr) -> str:
        """Compile an IR expression to DuckDB SQL string."""
        sg_expr = self._to_sqlglot(expr)
        sql = sg_expr.sql(dialect="duckdb")
        logger.debug(f"Compiled to SQL: {sql[:60]}...")
        return sql

    def _to_sqlglot(self, expr: Expr) -> exp.Expression:
        """Convert an IR node to a sqlglot expression."""
        if isinstance(expr, ColumnRef):
            # table.col
            return exp.Column(
                this=exp.to_identifier(expr.column),
                table=exp.to_identifier(expr.table),
            )

        if isinstance(expr, Literal):
            if isinstance(expr.value, str):
                return exp.Literal.string(expr.value)
            if isinstance(expr.value, bool):
                return exp.Boolean(this=str(expr.value).upper())
            if expr.value is None:
                return exp.Null()
            return exp.Literal.number(expr.value)

        if isinstance(expr, Call):
            func_def = self._registry.lookup(expr.func_name)
            sqlglot_name = func_def.sqlglot_name if func_def else expr.func_name
            args = [self._to_sqlglot(a) for a in expr.args]
            return exp.Anonymous(this=sqlglot_name, expressions=args)

        if isinstance(expr, UnaryOp):
            operand = self._to_sqlglot(expr.operand)
            if expr.op == "+":
                return operand
            if expr.op == "-":
                return exp.Neg(this=operand)
            if expr.op == "NOT":
                return exp.Not(this=operand)
            raise CompileError(f"Unknown unary operator: {expr.op}")

        if isinstance(expr, BinaryOp):
            left = self._to_sqlglot(expr.left)
            right = self._to_sqlglot(expr.right)
            op_map = {
                "+": exp.Add,
                "-": exp.Sub,
                "*": exp.Mul,
                "/": exp.Div,
                "=": exp.EQ,
                "==": exp.EQ,
                "<>": exp.NEQ,
                "!=": exp.NEQ,
                "<": exp.LT,
                "<=": exp.LTE,
                ">": exp.GT,
                ">=": exp.GTE,
                "AND": exp.And,
                "OR": exp.Or,
            }
            cls = op_map.get(expr.op.upper())
            if cls is None:
                raise CompileError(f"Unknown binary operator: {expr.op}")
            return cls(this=left, expression=right)

        if isinstance(expr, MethodCall):
            # Desugar method call to function call
            func_def = self._registry.lookup_method(expr.method_name)
            if func_def is None:
                raise CompileError(f"Unknown method: {expr.method_name}")
            sqlglot_name = func_def.sqlglot_name or func_def.name
            args = [self._to_sqlglot(expr.receiver)] + [self._to_sqlglot(a) for a in expr.args]
            return exp.Anonymous(this=sqlglot_name, expressions=args)

        raise CompileError(f"Unknown expression type: {type(expr).__name__}")
