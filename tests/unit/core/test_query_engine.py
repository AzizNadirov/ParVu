"""
Unit tests for QueryEngine undo and history.
"""
from __future__ import annotations

from pathlib import Path

import duckdb
import pytest

from parvu.core.query_engine import QueryEngine


class TestQueryEngineUndo:
    """Test query history and undo functionality."""

    @pytest.fixture
    def engine(self) -> QueryEngine:
        conn = duckdb.connect(":memory:")
        conn.execute("CREATE TABLE test AS SELECT 1 AS a, 2 AS b UNION ALL SELECT 3, 4")
        engine = QueryEngine(
            file_path=Path("dummy.parquet"),
            table_name="test",
            conn=conn,
        )
        return engine

    def test_can_undo_initially_false(self, engine: QueryEngine) -> None:
        assert not engine.can_undo

    def test_apply_transform_enables_undo(self, engine: QueryEngine) -> None:
        success, _ = engine.apply_transform("SELECT * FROM test WHERE a = 1")
        assert success
        assert engine.can_undo

    def test_undo_restores_previous_query(self, engine: QueryEngine) -> None:
        original_query = engine.current_query
        engine.apply_transform("SELECT * FROM test WHERE a = 1")
        assert engine.current_query != original_query

        success, _ = engine.undo()
        assert success
        assert engine.current_query == original_query
        assert not engine.can_undo

    def test_undo_multiple_steps(self, engine: QueryEngine) -> None:
        q1 = engine.current_query
        engine.apply_transform("SELECT * FROM test WHERE a = 1")
        q2 = engine.current_query
        engine.apply_transform("SELECT * FROM test WHERE a = 1 AND b = 2")
        q3 = engine.current_query

        success, _ = engine.undo()
        assert success
        assert engine.current_query == q2

        success, _ = engine.undo()
        assert success
        assert engine.current_query == q1
        assert not engine.can_undo

    def test_undo_nothing_fails(self, engine: QueryEngine) -> None:
        success, error = engine.undo()
        assert not success
        assert "Nothing to undo" in error

    def test_execute_query_enables_undo(self, engine: QueryEngine) -> None:
        original_query = engine.current_query
        success, _ = engine.execute_query("SELECT * FROM test WHERE a = 1")
        assert success
        assert engine.can_undo
        assert engine.current_query != original_query

        success, _ = engine.undo()
        assert success
        assert engine.current_query == original_query

    def test_reset_query_clears_history(self, engine: QueryEngine) -> None:
        engine.apply_transform("SELECT * FROM test WHERE a = 1")
        assert engine.can_undo
        engine.reset_query()
        assert not engine.can_undo

    def test_undo_preserves_row_counts(self, engine: QueryEngine) -> None:
        original_rows = engine.total_rows
        engine.apply_transform("SELECT * FROM test WHERE a = 99")  # no rows
        assert engine.total_rows == 0

        success, _ = engine.undo()
        assert success
        assert engine.total_rows == original_rows
