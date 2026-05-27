"""
Main Application Window for ParVu.

Orchestrates all UI components via the service container.
Supports multiple data-table tabs (Excel-style).
"""
from __future__ import annotations

import duckdb
from pathlib import Path

from PyQt6.QtWidgets import (
    QMainWindow, QWidget, QVBoxLayout, QFileDialog,
    QMessageBox, QProgressDialog, QApplication, QLabel,
    QTableWidget, QInputDialog, QDialog, QStyle,
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
from parvu.presentation.widgets.query_editor import QueryEditor
from parvu.presentation.widgets.data_table import DataTableView
from parvu.presentation.widgets.pagination_bar import PaginationBar
from parvu.presentation.widgets.tab_bar import TabBar
from parvu.presentation.models.table_tab import TableTab, slugify_name
from parvu.presentation.dialogs.settings_dialog import SettingsDialog
from parvu.presentation.dialogs.theme_selector import ThemeSelectorDialog
from parvu.presentation.dialogs.about_dialog import AboutDialog
from parvu.presentation.dialogs.table_info_dialog import TableInfoDialog
from parvu.presentation.dialogs.unique_values_dialog import UniqueValuesDialog
from parvu.presentation.dialogs.crash_reporter import CrashReportDialog
from parvu.presentation.dialogs.expression_dialog import ExpressionDialog
from parvu.presentation.dialogs.join_dialog import JoinDialog
from parvu.presentation.dialogs.append_dialog import AppendDialog
from parvu.presentation.dialogs.drop_duplicates_dialog import DropDuplicatesDialog
from parvu.presentation.dialogs.replace_dialog import ReplaceDialog
from parvu.presentation.dialogs.confirm_close_dialog import ConfirmCloseDialog
from parvu.presentation.dialogs.copy_tuple_dialog import CopyTupleDialog
from parvu.presentation.widgets.applied_steps import AppliedStepsPanel


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
        self._pending_step: str | None = None

        # Shared DuckDB connection for multi-table operations
        self._shared_conn = duckdb.connect(":memory:")

        self._setup_ui()
        self._apply_theme()
        self._setup_menu()

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

        # Query Editor
        self._query_label = QLabel(self._t("label.sql_query", table_name=self._container.settings.default_data_var_name))
        layout.addWidget(self._query_label)
        self._query_editor = QueryEditor(
            theme=self._container.theme_manager.current_theme,
        )
        self._query_editor.setMaximumHeight(140)
        self._query_editor.mode_changed.connect(self._on_query_mode_changed)
        layout.addWidget(self._query_editor)

        # Query toolbar
        self._query_toolbar = QueryToolbar()
        self._query_toolbar.execute_clicked.connect(self._execute_query)
        self._query_toolbar.reset_clicked.connect(self._reset_query)
        self._query_toolbar.info_clicked.connect(self._show_table_info)
        layout.addWidget(self._query_toolbar)

        # Applied Steps
        self._steps_panel = AppliedStepsPanel()
        self._steps_panel.undo_requested.connect(self._undo_last_step)
        layout.addWidget(self._steps_panel)

        # Data table
        self._data_table = DataTableView(theme=self._container.theme_manager.current_theme)
        self._data_table.sort_requested.connect(self._on_sort)
        self._data_table.unique_values_requested.connect(self._on_unique_values)
        self._data_table.cell_edited.connect(self._on_cell_edited)
        self._data_table.column_renamed.connect(self._on_column_renamed)
        self._data_table.column_removed.connect(self._on_column_removed)
        self._data_table.column_type_changed.connect(self._on_column_type_changed)
        self._data_table.column_duplicated.connect(self._on_column_duplicated)
        self._data_table.replace_values_requested.connect(self._on_column_replace_values)
        self._data_table.copy_column_tuple_requested.connect(self._on_copy_column_tuple)
        layout.addWidget(self._data_table)

        # Pagination
        self._pagination = PaginationBar()
        self._pagination.prev_clicked.connect(self._prev_page)
        self._pagination.next_clicked.connect(self._next_page)
        layout.addWidget(self._pagination)

        # Tab bar
        self._tab_bar = TabBar(theme=self._container.theme_manager.current_theme)
        self._tab_bar.tab_switched.connect(self._switch_tab)
        self._tab_bar.tab_closed.connect(self._close_tab)
        self._tab_bar.add_tab_requested.connect(self._browse_file)
        layout.addWidget(self._tab_bar)

        # Status bar
        self.statusBar().showMessage(self._t("status.ready"))

    def _setup_menu(self) -> None:
        menubar = self.menuBar()
        style = self.style()

        # ── File Menu ──
        file_menu = menubar.addMenu(self._t("menu.file"))

        new_action = QAction(self._t("menu.file.new_window"), self)
        new_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileDialogNewFolder))
        new_action.setShortcut("Ctrl+N")
        new_action.triggered.connect(self._new_window)
        file_menu.addAction(new_action)
        file_menu.addSeparator()

        open_action = QAction(self._t("menu.file.open"), self)
        open_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogOpenButton))
        open_action.setShortcut("Ctrl+O")
        open_action.triggered.connect(self._browse_file)
        file_menu.addAction(open_action)
        file_menu.addSeparator()

        export_action = QAction(self._t("menu.file.export"), self)
        export_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_ArrowDown))
        export_action.triggered.connect(self._export_results)
        file_menu.addAction(export_action)
        file_menu.addSeparator()

        save_action = QAction(self._t("menu.file.save"), self)
        save_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogSaveButton))
        save_action.setShortcut("Ctrl+S")
        save_action.triggered.connect(self._save_file)
        file_menu.addAction(save_action)

        save_as_action = QAction(self._t("menu.file.save_as"), self)
        save_as_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogSaveButton))
        save_as_action.setShortcut("Ctrl+Shift+S")
        save_as_action.triggered.connect(self._save_file_as)
        file_menu.addAction(save_as_action)
        file_menu.addSeparator()

        settings_action = QAction(self._t("menu.file.settings"), self)
        settings_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileDialogDetailedView))
        settings_action.triggered.connect(self._edit_settings)
        file_menu.addAction(settings_action)

        self._recents_menu = file_menu.addMenu(self._t("menu.file.recent_files"))
        self._recents_menu.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DirHomeIcon))
        self._update_recents_menu()
        file_menu.addSeparator()

        exit_action = QAction(self._t("menu.file.exit"), self)
        exit_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogCloseButton))
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        # ── Operations Menu ──
        operations_menu = menubar.addMenu(self._t("menu.operations"))

        math_action = QAction(self._t("menu.operations.math"), self)
        math_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_ComputerIcon))
        math_action.triggered.connect(self._on_op_math)
        operations_menu.addAction(math_action)

        join_action = QAction(self._t("menu.operations.join"), self)
        join_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DirLinkIcon))
        join_action.triggered.connect(self._on_op_join)
        operations_menu.addAction(join_action)

        append_action = QAction(self._t("menu.operations.append"), self)
        append_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_ArrowDown))
        append_action.triggered.connect(self._on_op_append)
        operations_menu.addAction(append_action)

        drop_dup_action = QAction("Drop Duplicates", self)
        drop_dup_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_BrowserStop))
        drop_dup_action.triggered.connect(self._on_op_drop_duplicates)
        operations_menu.addAction(drop_dup_action)

        replace_action = QAction(self._t("menu.operations.replace"), self)
        replace_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogResetButton))
        replace_action.triggered.connect(self._on_op_replace)
        operations_menu.addAction(replace_action)

        # ── Help Menu ──
        help_menu = menubar.addMenu(self._t("menu.help"))

        about_action = QAction(self._t("menu.help.about"), self)
        about_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_MessageBoxInformation))
        about_action.triggered.connect(self._show_about)
        help_menu.addAction(about_action)

    def _apply_theme(self) -> None:
        theme = self._container.theme_manager.current_theme
        if not theme:
            return
        stylesheet = self._container.theme_manager.generate_stylesheet(theme)
        self.setStyleSheet(stylesheet)
        self.setMinimumSize(theme.layout.window_min_width, theme.layout.window_min_height)

        self._query_editor.apply_theme(theme)
        self._data_table.apply_theme(theme)
        self._tab_bar.set_theme(theme)

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
            logger.info(f"Browse selected file: {file_path}")
            self._add_tab(Path(file_path))

    def _load_from_input(self) -> None:
        path = self._file_toolbar.path
        if path:
            logger.info(f"Load from input path: {path}")
            self._add_tab(Path(path))
        else:
            logger.warning("Load from input: empty path")
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

            # Deduplicate tab/view name if the same file is opened again
            existing_names = {t.name for t in self._tabs}
            if name in existing_names:
                counter = 2
                while f"{name}_{counter}" in existing_names:
                    counter += 1
                name = f"{name}_{counter}"

            # Register file as a named view in the shared connection
            adapter = self._container.file_adapter_registry.get_adapter(file_path)
            reader_sql = adapter.build_reader_query(file_path)
            self._shared_conn.execute(f"CREATE OR REPLACE VIEW {name} AS {reader_sql}")

            engine = self._container.create_query_engine(
                file_path, table_name=name, conn=self._shared_conn
            )
            tab = TableTab(name=name, file_path=file_path, engine=engine)

            self._tabs.append(tab)
            self._switch_tab(len(self._tabs) - 1)
            self._file_toolbar.setVisible(False)

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
        current = self._active_tab()
        if current:
            logger.debug(f"Switching from tab '{current.name}' (page {self._current_page})")
            current.current_page = self._current_page
            current.sql_query = self._query_editor.get_query()
            self._data_table.set_edit_queue(None)

        self._active_tab_index = index
        new_tab = self._active_tab()
        if not new_tab:
            return

        logger.info(f"Switched to tab '{new_tab.name}' ({new_tab.engine.total_rows} rows)")

        # Restore new tab state
        self._current_page = new_tab.current_page
        self._data_table.set_edit_queue(new_tab.edit_queue)
        self._query_editor.set_query(new_tab.sql_query)
        self._query_editor.update_completions(
            new_tab.engine.get_columns(), new_tab.name
        )
        # Update expression catalog for autocomplete
        from parvu.core.dsl.catalog import Catalog
        catalog = Catalog()
        catalog.register_tab(new_tab.name, new_tab.engine)
        self._query_editor.set_expression_catalog(catalog)
        self._on_query_mode_changed(self._query_editor.is_expression_mode)
        self._file_toolbar.set_path(str(new_tab.file_path) if new_tab.file_path else "")
        self._query_toolbar.set_enabled(True)
        self._steps_panel.set_steps(new_tab.applied_steps)

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
        logger.info(f"Closing tab '{tab.name}' (dirty={tab.is_dirty})")
        if tab.is_dirty:
            self._active_tab_index = index
            self._switch_tab(index)
            if not self._confirm_discard_unsaved():
                logger.info(f"Close tab '{tab.name}' cancelled by user")
                return
        self._shared_conn.execute(f"DROP VIEW IF EXISTS {tab.name}")
        tab.engine.close()
        self._tabs.pop(index)

        if not self._tabs:
            self._active_tab_index = -1
            self._data_table.setRowCount(0)
            self._data_table.setColumnCount(0)
            self._pagination.update_state(0, 0, 0)
            self._query_editor.set_query("")
            from parvu.core.dsl.catalog import Catalog
            self._query_editor.set_expression_catalog(Catalog())
            self._file_toolbar.set_path("")
            self._file_toolbar.setVisible(True)
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
        logger.debug(f"Loading page {self._current_page} for tab '{tab.name}'" + (f" with query: {query[:60]}..." if query else ""))
        self.statusBar().showMessage(self._t("status.loading"))
        self._current_worker_tab = tab
        self._worker = QueryWorker(tab.engine, self._current_page, query)
        self._worker.finished.connect(self._on_page_loaded)
        self._worker.error.connect(self._on_query_error)
        self._worker.start()

    def _on_page_loaded(self, df) -> None:
        tab = self._active_tab()
        if not tab or self._current_worker_tab is not tab:
            logger.debug("Stale page load result ignored")
            return

        offset = (self._current_page - 1) * tab.engine.page_size
        self._data_table.set_page_offset(offset)
        self._data_table.load_data(df)
        self._pagination.update_state(
            self._current_page, tab.engine.total_pages, tab.engine.total_rows
        )
        is_base = tab.engine.is_base_query
        self._data_table.setEditTriggers(
            QTableWidget.EditTrigger.DoubleClicked | QTableWidget.EditTrigger.EditKeyPressed
            if is_base else QTableWidget.EditTrigger.NoEditTriggers
        )

        # Record applied step for expression-mode assignments
        if self._pending_step:
            tab.applied_steps.append(self._pending_step)
            tab._undo_stack.append({"type": "sql"})
            self._steps_panel.set_steps(tab.applied_steps)
            self.statusBar().showMessage(f"{self._pending_step} applied.", 3000)
            self._pending_step = None

        logger.debug(f"Page {self._current_page} loaded: {len(df)} rows, base_query={is_base}")
        self._update_status_bar()

    def _on_query_error(self, error_msg: str) -> None:
        self._pending_step = None
        QMessageBox.critical(
            self, self._t("error.query_error"),
            self._t("error.query_error_msg", error_msg=error_msg)
        )
        self.statusBar().showMessage(self._t("status.query_failed"))
        logger.error(f"Query error: {error_msg}")

    def _on_query_mode_changed(self, is_expression: bool) -> None:
        """Update the query label when the editor mode changes."""
        tab = self._active_tab()
        table_name = tab.name if tab else self._container.settings.default_data_var_name
        if is_expression:
            self._query_label.setText(self._t("label.expression_query", table_name=table_name))
        else:
            self._query_label.setText(self._t("label.sql_query", table_name=table_name))

    def _execute_query(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        query = self._query_editor.get_query()
        if not query:
            QMessageBox.warning(self, self._t("error.empty_query"), self._t("error.empty_query_msg"))
            return

        # Detect expression-mode assignments so we can record an applied step
        self._pending_step = None
        if self._query_editor.is_expression_mode:
            expr_info = self._query_editor.get_expression_info()
            if expr_info.get("type") == "assignment":
                self._pending_step = f'Add column "{expr_info["column"]}"'

        logger.info(f"Executing query on '{tab.name}': {query[:80]}...")
        self._current_page = 1
        self._load_page(query)

    def _reset_query(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.info(f"Resetting query for tab '{tab.name}'")
        tab.engine.reset_query()
        tab.applied_steps.clear()
        tab._undo_stack.clear()
        self._steps_panel.clear()
        self._query_editor.set_query(
            self._container.settings.render_vars(self._container.settings.default_sql_query)
        )
        self._current_page = 1
        self._load_page()

    def _prev_page(self) -> None:
        if self._current_page > 1:
            self._current_page -= 1
            logger.debug(f"Previous page: {self._current_page}")
            self._load_page()

    def _next_page(self) -> None:
        tab = self._active_tab()
        if tab and self._current_page < tab.engine.total_pages:
            self._current_page += 1
            logger.debug(f"Next page: {self._current_page}/{tab.engine.total_pages}")
            self._load_page()

    def _on_sort(self, column: str, ascending: bool) -> None:
        tab = self._active_tab()
        if not tab:
            return
        direction = "ascending" if ascending else "descending"
        logger.info(f"Sorting '{tab.name}' by '{column}' {direction}")
        success, error = tab.engine.sort_by_column(column, ascending)
        if success:
            self._current_page = 1
            self._load_page()
            step = f'Sort "{column}" {direction}'
            tab.applied_steps.append(step)
            tab._undo_stack.append({"type": "sql"})
            self._steps_panel.set_steps(tab.applied_steps)
            self.statusBar().showMessage(self._t("status.sorted", column=column, direction=direction))
        else:
            logger.error(f"Sort failed on '{tab.name}': {error}")
            QMessageBox.warning(
                self, self._t("error.sort_failed"),
                self._t("error.sort_failed_msg", column=column, error_msg=error)
            )

    def _on_unique_values(self, column: str) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.info(f"Getting unique values for '{tab.name}.{column}'")

        if self._container.settings.enable_large_dataset_warning:
            if not self._confirm_large_dataset():
                logger.debug("Unique values cancelled: large dataset warning")
                return

        progress = QProgressDialog(
            self._t("status.calculating"), self._t("btn.cancel"), 0, 0, self
        )
        progress.setWindowModality(Qt.WindowModality.WindowModal)
        progress.show()

        try:
            values = tab.engine.get_unique_values(column)
            progress.close()
            logger.info(f"Unique values for '{tab.name}.{column}': {len(values)} values")

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
            logger.error(f"Unique values error for '{tab.name}.{column}': {e}")
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
        logger.info(f"Filter values on '{table}.{column}': {len(values)} values selected")
        self._query_editor.set_query(query)
        self._execute_query()

    def _export_results(self) -> bool:
        tab = self._active_tab()
        if not tab:
            QMessageBox.warning(self, self._t("warning.no_data"), self._t("warning.no_data_msg"))
            return False

        file_path, _ = QFileDialog.getSaveFileName(
            self, "Export Results", "",
            self._container.file_service.get_export_dialog_filter()
        )
        if not file_path:
            return False
        logger.info(f"Exporting results from '{tab.name}' to {file_path}")
        try:
            success = tab.engine.export_results(Path(file_path))
            if success:
                logger.info(f"Export complete: {file_path}")
                QMessageBox.information(
                    self, self._t("success.export_complete"),
                    self._t("success.export_complete_msg", path=file_path)
                )
                return True
            else:
                logger.error(f"Export failed: {file_path}")
                QMessageBox.critical(
                    self, self._t("success.export_failed"),
                    self._t("success.export_failed_msg")
                )
                return False
        except Exception as e:
            logger.error(f"Export error: {e}")
            QMessageBox.critical(
                self, self._t("error.export_error"),
                self._t("error.export_error_msg", error=str(e))
            )
            return False

    def _show_table_info(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.debug(f"Showing table info for '{tab.name}'")
        info = tab.engine.get_table_info()
        columns = tab.engine.get_column_types()
        dialog = TableInfoDialog(info, columns, self)
        dialog.exec()

    def _edit_settings(self) -> None:
        logger.debug("Opening settings dialog")
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
        logger.debug("Opening about dialog")
        dialog = AboutDialog(self._container.translator, self)
        dialog.exec()

    def _update_recents_menu(self) -> None:
        self._recents_menu.clear()
        file_icon = self.style().standardIcon(QStyle.StandardPixmap.SP_FileIcon)
        for recent in self._container.file_service.get_recent_files():
            action = QAction(file_icon, recent, self)
            action.triggered.connect(lambda checked, path=recent: self._add_tab(Path(path)))
            self._recents_menu.addAction(action)

        if self._container.file_service.get_recent_files():
            self._recents_menu.addSeparator()
            clear = QAction(
                self.style().standardIcon(QStyle.StandardPixmap.SP_DialogDiscardButton),
                self._t("menu.file.clear_recents"), self,
            )
            clear.triggered.connect(self._clear_recents)
            self._recents_menu.addAction(clear)

    def _clear_recents(self) -> None:
        logger.info("Clearing recent files")
        self._container.file_service.clear_recents()
        self._update_recents_menu()

    def _on_cell_edited(self, absolute_row: int, column: str, old_value, new_value) -> None:
        tab = self._active_tab()
        if tab:
            logger.debug(f"Cell edited: {tab.name}[row={absolute_row}, col={column}] '{old_value}' -> '{new_value}'")
            step = self._t("step.cell_edit", column=column, row=absolute_row + 1)
            tab.applied_steps.append(step)
            tab._undo_stack.append({"type": "cell_edit", "row": absolute_row, "column": column})
            self._steps_panel.set_steps(tab.applied_steps)
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
            logger.debug("Save file: no unsaved changes")
            return
        if not tab.file_path or not tab.file_path.exists():
            self._save_file_as()
            return
        logger.info(f"Saving edits to original file: {tab.file_path}")
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
            logger.info(f"Save As selected: {file_path}")
            self._do_save(Path(file_path))

    def _do_save(self, output_path: Path) -> None:
        """Perform the actual save operation in a background thread."""
        tab = self._active_tab()
        if not tab:
            return
        edits = tab.edit_queue.all_edits()
        if not edits:
            logger.debug("Do save: no edits to save")
            return

        logger.info(f"Starting background save ({len(edits)} edits) -> {output_path}")
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
            tab.applied_steps.clear()
            tab._undo_stack.clear()
            self._steps_panel.clear()

            # Recreate the view pointing to the newly saved file
            self._shared_conn.execute(f"DROP VIEW IF EXISTS {tab.name}")
            adapter = self._container.file_adapter_registry.get_adapter(output_path)
            reader_sql = adapter.build_reader_query(output_path)
            self._shared_conn.execute(f"CREATE OR REPLACE VIEW {tab.name} AS {reader_sql}")

            tab.engine = self._container.create_query_engine(
                output_path, table_name=tab.name, conn=self._shared_conn
            )
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

        count = tab.edit_queue.edited_cells_count()
        logger.debug(f"Confirm discard: {count} unsaved edits on '{tab.name}'")
        reply = QMessageBox.question(
            self,
            self._t("warning.unsaved_changes"),
            self._t("warning.unsaved_changes_msg", count=count),
            QMessageBox.StandardButton.Save | QMessageBox.StandardButton.Discard | QMessageBox.StandardButton.Cancel,
            QMessageBox.StandardButton.Save,
        )

        if reply == QMessageBox.StandardButton.Save:
            self._save_file()
            return not tab.edit_queue.is_dirty()
        elif reply == QMessageBox.StandardButton.Discard:
            logger.info(f"User discarded {count} unsaved edits on '{tab.name}'")
            return True
        else:
            logger.debug("User cancelled discard dialog")
            return False

    # ------------------------------------------------------------------
    # Operations
    # ------------------------------------------------------------------

    def _on_column_renamed(self, old_name: str, new_name: str) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.info(f"Renaming column '{old_name}' -> '{new_name}' on '{tab.name}'")
        cols = tab.engine.get_columns()
        select_parts = []
        for c in cols:
            if c == old_name:
                select_parts.append(f'"{c}" AS "{new_name}"')
            else:
                select_parts.append(f'"{c}"')
        select_list = ", ".join(select_parts)
        self._apply_transform(
            tab,
            f'SELECT {select_list} FROM ({tab.engine.current_query})',
            f'Rename "{old_name}" to "{new_name}"',
        )

    def _on_column_removed(self, column_name: str) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.info(f"Removing column '{column_name}' from '{tab.name}'")
        cols = tab.engine.get_columns()
        remaining = [c for c in cols if c != column_name]
        if not remaining:
            QMessageBox.warning(self, "Remove Column", "Cannot remove the only column.")
            return
        select_list = ", ".join(f'"{c}"' for c in remaining)
        self._apply_transform(
            tab,
            f'SELECT {select_list} FROM ({tab.engine.current_query})',
            f'Remove "{column_name}"',
        )

    def _on_column_type_changed(self, column_name: str, new_type: str) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.info(f"Changing type of '{tab.name}.{column_name}' to {new_type}")
        cols = tab.engine.get_columns()
        select_parts = []
        for c in cols:
            if c == column_name:
                select_parts.append(f'CAST("{c}" AS {new_type}) AS "{c}"')
            else:
                select_parts.append(f'"{c}"')
        select_list = ", ".join(select_parts)
        self._apply_transform(
            tab,
            f'SELECT {select_list} FROM ({tab.engine.current_query})',
            f'Change "{column_name}" type to {new_type}',
        )

    def _on_column_duplicated(self, column_name: str) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.info(f"Duplicating column '{column_name}' on '{tab.name}'")
        cols = tab.engine.get_columns()
        select_list = ", ".join(f'"{c}"' for c in cols)
        select_list += f', "{column_name}" AS "{column_name}_copy"'
        self._apply_transform(
            tab,
            f'SELECT {select_list} FROM ({tab.engine.current_query})',
            f'Duplicate "{column_name}"',
        )

    def _on_copy_column_tuple(self, column_name: str) -> None:
        tab = self._active_tab()
        if not tab:
            return
        total_rows = tab.engine.total_rows
        page_size = tab.engine.page_size
        # For small tables (single page), copy directly without dialog
        if total_rows <= page_size:
            values = self._data_table.get_column_values(column_name)
            tuple_str = "(" + ", ".join(repr(v) for v in values) + ")"
            QApplication.clipboard().setText(tuple_str)
            self.statusBar().showMessage(f"Copied {len(values)} values as tuple", 3000)
            return

        # Large table: show dialog with options
        dialog = CopyTupleDialog(
            column_name,
            total_rows,
            page_size,
            default_sample_size=self._container.settings.copy_tuple_sample_size,
            parent=self,
        )
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        result = dialog.get_result()
        if not result:
            return
        mode, n, label = result
        if mode == "page":
            values = self._data_table.get_column_values(column_name)
        elif mode == "first":
            values = tab.engine.get_column_sample(column_name, "first", n)
        elif mode == "random":
            values = tab.engine.get_column_sample(column_name, "random", n)
        else:
            values = []
        tuple_str = "(" + ", ".join(repr(v) for v in values) + ")"
        if label:
            tuple_str = f"{label} = {tuple_str}"
        QApplication.clipboard().setText(tuple_str)
        self.statusBar().showMessage(
            f"Copied {len(values)} values ({mode}) as tuple", 3000
        )

    def _on_op_math(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.debug(f"Math operation dialog opened for '{tab.name}'")
        cols = tab.engine.get_columns()
        if not cols:
            return

        from parvu.core.dsl.catalog import Catalog
        from parvu.core.dsl.registry import FunctionRegistry

        if len(cols) >= 2:
            default = f'{tab.name}[{cols[0]}] + {tab.name}[{cols[1]}]'
        else:
            default = ""

        catalog = Catalog()
        catalog.register_tab(tab.name, tab.engine)
        dialog = ExpressionDialog(
            catalog, FunctionRegistry(), "Math Operation", default, parent=self
        )
        if dialog.exec() != QDialog.DialogCode.Accepted:
            logger.debug("Math operation dialog cancelled")
            return

        result = dialog.get_result()
        if not result:
            logger.warning("Math operation dialog returned no result")
            return
        name, sql = result
        logger.info(f"Math op '{name}' on '{tab.name}': {sql[:60]}...")
        # Strip table qualifier because we are selecting from a subquery
        import re as _re
        sql = _re.sub(rf'\b{_re.escape(tab.name)}\.', '', sql)
        self._apply_transform(
            tab,
            f'SELECT *, {sql} AS "{name}" FROM ({tab.engine.current_query})',
            f'Math op "{name}"',
        )

    def _on_op_join(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.debug(f"Join dialog opened for '{tab.name}'")
        other_tabs = [
            (t.name, t.engine.get_columns())
            for t in self._tabs
            if t is not tab
        ]
        if not other_tabs:
            QMessageBox.information(self, "Join", "Open another tab to join with.")
            return

        dialog = JoinDialog(
            tab.name, tab.engine.get_columns(), other_tabs, parent=self
        )
        if dialog.exec() != QDialog.DialogCode.Accepted:
            logger.debug("Join dialog cancelled")
            return

        result = dialog.get_result()
        if not result:
            logger.warning("Join dialog returned no result")
            return
        right, join_type, left_key, right_key, sql = result
        logger.info(f"Joining '{tab.name}' with '{right}' ON {left_key}={right_key} ({join_type})")
        self._apply_transform(
            tab, sql, f'Join {right} ON {tab.name}.{left_key} = {right}.{right_key} ({join_type})'
        )

    def _on_op_append(self) -> None:
        tab = self._active_tab()
        if not tab:
            return
        logger.debug(f"Append dialog opened for '{tab.name}'")
        other_names = [t.name for t in self._tabs if t is not tab]
        if not other_names:
            QMessageBox.information(self, "Append", "Open another tab to append.")
            return

        dialog = AppendDialog(tab.name, other_names, parent=self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            logger.debug("Append dialog cancelled")
            return

        result = dialog.get_result()
        if not result:
            logger.warning("Append dialog returned no result")
            return
        selected, keyword, sql = result
        names = ", ".join(selected)
        logger.info(f"Appending {names} to '{tab.name}' ({keyword})")
        self._apply_transform(tab, sql, f'Append {names} ({keyword})')

    def _undo_last_step(self) -> None:
        """Undo the most recent applied step for the active tab."""
        tab = self._active_tab()
        if not tab or not tab.applied_steps:
            return

        undo_info = tab._undo_stack.pop()
        if undo_info["type"] == "cell_edit":
            tab.edit_queue.remove(undo_info["row"], undo_info["column"])
            tab.applied_steps.pop()
            self._steps_panel.set_steps(tab.applied_steps)
            self._load_page()
            self.statusBar().showMessage("Cell edit undone.", 3000)
            logger.info("Cell edit undone")
            return

        success, error = tab.engine.undo()
        if not success:
            QMessageBox.critical(self, "Undo Error", f"Failed to undo:\n\n{error}")
            logger.error(f"Undo failed: {error}")
            # Push undo info back since we couldn't undo
            tab._undo_stack.append(undo_info)
            return
        tab.applied_steps.pop()
        self._steps_panel.set_steps(tab.applied_steps)
        self._current_page = 1
        self._load_page()
        self.statusBar().showMessage("Last step undone.", 3000)
        logger.info("Undo applied")

    def _on_op_drop_duplicates(self) -> None:
        """Open drop-duplicates dialog and apply the transform."""
        tab = self._active_tab()
        if not tab:
            return
        cols = tab.engine.get_columns()
        if not cols:
            return
        dialog = DropDuplicatesDialog(tab.name, cols, parent=self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        result = dialog.get_result()
        if result is None:
            return
        subset, keep = result
        if not subset:
            subset = cols
        partition_cols = ", ".join(f'"{c}"' for c in subset)
        if keep == "first":
            query = f"SELECT * FROM ({tab.engine.current_query}) QUALIFY ROW_NUMBER() OVER (PARTITION BY {partition_cols}) = 1"
        else:
            query = f"SELECT * FROM ({tab.engine.current_query}) QUALIFY ROW_NUMBER() OVER (PARTITION BY {partition_cols} ORDER BY rowid DESC) = 1"
        col_names = ", ".join(subset)
        step = self._t("step.drop_duplicates", keep=keep, columns=col_names)
        self._apply_transform(tab, query, step)

    def _on_op_replace(self) -> None:
        """Open replace dialog from Operations menu and apply the transform."""
        self._run_replace_dialog()

    def _on_column_replace_values(self, column_name: str) -> None:
        """Open replace dialog from column context menu and apply the transform."""
        self._run_replace_dialog(selected_column=column_name)

    def _run_replace_dialog(self, selected_column: str | None = None) -> None:
        """Shared helper to run ReplaceDialog and apply the transform."""
        tab = self._active_tab()
        if not tab:
            return
        cols = tab.engine.get_columns()
        if not cols:
            return
        dialog = ReplaceDialog(tab.name, cols, selected_column=selected_column, parent=self)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        result = dialog.get_result()
        if not result:
            return
        col, pattern, replacement, output_col, case_sensitive, regex = result
        from parvu.core.dsl.ir import Replace, Literal, ColumnRef
        from parvu.core.dsl.compiler import Compiler
        from parvu.core.dsl.registry import FunctionRegistry
        from parvu.core.dsl.types import LogicalType
        expr = Replace(
            text=ColumnRef(table=tab.name, column=col, logical_type=LogicalType.TEXT),
            pattern=Literal(value=pattern, logical_type=LogicalType.TEXT),
            with_value=Literal(value=replacement, logical_type=LogicalType.TEXT),
            case_sensitive=case_sensitive,
            regex=regex,
        )
        sql = Compiler(FunctionRegistry()).compile(expr)
        # Strip table qualifier because we are selecting from a subquery
        import re as _re
        sql = _re.sub(rf'\b{_re.escape(tab.name)}\.', '', sql)
        # Build SELECT that overwrites the column if it already exists
        cols = tab.engine.get_columns()
        select_parts: list[str] = []
        for c in cols:
            if c == output_col:
                select_parts.append(f'{sql} AS "{c}"')
            else:
                select_parts.append(f'"{c}"')
        if output_col not in cols:
            select_parts.append(f'{sql} AS "{output_col}"')
        select_list = ", ".join(select_parts)
        query = f'SELECT {select_list} FROM ({tab.engine.current_query})'
        step = self._t("step.replace", column=col, pattern=pattern)
        self._apply_transform(tab, query, step)

    def _apply_transform(self, tab: TableTab, query: str, description: str = "") -> None:
        """Apply a SQL transformation to the given tab's engine."""
        success, error = tab.engine.apply_transform(query)
        if success:
            self._current_page = 1
            self._load_page()
            step = description or "Transform"
            tab.applied_steps.append(step)
            tab._undo_stack.append({"type": "sql"})
            self._steps_panel.set_steps(tab.applied_steps)
            self.statusBar().showMessage(f"{step} applied.", 3000)
            logger.info(f"Applied transform: {query[:80]}...")
        else:
            QMessageBox.critical(self, "Transform Error", f"Failed to apply transformation:\n\n{error}")
            logger.error(f"Transform failed: {error}")

    # ------------------------------------------------------------------

    def closeEvent(self, event) -> None:
        logger.info(f"Window closing. Tabs: {len(self._tabs)}")
        for tab in self._tabs:
            if tab.edit_queue.is_dirty():
                self._switch_tab(self._tabs.index(tab))
                if not self._confirm_discard_unsaved():
                    event.ignore()
                    return

        # Check for applied transforms
        if self._container.settings.warn_on_exit_with_transforms:
            tabs_with_transforms = [t for t in self._tabs if t.applied_steps]
            if tabs_with_transforms:
                names = ", ".join(t.name for t in tabs_with_transforms)
                dialog = ConfirmCloseDialog(
                    f"The following tabs have applied transforms that will be lost:\n\n{names}\n\n"
                    "Are you sure you want to close?",
                    parent=self,
                )
                dialog.exec()
                result = dialog.get_result()
                if result == ConfirmCloseDialog.CANCEL:
                    event.ignore()
                    return
                if result == ConfirmCloseDialog.SAVE_AND_CLOSE:
                    if not self._export_results():
                        event.ignore()
                        return
                if dialog.dont_ask_again():
                    self._container.settings.warn_on_exit_with_transforms = False
                    self._container.settings_manager.save()
                    QMessageBox.information(
                        self,
                        "Setting Saved",
                        "You can re-enable this warning in Settings → General.",
                    )

        for tab in self._tabs:
            tab.engine.close()
        self._shared_conn.close()
        logger.info("Window closed, shared connection released")
        if self._window_service:
            self._window_service.remove_window(self)
        event.accept()
