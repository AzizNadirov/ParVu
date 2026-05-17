"""
Background workers for ParVu.

QThread-based workers for non-blocking operations.
"""
from __future__ import annotations

import pandas as pd
from PyQt6.QtCore import QThread, pyqtSignal

from parvu.core.interfaces import IQueryEngine


class QueryWorker(QThread):
    """Background thread for executing queries and loading pages."""

    finished = pyqtSignal(pd.DataFrame)
    error = pyqtSignal(str)

    def __init__(
        self,
        engine: IQueryEngine,
        page_num: int,
        query: str | None = None,
    ):
        super().__init__()
        self._engine = engine
        self._page_num = page_num
        self._query = query

    def run(self) -> None:
        """Execute query or load page in background."""
        try:
            if self._query:
                success, error_msg = self._engine.execute_query(self._query)
                if not success:
                    self.error.emit(f"Query Error:\n\n{error_msg}")
                    return

            df = self._engine.get_page(self._page_num)
            self.finished.emit(df)
        except Exception as e:
            self.error.emit(f"Error: {str(e)}")


class ExportWorker(QThread):
    """Background thread for exporting data."""

    finished = pyqtSignal(bool, str)

    def __init__(self, engine: IQueryEngine, output_path: str):
        super().__init__()
        self._engine = engine
        self._output_path = output_path

    def run(self) -> None:
        """Export data in background."""
        try:
            success = self._engine.export_results(self._output_path)
            self.finished.emit(success, "")
        except Exception as e:
            self.finished.emit(False, str(e))


class UniqueValuesWorker(QThread):
    """Background thread for calculating unique values."""

    finished = pyqtSignal(list)
    error = pyqtSignal(str)

    def __init__(self, engine: IQueryEngine, column: str):
        super().__init__()
        self._engine = engine
        self._column = column

    def run(self) -> None:
        """Calculate unique values in background."""
        try:
            values = self._engine.get_unique_values(self._column)
            self.finished.emit(values)
        except Exception as e:
            self.error.emit(str(e))
