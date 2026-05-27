"""
Unit tests for the ParVu DSL parser.
"""
from __future__ import annotations

import pytest

from parvu.core.dsl.parser import DSLParser, ParseError


class TestParser:
    """Test the Lark-based expression parser."""

    @pytest.fixture
    def parser(self) -> DSLParser:
        return DSLParser()

    def test_column_ref(self, parser: DSLParser) -> None:
        tree = parser.parse("sales[revenue]")
        assert tree.data == "column_ref"

    def test_literal_number(self, parser: DSLParser) -> None:
        tree = parser.parse("42")
        assert tree.data == "number"

    def test_literal_string(self, parser: DSLParser) -> None:
        tree = parser.parse("'hello'")
        assert tree.data == "string"

    def test_literal_boolean(self, parser: DSLParser) -> None:
        tree = parser.parse("TRUE")
        assert tree.data == "boolean"

    def test_literal_null(self, parser: DSLParser) -> None:
        tree = parser.parse("NULL")
        assert tree.data == "null"

    def test_function_call_no_args(self, parser: DSLParser) -> None:
        tree = parser.parse("TODAY()")
        assert tree.data == "func_call"

    def test_function_call_one_arg(self, parser: DSLParser) -> None:
        tree = parser.parse("SUM(sales[revenue])")
        assert tree.data == "func_call"

    def test_function_call_multi_args(self, parser: DSLParser) -> None:
        tree = parser.parse("IF(sales[revenue] > 100, 'high', 'low')")
        assert tree.data == "func_call"

    def test_binary_op_add(self, parser: DSLParser) -> None:
        tree = parser.parse("sales[a] + sales[b]")
        assert tree.data == "add"

    def test_binary_op_div(self, parser: DSLParser) -> None:
        tree = parser.parse("sales[revenue] / sales[count]")
        assert tree.data == "div"

    def test_method_call(self, parser: DSLParser) -> None:
        tree = parser.parse("sales[name].upper")
        assert tree.data == "method_call"

    def test_complex_expression(self, parser: DSLParser) -> None:
        tree = parser.parse("SUM(sales[revenue]) / COUNT(sales[id])")
        assert tree.data == "div"

    def test_parenthesized(self, parser: DSLParser) -> None:
        tree = parser.parse("(sales[a] + sales[b])")
        # Parenthesized expr unwraps to the inner expression
        assert tree.data == "add"

    def test_parse_error(self, parser: DSLParser) -> None:
        with pytest.raises(ParseError):
            parser.parse("@#$%^&*")

    def test_parse_partial_incomplete(self, parser: DSLParser) -> None:
        """Partial parse should return None on incomplete input."""
        result = parser.parse_partial("SUM(")
        assert result is None

    def test_parse_partial_valid(self, parser: DSLParser) -> None:
        result = parser.parse_partial("sales[revenue]")
        assert result is not None

    def test_assignment(self, parser: DSLParser) -> None:
        tree = parser.parse("sales[new_col] = sales[revenue] + 100")
        assert tree.data == "assignment"

    def test_assignment_quoted_column(self, parser: DSLParser) -> None:
        tree = parser.parse('sales["new col"] = sales[revenue]')
        assert tree.data == "assignment"

    def test_drop_duplicates(self, parser: DSLParser) -> None:
        tree = parser.parse('drop_duplicates(sales[id], sales[name], "first")')
        assert tree.data == "func_call"
