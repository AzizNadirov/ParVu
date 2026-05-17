"""
Unit tests for edit queue.
"""
import pytest
import pandas as pd

from parvu.core.edit_queue import EditQueue, CellEdit, apply_edits_to_dataframe


def test_edit_queue_add():
    queue = EditQueue()
    edit = CellEdit(absolute_row=5, column="name", old_value="Alice", new_value="Bob")
    queue.add(edit)
    assert queue.is_dirty()
    assert queue.edit_count() == 1
    assert queue.edited_cells_count() == 1


def test_edit_queue_merge_same_cell():
    queue = EditQueue()
    queue.add(CellEdit(absolute_row=5, column="name", old_value="Alice", new_value="Bob"))
    queue.add(CellEdit(absolute_row=5, column="name", old_value="Bob", new_value="Charlie"))

    assert queue.edit_count() == 1
    assert queue.edited_cells_count() == 1
    retrieved = queue.get_edit(5, "name")
    assert retrieved.old_value == "Alice"  # preserved original
    assert retrieved.new_value == "Charlie"


def test_edit_queue_clear():
    queue = EditQueue()
    queue.add(CellEdit(absolute_row=1, column="x", old_value=1, new_value=2))
    queue.clear()
    assert not queue.is_dirty()
    assert queue.edit_count() == 0


def test_edit_queue_get_edit_missing():
    queue = EditQueue()
    assert queue.get_edit(0, "x") is None
    assert not queue.has_edit(0, "x")


def test_apply_edits_to_dataframe():
    df = pd.DataFrame({
        "name": ["Alice", "Bob", "Charlie"],
        "age": [25, 30, 35],
    })
    edits = [
        CellEdit(absolute_row=0, column="name", old_value="Alice", new_value="Alicia"),
        CellEdit(absolute_row=1, column="age", old_value=30, new_value=31),
    ]
    result = apply_edits_to_dataframe(df, edits)
    assert result.iloc[0]["name"] == "Alicia"
    assert result.iloc[1]["age"] == 31
    assert result.iloc[2]["name"] == "Charlie"  # unchanged


def test_apply_edits_empty_queue():
    df = pd.DataFrame({"x": [1, 2, 3]})
    result = apply_edits_to_dataframe(df, [])
    assert result.equals(df)
