"""
Unit tests for the ParVu DSL compiler.

Proves that hand-written ASTs compile to valid DuckDB SQL.
"""
from __future__ import annotations

import pytest

from parvu.core.dsl.ir import ColumnRef, Literal, Call, BinaryOp, MethodCall
from parvu.core.dsl.compiler import Compiler
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.types import LogicalType


class TestCompiler:
    """Test the IR → sqlglot → DuckDB SQL pipeline."""

    @pytest.fixture
    def compiler(self) -> Compiler:
        return Compiler(FunctionRegistry())

    def test_column_ref(self, compiler: Compiler) -> None:
        expr = ColumnRef(table="sales", column="revenue")
        sql = compiler.compile(expr)
        assert sql == 'sales.revenue'

    def test_literal_string(self, compiler: Compiler) -> None:
        expr = Literal(value="hello")
        sql = compiler.compile(expr)
        assert sql == "'hello'"

    def test_literal_number(self, compiler: Compiler) -> None:
        expr = Literal(value=42)
        sql = compiler.compile(expr)
        assert sql == "42"

    def test_literal_float(self, compiler: Compiler) -> None:
        expr = Literal(value=3.14)
        sql = compiler.compile(expr)
        assert sql == "3.14"

    def test_literal_boolean(self, compiler: Compiler) -> None:
        expr = Literal(value=True)
        sql = compiler.compile(expr)
        assert sql == "TRUE"

    def test_literal_null(self, compiler: Compiler) -> None:
        expr = Literal(value=None)
        sql = compiler.compile(expr)
        assert sql == "NULL"

    def test_call_upper(self, compiler: Compiler) -> None:
        expr = Call(func_name="UPPER", args=[ColumnRef(table="products", column="name")])
        sql = compiler.compile(expr)
        assert sql == 'UPPER(products.name)'

    def test_call_sum(self, compiler: Compiler) -> None:
        expr = Call(func_name="SUM", args=[ColumnRef(table="sales", column="revenue")])
        sql = compiler.compile(expr)
        assert sql == 'SUM(sales.revenue)'

    def test_call_if(self, compiler: Compiler) -> None:
        expr = Call(func_name="IF", args=[
            BinaryOp(op=">", left=ColumnRef(table="sales", column="revenue"), right=Literal(value=100)),
            Literal(value="high"),
            Literal(value="low"),
        ])
        sql = compiler.compile(expr)
        assert sql == "IF(sales.revenue > 100, 'high', 'low')"

    def test_binary_op_add(self, compiler: Compiler) -> None:
        expr = BinaryOp(op="+", left=ColumnRef(table="sales", column="a"), right=ColumnRef(table="sales", column="b"))
        sql = compiler.compile(expr)
        assert sql == 'sales.a + sales.b'

    def test_binary_op_and(self, compiler: Compiler) -> None:
        expr = BinaryOp(op="AND", left=ColumnRef(table="sales", column="flag"), right=ColumnRef(table="sales", column="active"))
        sql = compiler.compile(expr)
        assert sql == 'sales.flag AND sales.active'

    def test_method_call_upper(self, compiler: Compiler) -> None:
        expr = MethodCall(
            receiver=ColumnRef(table="products", column="name"),
            method_name="upper",
            args=[],
        )
        sql = compiler.compile(expr)
        assert sql == 'UPPER(products.name)'

    def test_method_call_contains(self, compiler: Compiler) -> None:
        expr = MethodCall(
            receiver=ColumnRef(table="products", column="name"),
            method_name="contains",
            args=[Literal(value="apple")],
        )
        sql = compiler.compile(expr)
        assert sql == "CONTAINS(products.name, 'apple')"

    def test_method_call_round(self, compiler: Compiler) -> None:
        expr = MethodCall(
            receiver=ColumnRef(table="sales", column="revenue"),
            method_name="round",
            args=[Literal(value=2)],
        )
        sql = compiler.compile(expr)
        assert sql == 'ROUND(sales.revenue, 2)'

    def test_complex_expression(self, compiler: Compiler) -> None:
        """SUM(sales[revenue]) / COUNT(sales[id])"""
        expr = BinaryOp(
            op="/",
            left=Call(func_name="SUM", args=[ColumnRef(table="sales", column="revenue")]),
            right=Call(func_name="COUNT", args=[ColumnRef(table="sales", column="id")]),
        )
        sql = compiler.compile(expr)
        assert sql == 'SUM(sales.revenue) / COUNT(sales.id)'

    def test_unknown_function_compiles(self, compiler: Compiler) -> None:
        """Unknown functions compile as-is (sqlglot passthrough)."""
        expr = Call(func_name="FOOBAR", args=[Literal(value=1)])
        sql = compiler.compile(expr)
        assert sql == "FOOBAR(1)"

    def test_unknown_method_raises(self, compiler: Compiler) -> None:
        from parvu.core.dsl.compiler import CompileError
        expr = MethodCall(
            receiver=ColumnRef(table="sales", column="x"),
            method_name="nonexistent",
            args=[],
        )
        with pytest.raises(CompileError):
            compiler.compile(expr)
