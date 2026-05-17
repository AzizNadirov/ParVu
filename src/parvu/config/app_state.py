"""
Application state container for ParVu.

Holds runtime state that doesn't need to be persisted.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import pandas as pd

from parvu.core.models import Page


@dataclass
class AppState:
    """Mutable application runtime state."""

    # Current file
    current_file: Path | None = None

    # Current query state
    current_page: int = 1
    current_query: str = ""

    # Current data page (cached)
    current_data: pd.DataFrame = field(default_factory=pd.DataFrame)

    # UI state
    is_loading: bool = False
    status_message: str = "Ready"

    # Theme
    current_theme_name: str = "ParVu Light"

    # Session
    session_id: str = ""
    log_file: Path | None = None

    def reset(self) -> None:
        """Reset state to defaults."""
        self.current_file = None
        self.current_page = 1
        self.current_query = ""
        self.current_data = pd.DataFrame()
        self.is_loading = False
        self.status_message = "Ready"
