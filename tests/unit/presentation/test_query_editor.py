"""
Unit tests for QueryEditor expression-mode helpers.
"""
from __future__ import annotations

import pytest

from parvu.presentation.widgets.query_editor import QueryEditor
from parvu.core.dsl.catalog import Catalog, ColumnInfo
from parvu.core.dsl.types import LogicalType


class TestQueryEditorExpressionInfo:
    """Test get_expression_info() metadata extraction."""

    @pytest.fixture
    def catalog(self) -> Catalog:
        cat = Catalog()
        cat._tabs["sales"] = {
            "revenue": ColumnInfo(
                name="revenue",
                physical_type="DOUBLE",
                logical_type=LogicalType.NUMERIC,
            ),
            "name": ColumnInfo(
                name="name",
                physical_type="VARCHAR",
                logical_type=LogicalType.TEXT,
            ),
        }
        return cat

    def test_get_expression_info_assignment(self, qtbot, catalog: Catalog) -> None:
        editor = QueryEditor(catalog=catalog)
        qtbot.addWidget(editor)
        editor.set_expression_mode(True)
        editor.set_query("sales[new_col] = sales[revenue] * 2")

        info = editor.get_expression_info()
        assert info["type"] == "assignment"
        assert info["table"] == "sales"
        assert info["column"] == "new_col"

    def test_get_expression_info_scalar(self, qtbot, catalog: Catalog) -> None:
        editor = QueryEditor(catalog=catalog)
        qtbot.addWidget(editor)
        editor.set_expression_mode(True)
        editor.set_query("sales[revenue] + 100")

        info = editor.get_expression_info()
        assert info["type"] == "expression"

    def test_get_expression_info_empty(self, qtbot, catalog: Catalog) -> None:
        editor = QueryEditor(catalog=catalog)
        qtbot.addWidget(editor)
        editor.set_expression_mode(True)
        editor.set_query("")

        info = editor.get_expression_info()
        assert info == {}

    def test_get_expression_info_sql_mode(self, qtbot, catalog: Catalog) -> None:
        editor = QueryEditor(catalog=catalog)
        qtbot.addWidget(editor)
        editor.set_expression_mode(False)
        editor.set_query("SELECT * FROM sales")

        info = editor.get_expression_info()
        assert info == {}
