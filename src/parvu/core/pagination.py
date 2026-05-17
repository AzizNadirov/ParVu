"""
Pagination logic for ParVu.

Pure business logic with no external dependencies.
"""
from __future__ import annotations

from parvu.core.interfaces import IPaginator


class Paginator(IPaginator):
    """Handles pagination calculations."""

    def __init__(self, total_rows: int, page_size: int):
        if page_size <= 0:
            raise ValueError("page_size must be positive")
        self._total_rows = total_rows
        self._page_size = page_size
        self._total_pages = max(1, (total_rows + page_size - 1) // page_size)

    @property
    def total_rows(self) -> int:
        return self._total_rows

    @property
    def total_pages(self) -> int:
        return self._total_pages

    @property
    def page_size(self) -> int:
        return self._page_size

    def page_offset(self, page_num: int) -> int:
        """Calculate OFFSET for a given 1-indexed page number."""
        return (self.clamp_page(page_num) - 1) * self._page_size

    def clamp_page(self, page_num: int) -> int:
        """Clamp page number to valid range [1, total_pages]."""
        return max(1, min(page_num, self._total_pages))

    def recalculate(self, total_rows: int) -> None:
        """Recalculate pagination after query changes."""
        self._total_rows = total_rows
        self._total_pages = max(1, (total_rows + self._page_size - 1) // self._page_size)

    def __repr__(self) -> str:
        return f"Paginator(rows={self._total_rows}, size={self._page_size}, pages={self._total_pages})"
