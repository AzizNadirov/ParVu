"""
Main Application Window for ParVu.

Orchestrates all UI components via the service container.
Supports multiple data-table tabs (Excel-style) with a shared OPSPan.
"""
from __future__ import annotations

from pathlib import Path

from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QFileDialog,
    QMessageBox, QProgressDialog, QApplication, QLabel,
    QTableWidget, QInputDialog,
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
from parvu.presentation.widgets.tab_bar import TabBar
from parvu.presentation.widgets.ops_pan import OPSPan
from parvu.presentation.models.table_tab import TableTab, slugify_name
from parvu.presentation.dialogs.settings_dialog import SettingsDialog
from parvu.presentation.dialogs.theme_selector import ThemeSelectorDialog
from parvu.presentation.dialogs.about_dialog import AboutDialog
from parvu.presentation.dialogs.table_info_dialog import TableInfoDialog
from parvu.presentation.dialogs.unique_values_dialog import UniqueValuesDialog
from parvu.presentation.dialogs.crash_reporter import CrashReportDialog


class MainWindow(QMainWindow, ThemeableMixin):
    """Main application window with multi-tab support."""

    def __init__(self, container: ServiceContainer, file_path: Path | None = None):
        super().__init__()
        self._container = container
        self._t = container.translator
        self._window_service: WindowService | None = None

        # Tab state
        self._tabs: list[TableTab] = []
        self._active_tab_index: int = -1
        self._current_page: int = 1
        self._current_worker_tab: TableTab | None = None

        self._setup_ui()
        self._setup_menu()
        self._apply_theme()

        if file_path:
            self._add_tab(file_path)

        logger.info("MainWindow initialized")

    def set_window_service(self, service: WindowService) -> None:
        """Set the window service for creating new windows."""
        self._window_service = service

    @property
    def is_empty(self) -> bool:
        """Return True if no tabs are open."""
        return not self._tabs

    def _active_tab(self) -> TableTab | None:
        """Return the currently active tab, or None."""
        if 0 <= self._active_tab_index < len(self._tabs):
            return self._tabs[self._active_tab_index]
        return None

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

        # OPSPan — operations panel
        self._ops_pan = OPSPan()
        self._ops_pan.add_column_requested.connect(self._on_op_add_column)
        self._ops_pan.remove_column_requested.connect(self._on_op_remove_column)
        self._ops_pan.change_type_requested.connect(self._on_op_change_type)
        self._ops_pan.math_op_requested.connect(self._on_op_math)
        layout.addWidget(self._ops_pan)

        # Data table
        layout.addWidget(QLabel(self._t("label.results")))
        self._data_table = DataTableView(theme=self._container.theme_manager.current_theme)
        self._data_table.sort_requested.connect(self._on_sort)
        self._data_table.unique_values_requested.connect(self._on_unique_values)
        self._data_table.cell_edited.connect(self._on_cell_edited)
        layout.addWidget(self._data_table)

        # Pagination
        self._pagination = PaginationBar()
        self._pagination.prev_clicked.connect(self._prev_page)
        self._pagination.next_clicked.connect(self._next_page)
        layout.addWidget(self._pagination)

        # Tab bar
        self._tab_bar = TabBar()
        self._tab_bar.tab_switched.connect(self._switch_tab)
        self._tab_bar.tab_closed.connect(self._close_tab)
        self._tab_bar.add_tab_requested.connect(self._browse_file)
        layout.addWidget(self._tab_bar)

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
        if file_path:
            self._add_tab(Path(file_path))

    def _load_from_input(self) -> None:
        path = self._file_toolbar.path
        if path:
            self._add_tab(Path(path))
        else:
            QMessageBox.warning(self, self._t("error.no_file"), self._t("error.no_file_msg"))

    def _add_tab(self, file_path: Path) -> None:
        if not file_path.exists():
            QMessageBox.critical(
                self, self._t("error.file_not_found"),
                self._t("error.file_not_found_msg", path=file_path)
            )
            return

        try:
            name = slugify_name(file_path.stem)
            engine = self._container.create_query_engine(file_path, table_name=name)
            tab = TableTab(name=name, file_path=file_path, engine=engine)

            self._tabs.append(tab)
            self._switch_tab(len(self._tabs) - 1)

            self._container.file_service.add_to_recents(file_path)
            self._update_recents_menu()

            self.statusBar().showMessage(
                self._t("status.loaded", filename=file_path.name, rows=engine.total_rows)
            )
            logger.info(f"Tab added: {name} ← {file_path}")

        except Exception as e:
            QMessageBox.critical(
                self, self._t("error.load_error"),
                self._t("error.load_error_msg", error=str(e))
            )
            logger.error(f"Failed to add tab: {e}")

    def _switch_tab(self, index: int) -> None:
        """Switch to the given tab index, saving/restoring per-tab state."""
        # Save current tab state
        current = self._active_tab()
        if current:
            current.current_page = self._current_page
            current.sql_query = self._sql_editor.get_query()
            self._data_table.set_edit_queue(None)

        self._active_tab_index = index
        new_tab = self._active_tab()
        if not new_tab:
            return

        # Restore new tab state
        self._current_page = new_tab.current_page
        self._data_table.set_edit_queue(new_tab.edit_queue)
        self._sql_editor.set_query(new_tab.sql_query)
        self._sql_editor.update_completions(
            new_tab.engine.get_columns(), new_tab.name
        )
        self._file_toolbar.set_path(str(new_tab.file_path) if new_tab.file_path else "")
        self._query_toolbar.set_enabled(True)

        # Update tab bar visuals
        self._tab_bar.set_tabs([t.name for t in self._tabs])
        self._tab_bar.set_active_index(index)

        # Load data
        self._load_page()

    def _close_tab(self, index: int) -> None:
        """Close the tab at the given index."""
        if not (0 <= index < len(self._tabs)):
            return
        tab = self._tabs[index]
        if tab.is_dirty:
            self._active_tab_index = index  # make it active so user sees which tab
            self._switch_tab(index)
            if not self._confirm_discard_unsaved():
                return
        tab.engine.close()
        self._tabs.pop(index)

        if not self._tabs:
            self._active_tab_index = -1
            self._data_table.setRowCount(0)
            self._data_table.setColumnCount(0)
            self._pagination.update_state(0, 0, 0)
            self._sql_editor.set_query("")
            self._file_toolbar.set_path("")
            self._query_toolbar.set_enabled(False)
            self._tab_bar.set_tabs([])
            self.statusBar().showMessage(self._t("status.ready"))
        else:
            new_index = min(index, len(self._tabs) - 1)
            self._switch_tab(new_index)

    def _load_page(self, query: str | None = None) -> None:
        tab = self._active_tab()
        if not tab:
            return
        self.statusBar().showMessage(self._t("status.loading"))
        self._current_worker_tab = tab
        self._worker = QueryWorker(tab.engine, self._current_page, query)
        self._worker.finished.connect(self._on_page_loaded)
        self._worker.error.connect(self._on_query_error)
        self._worker.start()

    def _on_page_loaded(self, df) -> None:
        tab = self._active_tab()
        if not tab or self._current_worker_tab is not tab:
            return  # stale result from a different tab

        offset = (self._current_page - 1) * tab.engine.page_size
        self._data_table.set_page_offset(offset)
        self._data_table.load_data(df)
        self._pagination.update_state(
            self._current_page, tab.engine.total_pages, tab.engine.total_rows
        )
        if tab.engine.is_base_query:
            self._data_table.setEditTriggers(
                QTableWidget.EditTrigger.DoubleClicked | QTableWidget.EditTrigger.EditKeyPressed
            )
        else:
            self._data_table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        self._update_status_bar()

    def _on_query_error(self, error_msg: str) -> None:
        QMessageBox.critical(
            self, self._t("error.query_error"),
            self._t("error.query_error_msg", error_msg=error_msg)
        )
        self.statusBar().showMessage(self._t("status.query_failed"))
        logger.error(f"Query error: {error_msg}")

    def _execute_query(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        query = self._sql_editor.get_query()
        if not query:
            QMessageBox.warning(self, self._t("error.empty_query"), self._t("error.empty_query_msg"))
            return
        self._current_page = 1
        self._load_page(query)

    def _reset_query(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        tab.engine.reset_query()
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
        tab = self._active_tab()
        if tab and self._current_page < tab.engine.total_pages:
            self._current_page += 1
            self._load_page()

    def _on_sort(self, column: str, ascending: bool) -> None:
        tab = self._active_tab()
        if not tab:
            return
        success, error = tab.engine.sort_by_column(column, ascending)
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
        tab = self._active_tab()
        if not tab:
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
            values = tab.engine.get_unique_values(column)
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
        tab = self._active_tab()
        if not tab:
            return True
        s = self._container.settings

        should_warn = False
        message = ""

        if s.warning_criteria == "rows":
            if tab.engine.total_rows > s.warning_threshold_rows:
                should_warn = True
                message = self._t("warning.large_dataset_rows", rows=tab.engine.total_rows, threshold=s.warning_threshold_rows)
        elif s.warning_criteria == "cells":
            num_cols = len(tab.engine.get_columns())
            total_cells = tab.engine.total_rows * num_cols
            if total_cells > s.warning_threshold_cells:
                should_warn = True
                message = self._t("warning.large_dataset_cells", cells=total_cells, rows=tab.engine.total_rows, columns=num_cols, threshold=s.warning_threshold_cells)
        elif s.warning_criteria == "filesize":
            file_size_mb = tab.engine.file_path.stat().st_size / (1024 * 1024)
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
        tab = self._active_tab()
        if not tab:
            return
        table = tab.name
        if len(values) == 1:
            query = f"SELECT * FROM {table} WHERE {column} = '{values[0]}'"
        else:
            values_str = ", ".join(f"'{v}'" for v in values)
            query = f"SELECT * FROM {table} WHERE {column} IN ({values_str})"
        self._sql_editor.set_query(query)
        self._execute_query()

    def _export_results(self) -> None:
        tab = self._active_tab()
        if not tab:
            QMessageBox.warning(self, self._t("warning.no_data"), self._t("warning.no_data_msg"))
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self, "Export Results", "",
            self._container.file_service.get_export_dialog_filter()
        )
        if file_path:
            try:
                success = tab.engine.export_results(Path(file_path))
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
        tab = self._active_tab()
        if not tab:
            return
        info = tab.engine.get_table_info()
        columns = tab.engine.get_column_types()
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
            action.triggered.connect(lambda checked, path=recent: self._add_tab(Path(path)))
            self._recents_menu.addAction(action)

        if self._container.file_service.get_recent_files():
            self._recents_menu.addSeparator()
            clear = QAction(self._t("menu.file.clear_recents"), self)
            clear.triggered.connect(self._clear_recents)
            self._recents_menu.addAction(clear)

    def _clear_recents(self) -> None:
        self._container.file_service.clear_recents()
        self._update_recents_menu()

    def _on_cell_edited(self, absolute_row: int, column: str, old_value, new_value) -> None:
        self._update_status_bar()

    def _update_status_bar(self) -> None:
        """Update status bar with page info and unsaved changes indicator."""
        tab = self._active_tab()
        if not tab:
            self.statusBar().showMessage(self._t("status.ready"))
            return

        page_info = self._t("status.page_info", page=self._current_page, total_pages=tab.engine.total_pages)
        if tab.edit_queue.is_dirty():
            page_info += f"  |  📝 {tab.edit_queue.edited_cells_count()} unsaved"
        self.statusBar().showMessage(page_info)

    def _save_file(self) -> None:
        """Save edits to the original file."""
        tab = self._active_tab()
        if not tab:
            return
        if not tab.edit_queue.is_dirty():
            return
        if not tab.file_path or not tab.file_path.exists():
            self._save_file_as()
            return
        self._do_save(tab.file_path)

    def _save_file_as(self) -> None:
        """Save edits to a new file (Save As)."""
        tab = self._active_tab()
        if not tab:
            QMessageBox.warning(self, self._t("warning.no_data"), self._t("warning.no_data_msg"))
            return
        file_path, _ = QFileDialog.getSaveFileName(
            self,
            self._t("dialog.save_as"),
            str(tab.file_path) if tab.file_path else "",
            self._container.file_service.get_export_dialog_filter(),
        )
        if file_path:
            self._do_save(Path(file_path))

    def _do_save(self, output_path: Path) -> None:
        """Perform the actual save operation in a background thread."""
        tab = self._active_tab()
        if not tab:
            return
        edits = tab.edit_queue.all_edits()
        if not edits:
            return

        self.statusBar().showMessage(self._t("status.saving"))
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
        tab = self._active_tab()
        if tab:
            tab.edit_queue.clear()
            tab.engine.close()
            tab.engine = self._container.create_query_engine(output_path, table_name=tab.name)
            tab.file_path = output_path
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
        logger.info(f"Saved edits to {output_path}")

    def _on_save_error(self, error_msg: str) -> None:
        """Called when background save fails."""
        self._save_progress.close()
        QMessageBox.critical(
            self,
            self._t("error.save_failed"),
            self._t("error.save_failed_msg", error=error_msg),
        )
        logger.error(f"Save failed: {error_msg}")

    def _confirm_discard_unsaved(self) -> bool:
        """Ask user to confirm discarding unsaved changes. Returns True to proceed."""
        tab = self._active_tab()
        if not tab or not tab.edit_queue.is_dirty():
            return True

        reply = QMessageBox.question(
            self,
            self._t("warning.unsaved_changes"),
            self._t("warning.unsaved_changes_msg", count=tab.edit_queue.edited_cells_count()),
            QMessageBox.StandardButton.Save | QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Save,
        )

        if reply == QMessageBox.StandardButton.Save:
            self._save_file()
            return not tab.edit_queue.is_dirty()
        elif reply == QMessageBox.StandardButton.Discard:
            return True
        else:
            return False

    # ------------------------------------------------------------------
    # OPSPan operations
    # ------------------------------------------------------------------

    def _on_op_add_column(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        name, ok1 = QInputDialog.getText(self, "Add Column", "New column name:")
        if not ok1 or not name:
            return
        expr, ok2 = QInputDialog.getText(self, "Add Column", "SQL expression:")
        if not ok2 or not expr:
            return
        self._apply_transform(tab, f'SELECT *, {expr} AS "{name}" FROM ({tab.engine.current_query})')

    def _on_op_remove_column(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        cols = tab.engine.get_columns()
        if not cols:
            return
        col, ok = QInputDialog.getItem(self, "Remove Column", "Select column:", cols, editable=False)
        if not ok:
            return
        remaining = [c for c in cols if c != col]
        if not remaining:
            QMessageBox.warning(self, "Remove Column", "Cannot remove the only column.")
            return
        select_list = ", ".join(f'"{c}"' for c in remaining)
        self._apply_transform(tab, f'SELECT {select_list} FROM ({tab.engine.current_query})')

    def _on_op_change_type(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        cols = tab.engine.get_columns()
        if not cols:
            return
        col, ok1 = QInputDialog.getItem(self, "Change Type", "Column:", cols, editable=False)
        if not ok1:
            return
        types = ["INTEGER", "BIGINT", "DOUBLE", "VARCHAR", "BOOLEAN", "DATE", "TIMESTAMP"]
        new_type, ok2 = QInputDialog.getItem(self, "Change Type", "New type:", types, editable=False)
        if not ok2:
            return
        select_parts = []
        for c in cols:
            if c == col:
                select_parts.append(f'CAST("{c}" AS {new_type}) AS "{c}"')
            else:
                select_parts.append(f'"{c}"')
        select_list = ", ".join(select_parts)
        self._apply_transform(tab, f'SELECT {select_list} FROM ({tab.engine.current_query})')

    def _on_op_math(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        cols = tab.engine.get_columns()
        if not cols:
            return

        name, ok = QInputDialog.getText(self, "Math Operation", "New column name:")
        if not ok or not name:
            return

        left, ok1 = QInputDialog.getItem(self, "Math Operation", "Left operand (column):", cols, editable=False)
        if not ok1:
            return
        ops = ["+", "-", "*", "/"]
        op, ok2 = QInputDialog.getItem(self, "Math Operation", "Operator:", ops, editable=False)
        if not ok2:
            return
        right, ok3 = QInputDialog.getItem(self, "Math Operation", "Right operand (column):", cols, editable=False)
        if not ok3:
            return

        expr = f'"{left}" {op} "{right}"'
        self._apply_transform(tab, f'SELECT *, {expr} AS "{name}" FROM ({tab.engine.current_query})')

    def _apply_transform(self, tab: TableTab, query: str) -> None:
        """Apply a SQL transformation to the given tab's engine."""
        success, error = tab.engine.execute_query(query)
        if success:
            self._current_page = 1
            self._load_page()
            self.statusBar().showMessage("Transformation applied.", 3000)
            logger.info(f"Applied transform: {query[:80]}...")
        else:
            QMessageBox.critical(self, "Transform Error", f"Failed to apply transformation:\n\n{error}")
            logger.error(f"Transform failed: {error}")

    # ------------------------------------------------------------------

    def closeEvent(self, event) -> None:
        # Check all tabs for unsaved changes
        for tab in self._tabs:
            if tab.edit_queue.is_dirty():
                self._switch_tab(self._tabs.index(tab))
                if not self._confirm_discard_unsaved():
                    event.ignore()
                    return

        for tab in self._tabs:
            tab.engine.close()
        if self._window_service:
            self._window_service.remove_window(self)
        event.accept()
