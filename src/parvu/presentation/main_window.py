"""
Main Application Window for ParVu.

Orchestrates all UI components via the service container.
"""
from __future__ import annotations

from pathlib import Path

from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QFileDialog,
    QMessageBox, QProgressDialog, QApplication, QLabel,
    QTableWidget,
)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QIcon, QAction
from loguru import logger

from parvu.services.container import ServiceContainer
from parvu.services.window_service import WindowService
from parvu.services.query_service import QueryService
from parvu.services.file_service import FileService
from parvu.infrastructure.themes.manager import ThemeManager
from parvu.infrastructure.i18n.translator import _Translator
from parvu.core.query_engine import QueryEngine
from parvu.core.edit_queue import EditQueue
from parvu.presentation.themeable import ThemeableMixin
from parvu.presentation.workers import QueryWorker, SaveWorker, UniqueValuesWorker
from parvu.presentation.widgets.file_toolbar import FileToolbar
from parvu.presentation.widgets.query_toolbar import QueryToolbar
from parvu.presentation.widgets.sql_editor import SQLEditor
from parvu.presentation.widgets.data_table import DataTableView
from parvu.presentation.widgets.pagination_bar import PaginationBar
from parvu.presentation.dialogs.settings_dialog import SettingsDialog
from parvu.presentation.dialogs.theme_selector import ThemeSelectorDialog
from parvu.presentation.dialogs.about_dialog import AboutDialog
from parvu.presentation.dialogs.table_info_dialog import TableInfoDialog
from parvu.presentation.dialogs.unique_values_dialog import UniqueValuesDialog
from parvu.presentation.dialogs.crash_reporter import CrashReportDialog


class MainWindow(QMainWindow, ThemeableMixin):
    """Main application window."""

    def __init__(self, container: ServiceContainer, file_path: Path | None = None):
        super().__init__()
        self._container = container
        self._t = container.translator
        self._window_service: WindowService | None = None
        self._engine: QueryEngine | None = None
        self._edit_queue = EditQueue()
        self._current_page = 1

        self._setup_ui()
        self._setup_menu()
        self._apply_theme()

        if file_path:
            self._load_file(file_path)

        logger.info("MainWindow initialized")

    def set_window_service(self, service: WindowService) -> None:
        """Set the window service for creating new windows."""
        self._window_service = service

    @property
    def is_empty(self) -> bool:
        """Return True if no file is currently loaded."""
        return self._engine is None

    def _setup_ui(self) -> None:
        self.setWindowTitle(self._t("app.title"))
        self.setGeometry(100, 100, 1400, 900)

        central = QWidget()
        self.setCentralWidget(central)
        layout = QVBoxLayout(central)

        # File toolbar
        self._file_toolbar = FileToolbar()
        self._file_toolbar.browse_clicked.connect(self._browse_file)
        self._file_toolbar.load_clicked.connect(self._load_from_input)
        layout.addWidget(self._file_toolbar)

        # SQL Editor
        layout.addWidget(QLabel(self._t("label.sql_query", table_name=self._container.settings.default_data_var_name)))
        self._sql_editor = SQLEditor(theme=self._container.theme_manager.current_theme)
        self._sql_editor.setMaximumHeight(100)
        layout.addWidget(self._sql_editor)

        # Query toolbar
        self._query_toolbar = QueryToolbar()
        self._query_toolbar.execute_clicked.connect(self._execute_query)
        self._query_toolbar.reset_clicked.connect(self._reset_query)
        self._query_toolbar.info_clicked.connect(self._show_table_info)
        layout.addWidget(self._query_toolbar)

        # Data table
        layout.addWidget(QLabel(self._t("label.results")))
        self._data_table = DataTableView(theme=self._container.theme_manager.current_theme)
        logger.debug(f"[MAIN] Created DataTableView id={id(self._data_table)}")
        self._data_table.set_edit_queue(self._edit_queue)
        logger.debug(f"[MAIN] Set edit_queue id={id(self._edit_queue)} on DataTableView")
        self._data_table.sort_requested.connect(self._on_sort)
        self._data_table.unique_values_requested.connect(self._on_unique_values)
        self._data_table.cell_edited.connect(self._on_cell_edited)
        layout.addWidget(self._data_table)

        # Pagination
        self._pagination = PaginationBar()
        self._pagination.prev_clicked.connect(self._prev_page)
        self._pagination.next_clicked.connect(self._next_page)
        layout.addWidget(self._pagination)

        # Status bar
        self.statusBar().showMessage(self._t("status.ready"))

    def _setup_menu(self) -> None:
        menubar = self.menuBar()

        file_menu = menubar.addMenu(self._t("menu.file"))

        new_action = QAction(self._t("menu.file.new_window"), self)
        new_action.setShortcut("Ctrl+N")
        new_action.triggered.connect(self._new_window)
        file_menu.addAction(new_action)
        file_menu.addSeparator()

        open_action = QAction(self._t("menu.file.open"), self)
        open_action.setShortcut("Ctrl+O")
        open_action.triggered.connect(self._browse_file)
        file_menu.addAction(open_action)
        file_menu.addSeparator()

        export_action = QAction(self._t("menu.file.export"), self)
        export_action.triggered.connect(self._export_results)
        file_menu.addAction(export_action)
        file_menu.addSeparator()

        save_action = QAction(self._t("menu.file.save"), self)
        save_action.setShortcut("Ctrl+S")
        save_action.triggered.connect(self._save_file)
        file_menu.addAction(save_action)

        save_as_action = QAction(self._t("menu.file.save_as"), self)
        save_as_action.setShortcut("Ctrl+Shift+S")
        save_as_action.triggered.connect(self._save_file_as)
        file_menu.addAction(save_as_action)
        file_menu.addSeparator()

        settings_action = QAction(self._t("menu.file.settings"), self)
        settings_action.triggered.connect(self._edit_settings)
        file_menu.addAction(settings_action)

        self._recents_menu = file_menu.addMenu(self._t("menu.file.recent_files"))
        self._update_recents_menu()
        file_menu.addSeparator()

        exit_action = QAction(self._t("menu.file.exit"), self)
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        help_menu = menubar.addMenu(self._t("menu.help"))
        about_action = QAction(self._t("menu.help.about"), self)
        about_action.triggered.connect(self._show_about)
        help_menu.addAction(about_action)

    def _apply_theme(self) -> None:
        theme = self._container.theme_manager.current_theme
        if not theme:
            return
        stylesheet = self._container.theme_manager.generate_stylesheet(theme)
        self.setStyleSheet(stylesheet)
        self.setMinimumSize(theme.layout.window_min_width, theme.layout.window_min_height)

        self._sql_editor.apply_theme(theme)
        self._data_table.apply_theme(theme)

    def _new_window(self) -> None:
        if self._window_service:
            self._window_service.create_window()

    def _browse_file(self) -> None:
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            self._t("dialog.open_file"),
            "",
            self._container.file_service.get_file_dialog_filter(),
        )
        if not file_path:
            return
        path = Path(file_path)
        if self.is_empty:
            self._load_file(path)
        elif self._window_service:
            self._window_service.create_window(path)
        else:
            self._load_file(path)

    def _load_from_input(self) -> None:
        path = self._file_toolbar.path
        if path:
            self._load_file(Path(path))
        else:
            QMessageBox.warning(self, self._t("error.no_file"), self._t("error.no_file_msg"))

    def _load_file(self, file_path: Path) -> None:
        if not file_path.exists():
            QMessageBox.critical(
                self, self._t("error.file_not_found"),
                self._t("error.file_not_found_msg", path=file_path)
            )
            return

        try:
            if self._engine:
                self._engine.close()

            self._engine = self._container.create_query_engine(file_path)
            self._current_page = 1
            self._edit_queue.clear()
            self._data_table.set_page_offset(0)
            self._file_toolbar.set_path(str(file_path))

            columns = self._engine.get_columns()
            self._sql_editor.update_completions(
                columns, self._container.settings.default_data_var_name
            )

            self._query_toolbar.set_enabled(True)
            self._container.file_service.add_to_recents(file_path)
            self._update_recents_menu()
            self._load_page()

            self.statusBar().showMessage(
                self._t("status.loaded", filename=file_path.name, rows=self._engine.total_rows)
            )
            logger.info(f"File loaded: {file_path}")

        except Exception as e:
            QMessageBox.critical(
                self, self._t("error.load_error"),
                self._t("error.load_error_msg", error=str(e))
            )
            logger.error(f"Failed to load file: {e}")

    def _load_page(self, query: str | None = None) -> None:
        if not self._engine:
            logger.warning("[MAIN] _load_page aborted: no engine")
            return

        self.statusBar().showMessage(self._t("status.loading"))
        logger.debug(f"[MAIN] _load_page page={self._current_page}, query={query!r}")
        self._worker = QueryWorker(self._engine, self._current_page, query)
        self._worker.finished.connect(self._on_page_loaded)
        self._worker.error.connect(self._on_query_error)
        self._worker.start()

    def _on_page_loaded(self, df) -> None:
        offset = (self._current_page - 1) * self._engine.page_size if self._engine else 0
        logger.debug(f"[MAIN] _on_page_loaded rows={len(df)}, offset={offset}, base_query={self._engine.is_base_query if self._engine else None}")
        self._data_table.set_page_offset(offset)
        self._data_table.load_data(df)
        if self._engine:
            self._pagination.update_state(
                self._current_page, self._engine.total_pages, self._engine.total_rows
            )
            # Disable editing when viewing query results/sorts to prevent
            # applying edits to wrong rows in the original file.
            if self._engine.is_base_query:
                self._data_table.setEditTriggers(
                    QTableWidget.EditTrigger.DoubleClicked | QTableWidget.EditTrigger.EditKeyPressed
                )
                logger.debug("[MAIN] Editing enabled (base query)")
            else:
                self._data_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
                logger.debug("[MAIN] Editing disabled (non-base query)")
            self._update_status_bar()

    def _on_query_error(self, error_msg: str) -> None:
        QMessageBox.critical(
            self, self._t("error.query_error"),
            self._t("error.query_error_msg", error_msg=error_msg)
        )
        self.statusBar().showMessage(self._t("status.query_failed"))
        logger.error(f"Query error: {error_msg}")

    def _execute_query(self) -> None:
        if not self._engine:
            return
        query = self._sql_editor.get_query()
        if not query:
            QMessageBox.warning(self, self._t("error.empty_query"), self._t("error.empty_query_msg"))
            return
        self._current_page = 1
        self._load_page(query)

    def _reset_query(self) -> None:
        if not self._engine:
            return
        self._engine.reset_query()
        self._sql_editor.set_query(
            self._container.settings.render_vars(self._container.settings.default_sql_query)
        )
        self._current_page = 1
        self._load_page()

    def _prev_page(self) -> None:
        if self._current_page > 1:
            self._current_page -= 1
            self._load_page()

    def _next_page(self) -> None:
        if self._engine and self._current_page < self._engine.total_pages:
            self._current_page += 1
            self._load_page()

    def _on_sort(self, column: str, ascending: bool) -> None:
        if not self._engine:
            return
        success, error = self._engine.sort_by_column(column, ascending)
        if success:
            self._current_page = 1
            self._load_page()
            direction = "ascending" if ascending else "descending"
            self.statusBar().showMessage(self._t("status.sorted", column=column, direction=direction))
        else:
            QMessageBox.warning(
                self, self._t("error.sort_failed"),
                self._t("error.sort_failed_msg", column=column, error_msg=error)
            )

    def _on_unique_values(self, column: str) -> None:
        if not self._engine:
            return

        if self._container.settings.enable_large_dataset_warning:
            if not self._confirm_large_dataset():
                return

        progress = QProgressDialog(
            self._t("status.calculating"), self._t("btn.cancel"), 0, 0, self
        )
        progress.setWindowModality(Qt.WindowModality.WindowModal)
        progress.show()

        try:
            values = self._engine.get_unique_values(column)
            progress.close()

            if values:
                dialog = UniqueValuesDialog(column, values, self)
                dialog.values_selected.connect(self._on_filter_values)
                dialog.exec()
            else:
                QMessageBox.information(
                    self, self._t("error.no_values"),
                    self._t("error.no_values_msg", column=column)
                )
        except Exception as e:
            progress.close()
            QMessageBox.critical(
                self, self._t("error.unique_values_error"),
                self._t("error.unique_values_error_msg", error=str(e))
            )

    def _confirm_large_dataset(self) -> bool:
        s = self._container.settings
        engine = self._engine
        if not engine:
            return True

        should_warn = False
        message = ""

        if s.warning_criteria == "rows":
            if engine.total_rows > s.warning_threshold_rows:
                should_warn = True
                message = self._t("warning.large_dataset_rows", rows=engine.total_rows, threshold=s.warning_threshold_rows)
        elif s.warning_criteria == "cells":
            num_cols = len(engine.get_columns())
            total_cells = engine.total_rows * num_cols
            if total_cells > s.warning_threshold_cells:
                should_warn = True
                message = self._t("warning.large_dataset_cells", cells=total_cells, rows=engine.total_rows, columns=num_cols, threshold=s.warning_threshold_cells)
        elif s.warning_criteria == "filesize":
            file_size_mb = engine.file_path.stat().st_size / (1024 * 1024)
            if file_size_mb > s.warning_threshold_filesize_mb:
                should_warn = True
                message = self._t("warning.large_dataset_size", size=file_size_mb, threshold=s.warning_threshold_filesize_mb)

        if should_warn:
            reply = QMessageBox.question(
                self, self._t("warning.large_dataset"), message,
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
            )
            return reply == QMessageBox.StandardButton.Yes
        return True

    def _on_filter_values(self, column: str, values: list) -> None:
        table = self._container.settings.default_data_var_name
        if len(values) == 1:
            query = f"SELECT * FROM {table} WHERE {column} = '{values[0]}'"
        else:
            values_str = ", ".join(f"'{v}'" for v in values)
            query = f"SELECT * FROM {table} WHERE {column} IN ({values_str})"
        self._sql_editor.set_query(query)
        self._execute_query()

    def _export_results(self) -> None:
        if not self._engine:
            QMessageBox.warning(self, self._t("warning.no_data"), self._t("warning.no_data_msg"))
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self, "Export Results", "",
            self._container.file_service.get_export_dialog_filter()
        )
        if file_path:
            try:
                success = self._engine.export_results(Path(file_path))
                if success:
                    QMessageBox.information(
                        self, self._t("success.export_complete"),
                        self._t("success.export_complete_msg", path=file_path)
                    )
                else:
                    QMessageBox.critical(
                        self, self._t("success.export_failed"),
                        self._t("success.export_failed_msg")
                    )
            except Exception as e:
                QMessageBox.critical(
                    self, self._t("error.export_error"),
                    self._t("error.export_error_msg", error=str(e))
                )

    def _show_table_info(self) -> None:
        if not self._engine:
            return
        info = self._engine.get_table_info()
        columns = self._engine.get_column_types()
        dialog = TableInfoDialog(info, columns, self)
        dialog.exec()

    def _edit_settings(self) -> None:
        dialog = SettingsDialog(
            settings=self._container.settings,
            settings_manager=self._container.settings_manager,
            theme_manager=self._container.theme_manager,
            i18n=self._container.i18n,
            current_theme_name=self._container.theme_manager.current_theme.name if self._container.theme_manager.current_theme else None,
            parent=self,
        )
        dialog.theme_changed.connect(self._change_theme)
        dialog.settings_changed.connect(self._on_settings_changed)
        dialog.exec()

    def _change_theme(self, theme_name: str) -> None:
        if self._container.theme_manager.set_theme(theme_name):
            self._apply_theme()
            self._container.settings.current_theme = theme_name
            self._container.save_settings()
            self.statusBar().showMessage(f"Theme changed to: {theme_name}", 3000)
            logger.info(f"Theme changed to: {theme_name}")

    def _on_settings_changed(self) -> None:
        self.statusBar().showMessage("Settings saved. Some changes may require restart.", 5000)

    def _show_about(self) -> None:
        dialog = AboutDialog(self._container.translator, self)
        dialog.exec()

    def _update_recents_menu(self) -> None:
        self._recents_menu.clear()
        for recent in self._container.file_service.get_recent_files():
            action = QAction(recent, self)
            action.triggered.connect(lambda checked, path=recent: self._load_recent(path))
            self._recents_menu.addAction(action)

        if self._container.file_service.get_recent_files():
            self._recents_menu.addSeparator()
            clear = QAction(self._t("menu.file.clear_recents"), self)
            clear.triggered.connect(self._clear_recents)
            self._recents_menu.addAction(clear)

    def _load_recent(self, file_path: str) -> None:
        path = Path(file_path)
        if not path.exists():
            reply = QMessageBox.question(
                self, self._t("error.file_not_found"),
                self._t("warning.file_not_found_recent", path=file_path),
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
            )
            if reply == QMessageBox.StandardButton.Yes:
                self._container.file_service.remove_from_recents(file_path)
                self._update_recents_menu()
            return
        if self.is_empty:
            self._load_file(path)
        elif self._window_service:
            self._window_service.create_window(path)
        else:
            self._load_file(path)

    def _clear_recents(self) -> None:
        self._container.file_service.clear_recents()
        self._update_recents_menu()

    def _on_cell_edited(self, absolute_row: int, column: str, old_value, new_value) -> None:
        logger.debug(f"[MAIN] _on_cell_edited row={absolute_row}, col={column}, old={old_value!r}, new={new_value!r}")
        self._update_status_bar()

    def _update_status_bar(self) -> None:
        """Update status bar with page info and unsaved changes indicator."""
        if not self._engine:
            self.statusBar().showMessage(self._t("status.ready"))
            return

        page_info = self._t("status.page_info", page=self._current_page, total_pages=self._engine.total_pages)
        if self._edit_queue.is_dirty():
            page_info += f"  |  📝 {self._edit_queue.edited_cells_count()} unsaved"
        logger.debug(f"[MAIN] _update_status_bar: {page_info}")
        self.statusBar().showMessage(page_info)

    def _save_file(self) -> None:
        """Save edits to the original file."""
        logger.debug(f"[MAIN] _save_file called, engine={self._engine!r}, dirty={self._edit_queue.is_dirty()}, edits={self._edit_queue.edit_count()}")
        if not self._engine:
            logger.warning("[MAIN] Save aborted: no engine")
            return
        if not self._edit_queue.is_dirty():
            logger.info("[MAIN] Save skipped: no unsaved changes")
            return

        if not self._engine.file_path.exists():
            self._save_file_as()
            return

        self._do_save(self._engine.file_path)

    def _save_file_as(self) -> None:
        """Save edits to a new file (Save As)."""
        if not self._engine:
            QMessageBox.warning(self, self._t("warning.no_data"), self._t("warning.no_data_msg"))
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self,
            self._t("dialog.save_as"),
            str(self._engine.file_path),
            self._container.file_service.get_export_dialog_filter(),
        )
        if file_path:
            self._do_save(Path(file_path))

    def _do_save(self, output_path: Path) -> None:
        """Perform the actual save operation in a background thread."""
        logger.debug(f"[MAIN] _do_save called for {output_path}")

        edits = self._edit_queue.all_edits()
        if not edits:
            return

        self.statusBar().showMessage(self._t("status.saving"))

        # Modal progress dialog — blocks interaction while saving
        self._save_progress = QProgressDialog(
            self._t("status.saving"), None, 0, 0, self
        )
        self._save_progress.setWindowModality(Qt.WindowModality.WindowModal)
        self._save_progress.setCancelButton(None)
        self._save_progress.show()

        self._save_worker = SaveWorker(output_path, edits)
        self._save_worker.finished.connect(lambda: self._on_save_finished(output_path))
        self._save_worker.error.connect(self._on_save_error)
        self._save_worker.start()

    def _on_save_finished(self, output_path: Path) -> None:
        """Called when background save completes successfully."""
        self._save_progress.close()
        self._edit_queue.clear()

        # Recreate engine so DuckDB picks up the updated file.
        if self._engine:
            self._engine.close()
        self._engine = self._container.create_query_engine(output_path)
        self._current_page = 1
        self._data_table.set_page_offset(0)
        self._file_toolbar.set_path(str(output_path))
        self._load_page()
        self._update_status_bar()

        QMessageBox.information(
            self,
            self._t("success.save_complete"),
            self._t("success.save_complete_msg", path=output_path),
        )
        logger.info(f"[MAIN] Saved edits to {output_path}")

    def _on_save_error(self, error_msg: str) -> None:
        """Called when background save fails."""
        self._save_progress.close()
        QMessageBox.critical(
            self,
            self._t("error.save_failed"),
            self._t("error.save_failed_msg", error=error_msg),
        )
        logger.error(f"[MAIN] Save failed: {error_msg}")

    def _confirm_discard_unsaved(self) -> bool:
        """Ask user to confirm discarding unsaved changes. Returns True to proceed."""
        if not self._edit_queue.is_dirty():
            return True

        reply = QMessageBox.question(
            self,
            self._t("warning.unsaved_changes"),
            self._t("warning.unsaved_changes_msg", count=self._edit_queue.edited_cells_count()),
            QMessageBox.StandardButton.Save | QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Save,
        )

        if reply == QMessageBox.StandardButton.Save:
            self._save_file()
            return not self._edit_queue.is_dirty()  # Only proceed if save succeeded
        elif reply == QMessageBox.StandardButton.Discard:
            return True
        else:
            return False

    def closeEvent(self, event) -> None:
        if self._edit_queue.is_dirty():
            if not self._confirm_discard_unsaved():
                event.ignore()
                return

        if self._engine:
            self._engine.close()
        if self._window_service:
            self._window_service.remove_window(self)
        event.accept()
