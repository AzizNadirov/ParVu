"""
TableTab model — represents one data table tab in a ParVu window.
"""
from __future__ import annotations

import re
from pathlib import Path
from dataclasses import dataclass, field

from parvu.core.query_engine import QueryEngine
from parvu.core.edit_queue import EditQueue


def slugify_name(text: str) -> str:
    """Convert a filename stem into a valid SQL identifier.

    - Lowercase
    - Replace non-alphanumeric with underscore
    - Strip leading/trailing underscores
    - Ensure it doesn't start with a digit
    """
    text = text.lower()
    text = re.sub(r"[^a-z0-9_]+", "_", text)
    text = text.strip("_")
    if text and text[0].isdigit():
        text = "_" + text
    return text or "data"


@dataclass
class TableTab:
    """State container for a single data table tab."""

    name: str
    file_path: Path | None
    engine: QueryEngine
    edit_queue: EditQueue = field(default_factory=EditQueue)
    current_page: int = 1
    sql_query: str = ""
    applied_steps: list[str] = field(default_factory=list)
    sort_column: str | None = None
    sort_ascending: bool = True
    _undo_stack: list[dict] = field(default_factory=list)

    def __post_init__(self) -> None:
        if not self.sql_query:
            self.sql_query = f"SELECT * FROM {self.name}"

    @property
    def is_dirty(self) -> bool:
        return self.edit_queue.is_dirty()
