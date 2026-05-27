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

from parvu.core.dsl.ir import Expr, ColumnRef, Literal, Call, BinaryOp, UnaryOp, MethodCall, Assignment, DropDuplicates, Replace
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
        if isinstance(expr, Assignment):
            # SELECT *, <value> AS "<new_col>" FROM <table>
            value_expr = self._to_sqlglot(expr.value)
            alias = exp.Alias(this=value_expr, alias=exp.to_identifier(expr.column, quoted=True))
            star = exp.Star()
            return exp.Select(
                expressions=[star, alias],
            ).from_(exp.to_identifier(expr.table))

        if isinstance(expr, DropDuplicates):
            # drop_duplicates(table, cols..., keep)
            table_id = exp.to_identifier(expr.table)
            star = exp.Star()
            if expr.keep == "first" and not expr.columns:
                # SELECT DISTINCT * FROM table
                return exp.Select(expressions=[star], distinct=True).from_(table_id)
            # QUALIFY ROW_NUMBER() OVER (PARTITION BY cols [ORDER BY rowid DESC]) = 1
            partition_cols = [exp.to_identifier(c) for c in expr.columns]
            window_this = exp.Anonymous(this="ROW_NUMBER", expressions=[])
            if expr.keep == "last":
                order = exp.Order(expressions=[exp.Ordered(this=exp.to_identifier("rowid"), desc=True)])
                window = exp.Window(this=window_this, partition_by=partition_cols, order=order)
            else:
                window = exp.Window(this=window_this, partition_by=partition_cols)
            qualify_expr = exp.EQ(this=window, expression=exp.Literal.number(1))
            return exp.Select(
                expressions=[star],
                qualify=exp.Qualify(this=qualify_expr),
            ).from_(table_id)

        if isinstance(expr, Replace):
            text = self._to_sqlglot(expr.text)
            replacement = self._to_sqlglot(expr.with_value)
            # Simple literal replacement (case-sensitive, no regex)
            if not expr.regex and expr.case_sensitive:
                pattern = self._to_sqlglot(expr.pattern)
                return exp.Anonymous(this="REPLACE", expressions=[text, pattern, replacement])
            # Regex or case-insensitive: use REGEXP_REPLACE with flags
            flags = "g"
            if not expr.case_sensitive:
                flags += "i"
            if not expr.regex:
                # Escape literal pattern so metacharacters are treated literally
                if isinstance(expr.pattern, Literal) and isinstance(expr.pattern.value, str):
                    import re as _re
                    pattern = exp.Literal.string(_re.escape(expr.pattern.value))
                else:
                    pattern = self._to_sqlglot(expr.pattern)
            else:
                pattern = self._to_sqlglot(expr.pattern)
            return exp.Anonymous(
                this="REGEXP_REPLACE",
                expressions=[text, pattern, replacement, exp.Literal.string(flags)],
            )

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
