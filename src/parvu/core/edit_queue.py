"""
Edit queue for cell-level modifications.

Tracks pending edits in memory until the user explicitly saves.
Works with paginated views by tracking absolute row indices.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from enum import Enum, auto

import pandas as pd
from loguru import logger


class EditType(Enum):
    """Type of edit operation."""
    CELL_EDIT = auto()
    ROW_DELETE = auto()
    ROW_INSERT = auto()


@dataclass(frozen=True, slots=True)
class CellEdit:
    """A single cell edit operation."""
    absolute_row: int  # 0-indexed absolute row in the full dataset
    column: str
    old_value: Any
    new_value: Any
    page_num: int = 1  # which page the edit was made on

    def __repr__(self) -> str:
        return (
            f"CellEdit(row={self.absolute_row}, col={self.column!r}, "
            f"{self.old_value!r} → {self.new_value!r})"
        )


@dataclass
class EditQueue:
    """Queue of pending edits waiting to be saved."""

    _edits: list[CellEdit] = field(default_factory=list)
    _index: dict[tuple[int, str], CellEdit] = field(default_factory=dict)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def add(self, edit: CellEdit) -> None:
        """Add or update a cell edit."""
        key = (edit.absolute_row, edit.column)

        # If we already edited this cell, replace it (but keep original old_value)
        if key in self._index:
            existing = self._index[key]
            # Create new edit with same old_value but new new_value
            merged = CellEdit(
                absolute_row=edit.absolute_row,
                column=edit.column,
                old_value=existing.old_value,
                new_value=edit.new_value,
                page_num=edit.page_num,
            )
            self._index[key] = merged
            # Replace in list
            for i, e in enumerate(self._edits):
                if (e.absolute_row, e.column) == key:
                    self._edits[i] = merged
                    break
        else:
            self._index[key] = edit
            self._edits.append(edit)

        logger.debug(f"Edit queued: {edit}")

    def remove(self, absolute_row: int, column: str) -> None:
        """Remove an edit for a specific cell."""
        key = (absolute_row, column)
        if key in self._index:
            del self._index[key]
            self._edits = [e for e in self._edits if (e.absolute_row, e.column) != key]

    def get_edit(self, absolute_row: int, column: str) -> CellEdit | None:
        """Get the pending edit for a specific cell, if any."""
        return self._index.get((absolute_row, column))

    def has_edit(self, absolute_row: int, column: str) -> bool:
        """Check if a cell has a pending edit."""
        return (absolute_row, column) in self._index

    def is_dirty(self) -> bool:
        """Return True if there are unsaved changes."""
        return len(self._edits) > 0

    def clear(self) -> None:
        """Clear all pending edits (e.g., after successful save)."""
        count = len(self._edits)
        self._edits.clear()
        self._index.clear()
        logger.info(f"Edit queue cleared ({count} edits discarded)")

    def all_edits(self) -> list[CellEdit]:
        """Return all pending edits."""
        return list(self._edits)

    def edit_count(self) -> int:
        """Return number of pending edits."""
        return len(self._edits)

    def edited_cells_count(self) -> int:
        """Return number of unique cells edited."""
        return len(self._index)

    def __len__(self) -> int:
        return len(self._edits)

    def __repr__(self) -> str:
        return f"EditQueue({len(self._edits)} edits, {len(self._index)} unique cells)"


# ------------------------------------------------------------------
# Apply edits to a DataFrame
# ------------------------------------------------------------------

def apply_edits_to_dataframe(df: pd.DataFrame, edits: list[CellEdit]) -> pd.DataFrame:
    """
    Apply a list of cell edits to a DataFrame.

    Args:
        df: The DataFrame to modify.
        edits: List of CellEdit operations.

    Returns:
        Modified DataFrame (copy).
    """
    if not edits:
        return df.copy()

    result = df.copy()

    for edit in edits:
        if edit.absolute_row < 0 or edit.absolute_row >= len(result):
            logger.warning(
                f"Skipping edit for out-of-bounds row {edit.absolute_row} "
                f"(df has {len(result)} rows)"
            )
            continue

        if edit.column not in result.columns:
            logger.warning(f"Skipping edit for unknown column {edit.column}")
            continue

        # Try to preserve the original dtype
        try:
            col_dtype = result[edit.column].dtype
            if col_dtype.kind in 'iufc':  # integer, unsigned, float, complex
                result.iloc[edit.absolute_row, result.columns.get_loc(edit.column)] = pd.to_numeric(edit.new_value, errors='coerce')
            elif col_dtype.kind == 'b':  # boolean
                result.iloc[edit.absolute_row, result.columns.get_loc(edit.column)] = edit.new_value.lower() in ('true', '1', 'yes', 'on') if isinstance(edit.new_value, str) else bool(edit.new_value)
            else:
                result.iloc[edit.absolute_row, result.columns.get_loc(edit.column)] = edit.new_value
        except Exception as e:
            logger.warning(f"Could not preserve dtype for edit {edit}: {e}")
            result.iloc[edit.absolute_row, result.columns.get_loc(edit.column)] = edit.new_value

    return result


def read_source_file(file_path: str | Any, adapter_registry: Any = None) -> pd.DataFrame:
    """
    Read the source file into a pandas DataFrame for editing.

    Args:
        file_path: Path to the source file.
        adapter_registry: Optional FileAdapterRegistry for format detection.

    Returns:
        Full DataFrame.
    """
    from pathlib import Path
    path = Path(file_path)
    suffix = path.suffix.lower()

    if suffix == '.parquet':
        return pd.read_parquet(path)
    elif suffix == '.csv':
        return pd.read_csv(path)
    elif suffix == '.json':
        return pd.read_json(path)
    elif suffix in ('.xlsx', '.xls'):
        return pd.read_excel(path)
    else:
        raise ValueError(f"Unsupported file format for editing: {suffix}")


def write_source_file(df: pd.DataFrame, file_path: str | Any) -> None:
    """
    Write a DataFrame back to a file, preserving the original format.

    Args:
        df: DataFrame to write.
        file_path: Target path.
    """
    from pathlib import Path
    path = Path(file_path)
    suffix = path.suffix.lower()

    if suffix == '.parquet':
        df.to_parquet(path, index=False)
    elif suffix == '.csv':
        df.to_csv(path, index=False)
    elif suffix == '.json':
        df.to_json(path, orient='records', indent=2)
    elif suffix in ('.xlsx', '.xls'):
        df.to_excel(path, index=False)
    else:
        raise ValueError(f"Unsupported file format for saving: {suffix}")

    logger.info(f"Saved {len(df)} rows × {len(df.columns)} columns to {path}")
