"""
Unit tests for TableTab undo stack integration.
"""
from __future__ import annotations

from pathlib import Path

import duckdb
import pytest

from parvu.presentation.models.table_tab import TableTab
from parvu.core.query_engine import QueryEngine


class TestTableTabUndoStack:
    """Test applied_steps and _undo_stack stay in sync."""

    @pytest.fixture
    def tab(self) -> TableTab:
        conn = duckdb.connect(":memory:")
        conn.execute("CREATE TABLE test AS SELECT 1 AS a, 2 AS b")
        engine = QueryEngine(
            file_path=Path("dummy.parquet"),
            table_name="test",
            conn=conn,
        )
        return TableTab(name="test", file_path=Path("dummy.parquet"), engine=engine)

    def test_sql_step_populates_undo_stack(self, tab: TableTab) -> None:
        tab.applied_steps.append("Sort 'a' ascending")
        tab._undo_stack.append({"type": "sql"})
        assert len(tab.applied_steps) == 1
        assert len(tab._undo_stack) == 1
        assert tab._undo_stack[0]["type"] == "sql"

    def test_cell_edit_step_populates_undo_stack(self, tab: TableTab) -> None:
        tab.applied_steps.append("Edit 'a' at row 1")
        tab._undo_stack.append({"type": "cell_edit", "row": 0, "column": "a"})
        assert len(tab.applied_steps) == 1
        assert len(tab._undo_stack) == 1
        assert tab._undo_stack[0]["type"] == "cell_edit"
        assert tab._undo_stack[0]["row"] == 0

    def test_clear_clears_both_lists(self, tab: TableTab) -> None:
        tab.applied_steps.append("Sort 'a' ascending")
        tab._undo_stack.append({"type": "sql"})
        tab.applied_steps.clear()
        tab._undo_stack.clear()
        assert len(tab.applied_steps) == 0
        assert len(tab._undo_stack) == 0

    def test_pop_both_lists_stays_in_sync(self, tab: TableTab) -> None:
        tab.applied_steps.append("Sort 'a' ascending")
        tab._undo_stack.append({"type": "sql"})
        tab.applied_steps.append("Edit 'b' at row 2")
        tab._undo_stack.append({"type": "cell_edit", "row": 1, "column": "b"})

        # Pop last (cell edit)
        undo_info = tab._undo_stack.pop()
        tab.applied_steps.pop()
        assert undo_info["type"] == "cell_edit"
        assert len(tab.applied_steps) == 1
        assert len(tab._undo_stack) == 1

        # Pop remaining (sql)
        undo_info = tab._undo_stack.pop()
        tab.applied_steps.pop()
        assert undo_info["type"] == "sql"
        assert len(tab.applied_steps) == 0
        assert len(tab._undo_stack) == 0
