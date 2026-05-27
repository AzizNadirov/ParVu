"""
Unit tests for the ParVu DSL resolver + type checker.
"""
from __future__ import annotations

import pytest

from parvu.core.dsl.parser import DSLParser
from parvu.core.dsl.resolver import Resolver, ResolutionError
from parvu.core.dsl.compiler import Compiler
from parvu.core.dsl.catalog import Catalog
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.ir import ColumnRef, Literal, Call, BinaryOp, MethodCall, Assignment
from parvu.core.dsl.types import LogicalType


class TestResolver:
    """Test parse → resolve → compile pipeline."""

    @pytest.fixture
    def parser(self) -> DSLParser:
        return DSLParser()

    @pytest.fixture
    def catalog(self) -> Catalog:
        cat = Catalog()
        # Manually inject column metadata
        cat._tabs["sales"] = {
            "revenue": type("CI", (), {"name": "revenue", "physical_type": "DOUBLE", "logical_type": LogicalType.NUMERIC})(),
            "id": type("CI", (), {"name": "id", "physical_type": "INTEGER", "logical_type": LogicalType.NUMERIC})(),
            "name": type("CI", (), {"name": "name", "physical_type": "VARCHAR", "logical_type": LogicalType.TEXT})(),
        }
        return cat

    @pytest.fixture
    def registry(self) -> FunctionRegistry:
        return FunctionRegistry()

    @pytest.fixture
    def resolver(self, catalog: Catalog, registry: FunctionRegistry) -> Resolver:
        return Resolver(catalog, registry)

    def test_resolve_column_ref(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("sales[revenue]")
        expr = resolver.resolve(tree)
        assert isinstance(expr, ColumnRef)
        assert expr.table == "sales"
        assert expr.column == "revenue"
        assert expr.logical_type == LogicalType.NUMERIC

    def test_resolve_literal_number(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("42")
        expr = resolver.resolve(tree)
        assert isinstance(expr, Literal)
        assert expr.value == 42
        assert expr.logical_type == LogicalType.NUMERIC

    def test_resolve_function_call(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("SUM(sales[revenue])")
        expr = resolver.resolve(tree)
        assert isinstance(expr, Call)
        assert expr.func_name == "SUM"
        assert expr.logical_type == LogicalType.NUMERIC

    def test_resolve_binary_op(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("sales[revenue] + 100")
        expr = resolver.resolve(tree)
        assert isinstance(expr, BinaryOp)
        assert expr.op == "+"

    def test_resolve_method_call(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("sales[name].upper")
        expr = resolver.resolve(tree)
        assert isinstance(expr, MethodCall)
        assert expr.method_name == "upper"
        assert expr.logical_type == LogicalType.TEXT

    def test_resolve_unknown_table_raises(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("unknown[col]")
        with pytest.raises(ResolutionError, match="Unknown table"):
            resolver.resolve(tree)

    def test_resolve_unknown_column_raises(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("sales[unknown_col]")
        with pytest.raises(ResolutionError, match="Unknown column"):
            resolver.resolve(tree)

    def test_resolve_unknown_function_raises(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("UNKNOWN_FUNC(sales[revenue])")
        with pytest.raises(ResolutionError, match="Unknown function"):
            resolver.resolve(tree)

    def test_end_to_end_compile(self, parser: DSLParser, resolver: Resolver) -> None:
        """Parse → resolve → compile → valid DuckDB SQL."""
        tree = parser.parse("SUM(sales[revenue]) / COUNT(sales[id])")
        expr = resolver.resolve(tree)
        sql = Compiler().compile(expr)
        assert sql == "SUM(sales.revenue) / COUNT(sales.id)"

    def test_end_to_end_if(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("IF(sales[revenue] > 100, 'high', 'low')")
        expr = resolver.resolve(tree)
        sql = Compiler().compile(expr)
        assert sql == "IF(sales.revenue > 100, 'high', 'low')"

    def test_resolve_assignment(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("sales[new_col] = UPPER(sales[name])")
        expr = resolver.resolve(tree)
        assert isinstance(expr, Assignment)
        assert expr.table == "sales"
        assert expr.column == "new_col"
        assert expr.logical_type == LogicalType.TEXT

    def test_resolve_assignment_unknown_table_raises(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("unknown[new_col] = 1")
        with pytest.raises(ResolutionError, match="Unknown table"):
            resolver.resolve(tree)

    def test_resolve_assignment_allows_unknown_column(self, parser: DSLParser, resolver: Resolver) -> None:
        """Target column in assignment need not exist yet."""
        tree = parser.parse("sales[totally_new_column] = sales[revenue] * 2")
        expr = resolver.resolve(tree)
        assert isinstance(expr, Assignment)
        assert expr.column == "totally_new_column"

    def test_end_to_end_assignment_compile(self, parser: DSLParser, resolver: Resolver) -> None:
        tree = parser.parse("sales[new_col] = CONCAT(sales[name], '!')")
        expr = resolver.resolve(tree)
        sql = Compiler().compile(expr)
        assert sql == 'SELECT *, CONCAT(sales.name, \'!\') AS "new_col" FROM sales'
