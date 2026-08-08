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
from parvu.core.exporter import ExportCancelled, ExportOptions, export_dataset


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
    """Background thread streaming the current result to a file.

    ``conn`` must be a dedicated DuckDB cursor (``connection.cursor()``)
    created by the caller — the worker owns it for its lifetime.
    ``source_df`` short-circuits the query when pending cell edits have
    already been materialized on the main thread.
    """

    progress = pyqtSignal(int)  # rows written so far
    done = pyqtSignal(int)  # total rows written
    cancelled = pyqtSignal()
    error = pyqtSignal(str)

    def __init__(
        self,
        conn,
        source_query: str,
        options: ExportOptions,
        source_df=None,
    ):
        super().__init__()
        self._conn = conn
        self._source_query = source_query
        self._options = options
        self._source_df = source_df
        self._cancelled = False

    def cancel(self) -> None:
        self._cancelled = True

    def run(self) -> None:
        try:
            query = self._source_query
            if self._source_df is not None:
                self._conn.register("_parvu_export_src", self._source_df)
                query = "SELECT * FROM _parvu_export_src"
            rows = export_dataset(
                self._conn,
                query,
                self._options,
                on_progress=self.progress.emit,
                is_cancelled=lambda: self._cancelled,
            )
            self.done.emit(rows)
        except ExportCancelled:
            self.cancelled.emit()
        except Exception as e:
            self.error.emit(str(e))
        finally:
            self._source_df = None
            self._conn.close()
            gc.collect()


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
