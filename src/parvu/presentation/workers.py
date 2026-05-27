"""
Background workers for ParVu.

QThread-based workers for non-blocking operations.
"""
from __future__ import annotations

import gc
from pathlib import Path

import pandas as pd
from PyQt6.QtCore import QThread, pyqtSignal

from parvu.core.interfaces import IQueryEngine
from parvu.core.edit_queue import CellEdit, apply_edits_to_dataframe, read_source_file, write_source_file


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


class SaveWorker(QThread):
    """Background thread for saving the active result to disk.

    Two source modes:
    - Legacy file mode (``source_df`` None): reads ``output_path`` directly
      via pandas inside the worker, applies cell edits, writes back. Used
      when only cell edits are pending — no transforms applied.
    - DataFrame mode (``source_df`` provided): the caller has already
      materialized the current engine query on the main thread (DuckDB
      connections are not safe across threads). The worker just applies
      edits and writes.
    """

    finished = pyqtSignal()
    error = pyqtSignal(str)

    def __init__(
        self,
        output_path: Path,
        edits: list[CellEdit],
        source_df=None,
    ):
        super().__init__()
        self._output_path = output_path
        self._edits = edits
        self._source_df = source_df

    def run(self) -> None:
        try:
            if self._source_df is not None:
                df = self._source_df
            else:
                df = read_source_file(self._output_path)
            df = apply_edits_to_dataframe(df, self._edits)
            write_source_file(df, self._output_path)
            del df
            gc.collect()
            self.finished.emit()
        except Exception as e:
            self.error.emit(str(e))


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
