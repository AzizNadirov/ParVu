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
    QMessageBox, QProgressDialog, QApplication,
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
from parvu.core.edit_queue import EditQueue, apply_edits_to_dataframe
from parvu.presentation.themeable import ThemeableMixin
from parvu.presentation.workers import ExportWorker, QueryWorker, SaveWorker, UniqueValuesWorker
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
from parvu.presentation.dialogs.expression_help_dialog import ExpressionHelpDialog
from parvu.presentation.dialogs.table_info_dialog import TableInfoDialog
from parvu.presentation.dialogs.unique_values_dialog import UniqueValuesDialog
from parvu.presentation.dialogs.crash_reporter import CrashReportDialog
from parvu.presentation.dialogs.expression_dialog import ExpressionDialog
from parvu.presentation.dialogs.join_dialog import JoinDialog
from parvu.presentation.dialogs.append_dialog import AppendDialog
from parvu.presentation.dialogs.drop_duplicates_dialog import DropDuplicatesDialog
from parvu.presentation.dialogs.drop_null_dialog import DropNullDialog
from parvu.presentation.dialogs.replace_dialog import ReplaceDialog
from parvu.presentation.dialogs.confirm_close_dialog import ConfirmCloseDialog
from parvu.presentation.dialogs.unsaved_changes_dialog import UnsavedChangesDialog
from parvu.presentation.dialogs.copy_tuple_dialog import CopyTupleDialog
from parvu.presentation.dialogs.export_dialog import ExportDialog
from parvu.presentation.widgets.applied_steps import AppliedStepsPanel
from parvu.presentation.widgets.collapsible_panel import CollapsiblePanel
from parvu.presentation.widgets.find_bar import FindBar
from parvu.presentation.dialogs.search_dialog import SearchDialog


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

        self.setAcceptDrops(True)

        self._setup_ui()
        self._apply_theme()
        self._setup_menu()
        self._install_shortcuts()

        if file_path:
            self._add_tab(file_path)

        logger.info("MainWindow initialized")

    def dragEnterEvent(self, event) -> None:
        if event.mimeData().hasUrls() and any(
            url.isLocalFile() for url in event.mimeData().urls()
        ):
            event.acceptProposedAction()
        else:
            event.ignore()

    def dragMoveEvent(self, event) -> None:
        if event.mimeData().hasUrls():
            event.acceptProposedAction()

    def dropEvent(self, event) -> None:
        urls = event.mimeData().urls()
        opened = 0
        for url in urls:
            if not url.isLocalFile():
                continue
            path = Path(url.toLocalFile())
            if path.is_file():
                logger.info(f"Drop opened: {path}")
                self._add_tab(path)
                opened += 1
        if opened:
            event.acceptProposedAction()
        else:
            event.ignore()

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

        # Query Editor (collapsible Operations panel)
        self._query_panel = CollapsiblePanel(
            title=self._t("menu.operations"),
            expanded=True,
        )

        self._query_editor = QueryEditor(
            theme=self._container.theme_manager.current_theme,
        )
        self._query_editor.set_header_label(
            self._t("label.sql_query", table_name=self._container.settings.default_data_var_name)
        )
        self._query_editor.mode_changed.connect(self._on_query_mode_changed)
        self._query_panel.add_widget(self._query_editor)

        self._query_toolbar = QueryToolbar()
        self._query_toolbar.execute_clicked.connect(self._execute_query)
        self._query_toolbar.reset_clicked.connect(self._reset_query)
        self._query_toolbar.info_clicked.connect(self._show_table_info)
        self._query_panel.add_widget(self._query_toolbar)

        layout.addWidget(self._query_panel)

        # Applied Steps
        self._steps_panel = AppliedStepsPanel()
        self._steps_panel.undo_requested.connect(self._undo_last_step)
        layout.addWidget(self._steps_panel)

        # Data table
        self._data_table = DataTableView(
            theme=self._container.theme_manager.current_theme,
            translator=self._t,
        )
        self._data_table.sort_requested.connect(self._on_sort)
        self._data_table.unique_values_requested.connect(self._on_unique_values)
        self._data_table.cell_edited.connect(self._on_cell_edited)
        self._data_table.column_renamed.connect(self._on_column_renamed)
        self._data_table.column_removed.connect(self._on_column_removed)
        self._data_table.column_type_changed.connect(self._on_column_type_changed)
        self._data_table.column_duplicated.connect(self._on_column_duplicated)
        self._data_table.replace_values_requested.connect(self._on_column_replace_values)
        self._data_table.copy_column_tuple_requested.connect(self._on_copy_column_tuple)
        self._data_table.drop_null_requested.connect(self._on_column_drop_null)
        self._data_table.column_stats_requested.connect(self._on_column_stats)
        layout.addWidget(self._data_table)

        # Find bar (Ctrl+F overlay)
        self._find_bar = FindBar(self._t, self)
        self._find_bar.text_changed.connect(self._find_run)
        self._find_bar.options_changed.connect(self._find_run)
        self._find_bar.next_match.connect(lambda: self._find_step(1))
        self._find_bar.prev_match.connect(lambda: self._find_step(-1))
        self._find_bar.closed.connect(self._find_close)
        self._find_matches: list[tuple[int, int]] = []
        self._find_index: int = -1
        layout.addWidget(self._find_bar)

        # Full-table search dialog (lazy: created on first open)
        self._search_dialog: SearchDialog | None = None
        # If a search jump triggers a page load, remember the cell to select
        # once _on_page_loaded fires.
        self._pending_search_jump: tuple[int, str] | None = None

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
        new_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileIcon))
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

        save_action = QAction(self._t("menu.file.save"), self)
        save_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogSaveButton))
        save_action.setShortcut("Ctrl+S")
        save_action.triggered.connect(self._save_file)
        file_menu.addAction(save_action)

        export_action = QAction(self._t("menu.file.export"), self)
        export_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileLinkIcon))
        export_action.setShortcut("Ctrl+Shift+S")
        export_action.triggered.connect(self._export_results)
        file_menu.addAction(export_action)
        file_menu.addSeparator()

        settings_action = QAction(self._t("menu.file.settings"), self)
        settings_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileDialogDetailedView))
        settings_action.triggered.connect(self._edit_settings)
        file_menu.addAction(settings_action)

        self._recents_menu = file_menu.addMenu(self._t("menu.file.recent_files"))
        self._recents_menu.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DirIcon))
        self._update_recents_menu()
        file_menu.addSeparator()

        exit_action = QAction(self._t("menu.file.exit"), self)
        exit_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogCloseButton))
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        # ── Edit Menu ──
        edit_menu = menubar.addMenu(self._t("menu.edit"))

        find_action = QAction(self._t("menu.edit.find"), self)
        find_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileDialogContentsView))
        find_action.setShortcut("Ctrl+F")
        find_action.triggered.connect(self._open_search_dialog_from_menu)
        edit_menu.addAction(find_action)

        find_next_action = QAction(self._t("menu.edit.find_next"), self)
        find_next_action.setShortcut("F3")
        find_next_action.triggered.connect(self._find_next_from_menu)
        edit_menu.addAction(find_next_action)

        find_prev_action = QAction(self._t("menu.edit.find_prev"), self)
        find_prev_action.setShortcut("Shift+F3")
        find_prev_action.triggered.connect(self._find_prev_from_menu)
        edit_menu.addAction(find_prev_action)

        # ── Operations Menu ──
        operations_menu = menubar.addMenu(self._t("menu.operations"))

        math_action = QAction(self._t("menu.operations.math"), self)
        math_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileDialogContentsView))
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

        drop_dup_action = QAction(self._t("menu.operations.drop_duplicates"), self)
        drop_dup_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogDiscardButton))
        drop_dup_action.triggered.connect(self._on_op_drop_duplicates)
        operations_menu.addAction(drop_dup_action)

        drop_null_action = QAction(self._t("menu.operations.drop_null"), self)
        drop_null_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogDiscardButton))
        drop_null_action.triggered.connect(self._on_op_drop_null)
        operations_menu.addAction(drop_null_action)

        replace_action = QAction(self._t("menu.operations.replace"), self)
        replace_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_BrowserReload))
        replace_action.triggered.connect(self._on_op_replace)
        operations_menu.addAction(replace_action)

        # ── Help Menu ──
        help_menu = menubar.addMenu(self._t("menu.help"))

        expr_help_action = QAction(self._t("menu.help.expression"), self)
        expr_help_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_DialogHelpButton))
        expr_help_action.triggered.connect(self._show_expression_help)
        help_menu.addAction(expr_help_action)

        shortcuts_action = QAction(self._t("menu.help.shortcuts"), self)
        shortcuts_action.setIcon(style.standardIcon(QStyle.StandardPixmap.SP_FileDialogListView))
        shortcuts_action.setShortcut("Ctrl+/")
        shortcuts_action.triggered.connect(self._show_shortcuts)
        help_menu.addAction(shortcuts_action)

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
        self._query_panel.set_theme(theme)
        self._steps_panel.set_theme(theme)

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
            current.sort_column = self._data_table._sort_column
            current.sort_ascending = self._data_table._sort_ascending
            self._data_table.set_edit_queue(None)

        self._active_tab_index = index
        new_tab = self._active_tab()
        if not new_tab:
            return

        logger.info(f"Switched to tab '{new_tab.name}' ({new_tab.engine.total_rows} rows)")

        # Restore new tab state
        self._current_page = new_tab.current_page
        self._data_table._sort_column = new_tab.sort_column
        self._data_table._sort_ascending = new_tab.sort_ascending
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

        # Refresh search dialog (if open) with the new engine
        if self._search_dialog is not None and self._search_dialog.isVisible():
            self._search_dialog.set_engine(new_tab.engine)

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

        # Complete any deferred search-jump now that the page is in the table.
        if self._pending_search_jump is not None:
            row_in_page, col_name = self._pending_search_jump
            logger.info(
                f"[search] _on_page_loaded: completing deferred jump to "
                f"row_in_page={row_in_page}, col='{col_name}'"
            )
            self._pending_search_jump = None
            self._select_search_cell(row_in_page, col_name)

    def _on_query_error(self, error_msg: str) -> None:
        self._pending_step = None
        QMessageBox.critical(
            self, self._t("error.query_error"),
            self._t("error.query_error_msg", error_msg=error_msg)
        )
        self.statusBar().showMessage(self._t("status.query_failed"))
        logger.error(f"Query error: {error_msg}")

    def _on_query_mode_changed(self, is_expression: bool) -> None:
        """Update the query editor header label when mode changes."""
        tab = self._active_tab()
        table_name = tab.name if tab else self._container.settings.default_data_var_name
        if is_expression:
            self._query_editor.set_header_label(self._t("label.expression_query", table_name=table_name))
        else:
            self._query_editor.set_header_label(self._t("label.sql_query", table_name=table_name))

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
        """Export the current result: options dialog, then a background write."""
        tab = self._active_tab()
        if not tab:
            QMessageBox.warning(self, self._t("warning.no_data"), self._t("warning.no_data_msg"))
            return False

        default_path = (
            tab.file_path.with_suffix(".csv") if tab.file_path else Path("export.csv")
        )
        dialog = ExportDialog(
            default_path,
            tab.engine.get_column_types(),
            self,
            pending_edits=tab.edit_queue.edited_cells_count(),
        )
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return False
        options = dialog.get_options()

        # Pending cell edits only exist in memory — materialize them so the
        # export reflects what the user sees.
        source_df = None
        edits = tab.edit_queue.all_edits()
        if edits:
            source_df = apply_edits_to_dataframe(tab.engine.fetch_dataframe(), edits)

        total_rows = max(tab.engine.total_rows, 1)
        logger.info(f"Exporting '{tab.name}' -> {options.path} ({options.format})")

        self._export_progress = QProgressDialog(
            self._t("export.progress", path=options.path.name), self._t("btn.cancel"),
            0, 100, self,
        )
        self._export_progress.setWindowTitle(self._t("dialog.export_results"))
        self._export_progress.setWindowModality(Qt.WindowModality.WindowModal)
        self._export_progress.setMinimumDuration(0)
        self._export_progress.setAutoClose(False)
        self._export_progress.setAutoReset(False)
        self._export_progress.setValue(0)

        self._export_worker = ExportWorker(
            tab.engine.new_cursor(), tab.engine.current_query, options, source_df
        )
        self._export_worker.progress.connect(
            lambda rows: self._on_export_progress(rows, total_rows)
        )
        self._export_worker.done.connect(lambda rows: self._on_export_done(options.path, rows))
        self._export_worker.cancelled.connect(self._on_export_cancelled)
        self._export_worker.error.connect(self._on_export_error)
        self._export_progress.canceled.connect(self._export_worker.cancel)
        self._export_worker.start()
        return True

    def _on_export_progress(self, rows: int, total_rows: int) -> None:
        if getattr(self, "_export_progress", None) is not None:
            self._export_progress.setLabelText(
                self._t("export.progress_rows", rows=f"{rows:,}", total=f"{total_rows:,}")
            )
            self._export_progress.setValue(min(100, rows * 100 // total_rows))

    def _close_export_progress(self) -> None:
        if getattr(self, "_export_progress", None) is not None:
            # QProgressDialog.close() emits canceled() — don't let it reach the worker.
            self._export_progress.blockSignals(True)
            self._export_progress.close()
            self._export_progress = None

    def _on_export_done(self, path: Path, rows: int) -> None:
        self._close_export_progress()
        logger.info(f"Export complete: {rows} rows -> {path}")
        QMessageBox.information(
            self, self._t("success.export_complete"),
            self._t("success.export_complete_msg", path=path) + f"\n\n{rows:,} rows",
        )

    def _on_export_cancelled(self) -> None:
        self._close_export_progress()
        logger.info("Export cancelled by user")
        self.statusBar().showMessage(self._t("export.cancelled"), 5000)

    def _on_export_error(self, error_msg: str) -> None:
        self._close_export_progress()
        logger.error(f"Export failed: {error_msg}")
        QMessageBox.critical(
            self, self._t("error.export_error"),
            self._t("error.export_error_msg", error=error_msg),
        )

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

    def _show_expression_help(self) -> None:
        logger.debug("Opening expression help dialog")
        dialog = ExpressionHelpDialog(self._container.translator, parent=self)
        dialog.exec()

    def _show_about(self) -> None:
        logger.debug("Opening about dialog")
        dialog = AboutDialog(self._container.translator, self)
        dialog.exec()

    # ── Keyboard shortcuts & Find-in-table ──────────────────────────────────
    def _install_shortcuts(self) -> None:
        from PyQt6.QtGui import QShortcut, QKeySequence

        # Ctrl+Shift+F kept as alias so muscle memory still works.
        search_alias_sc = QShortcut(QKeySequence("Ctrl+Shift+F"), self)
        search_alias_sc.activated.connect(self._open_search_dialog_from_shortcut)

        cheat_sc = QShortcut(QKeySequence("Ctrl+/"), self)
        cheat_sc.activated.connect(self._show_shortcuts)
        logger.info(
            "[shortcuts] installed: Ctrl+Shift+F=Search, Ctrl+/=Shortcuts cheatsheet. "
            "Ctrl+F bound via Edit menu QAction."
        )

    def _open_search_dialog_from_shortcut(self) -> None:
        logger.info("[shortcut] Ctrl+Shift+F pressed -> opening search dialog")
        self._open_search_dialog()

    def _open_search_dialog_from_menu(self) -> None:
        logger.info("[menu] Edit > Find / Ctrl+F -> opening search dialog")
        self._open_search_dialog()

    def _find_next_from_menu(self) -> None:
        logger.info("[menu] Find Next (F3)")
        if self._search_dialog is None or not self._search_dialog.isVisible():
            self._open_search_dialog()
            return
        self._search_dialog._on_next()

    def _find_prev_from_menu(self) -> None:
        logger.info("[menu] Find Previous (Shift+F3)")
        if self._search_dialog is None or not self._search_dialog.isVisible():
            self._open_search_dialog()
            return
        self._search_dialog._on_prev()

    def _open_search_dialog(self) -> None:
        tab = self._active_tab()
        if not tab:
            QMessageBox.information(
                self,
                self._t("search.menu_msgbox.title"),
                self._t("search.menu_msgbox.no_file"),
            )
            return
        if self._search_dialog is None:
            self._search_dialog = SearchDialog(self._t, self)
            self._search_dialog.jump_requested.connect(self._search_jump)
        logger.info(
            f"[search] opening dialog: tab='{tab.name}', "
            f"engine.total_rows={tab.engine.total_rows}, "
            f"page={self._current_page}, table_rows={self._data_table.rowCount()}"
        )
        self._search_dialog.set_engine(tab.engine)

        # Anchor at the current table cursor so the first Next/Prev moves
        # forward/backward from where the user is looking.
        page_size = tab.engine.page_size
        page_offset = (self._current_page - 1) * page_size
        cur_row = self._data_table.currentRow()
        cur_col = self._data_table.currentColumn()
        anchor_row = page_offset + cur_row if cur_row >= 0 else None
        anchor_col = cur_col if cur_col >= 0 else None
        self._search_dialog.set_initial_anchor(anchor_row, anchor_col)

        preset = ""
        selected = self._data_table.selectedItems()
        if selected and selected[0].text() and len(selected[0].text()) < 80:
            preset = selected[0].text()

        # Position near top-right of main window so dialog isn't hidden.
        parent_rect = self.geometry()
        dialog_w = 560
        dialog_h = 260
        target_x = max(0, parent_rect.x() + parent_rect.width() - dialog_w - 40)
        target_y = parent_rect.y() + 80
        self._search_dialog.resize(dialog_w, dialog_h)
        self._search_dialog.move(target_x, target_y)

        self._search_dialog.show()
        self._search_dialog.raise_()
        self._search_dialog.activateWindow()
        self._search_dialog.focus_input(preset)
        logger.info(
            f"[search] dialog shown at ({target_x}, {target_y}), "
            f"visible={self._search_dialog.isVisible()}, "
            f"active={self._search_dialog.isActiveWindow()}"
        )

    def _search_jump(self, absolute_row: int, column_name: str) -> None:
        tab = self._active_tab()
        if not tab:
            logger.warning("[search] _search_jump: no active tab")
            return
        page_size = tab.engine.page_size
        target_page = (absolute_row // page_size) + 1
        row_in_page = absolute_row % page_size
        logger.info(
            f"[search] _search_jump: absolute_row={absolute_row}, col='{column_name}', "
            f"target_page={target_page} (current={self._current_page}), "
            f"row_in_page={row_in_page}"
        )

        if target_page != self._current_page:
            self._pending_search_jump = (row_in_page, column_name)
            self._current_page = target_page
            logger.debug(
                f"[search] cross-page jump; deferring select until page {target_page} loads"
            )
            self._load_page()
            return

        self._select_search_cell(row_in_page, column_name)

    def _select_search_cell(self, row_in_page: int, column_name: str) -> None:
        """Select and scroll-to the cell on the currently loaded page."""
        col_idx = -1
        for i in range(self._data_table.columnCount()):
            item = self._data_table.horizontalHeaderItem(i)
            if item and item.text() == column_name:
                col_idx = i
                break

        rc = self._data_table.rowCount()
        cc = self._data_table.columnCount()
        if not (0 <= row_in_page < rc) or col_idx < 0:
            logger.warning(
                f"[search] _select_search_cell out of range: "
                f"row_in_page={row_in_page}, col='{column_name}', "
                f"col_idx={col_idx}, rowCount={rc}, colCount={cc}"
            )
            return

        self._data_table.setCurrentCell(row_in_page, col_idx)
        cell = self._data_table.item(row_in_page, col_idx)
        if cell:
            self._data_table.scrollToItem(cell)
        page_size = self._active_tab().engine.page_size
        absolute = (self._current_page - 1) * page_size + row_in_page
        logger.info(
            f"[search] _select_search_cell: selected ({row_in_page}, {col_idx}), "
            f"absolute row {absolute + 1}, col='{column_name}'"
        )
        self.statusBar().showMessage(
            self._t("search.statusbar.jumped", row=absolute + 1, column=column_name)
        )

    def _find_open(self) -> None:
        preset = ""
        selected = self._data_table.selectedItems()
        if selected:
            text = selected[0].text()
            if text and len(text) < 80:
                preset = text
        self._find_bar.show()
        self._find_bar.focus_input(preset)
        if preset:
            self._find_run()

    def _find_close(self) -> None:
        self._find_bar.hide()
        self._find_matches = []
        self._find_index = -1
        self._data_table.setFocus()

    def _find_run(self) -> None:
        query = self._find_bar.query
        self._find_matches = []
        self._find_index = -1
        if not query:
            self._find_bar.set_match_count(0, 0)
            return

        case = self._find_bar.case_sensitive
        whole = self._find_bar.whole_cell
        needle = query if case else query.lower()

        rows = self._data_table.rowCount()
        cols = self._data_table.columnCount()
        for r in range(rows):
            for c in range(cols):
                item = self._data_table.item(r, c)
                if item is None:
                    continue
                hay = item.text() if case else item.text().lower()
                if whole:
                    matched = hay == needle
                else:
                    matched = needle in hay
                if matched:
                    self._find_matches.append((r, c))

        if self._find_matches:
            self._find_index = 0
            self._find_jump_to_current()
        self._find_bar.set_match_count(
            self._find_index + 1 if self._find_matches else 0,
            len(self._find_matches),
        )

    def _find_step(self, direction: int) -> None:
        if not self._find_matches:
            if self._find_bar.isVisible():
                self._find_run()
            return
        self._find_index = (self._find_index + direction) % len(self._find_matches)
        self._find_jump_to_current()
        self._find_bar.set_match_count(self._find_index + 1, len(self._find_matches))

    def _find_jump_to_current(self) -> None:
        r, c = self._find_matches[self._find_index]
        self._data_table.setCurrentCell(r, c)
        item = self._data_table.item(r, c)
        if item:
            self._data_table.scrollToItem(item)

    def _show_shortcuts(self) -> None:
        from parvu.presentation.dialogs.shortcuts_dialog import ShortcutsDialog
        ShortcutsDialog(self._t, self).exec()

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
        """Save edits and applied transforms to the original file."""
        tab = self._active_tab()
        if not tab:
            return
        if not tab.edit_queue.is_dirty() and not tab.applied_steps:
            logger.debug("Save file: no unsaved changes (no edits and no transforms)")
            return
        if not tab.file_path or not tab.file_path.exists():
            self._export_results()
            return
        logger.info(
            f"Saving to original file: {tab.file_path} "
            f"(edits={tab.edit_queue.edited_cells_count()}, "
            f"transforms={len(tab.applied_steps)})"
        )
        self._do_save(tab.file_path)

    def _do_save(self, output_path: Path) -> None:
        """Save the active tab's state (transforms + edits) to ``output_path``.

        Three paths:
        - Transforms only: synchronous DuckDB COPY (fast, single-threaded).
        - Transforms + edits: synchronous COPY to a temp file, then pandas
          applies edits and rewrites.
        - Edits only: legacy background worker reads the original file with
          pandas, applies edits, writes.
        """
        tab = self._active_tab()
        if not tab:
            return
        edits = tab.edit_queue.all_edits()
        has_transforms = bool(tab.applied_steps)
        if not edits and not has_transforms:
            logger.debug("Do save: nothing to save (no edits, no transforms)")
            return

        logger.info(
            f"Starting save -> {output_path} "
            f"(edits={len(edits)}, transforms={len(tab.applied_steps)})"
        )
        self.statusBar().showMessage(self._t("status.saving"))

        if has_transforms:
            # Run synchronously to avoid sharing the DuckDB connection with a
            # QThread (DuckDB connections are not thread-safe).
            if hasattr(self, "_worker") and self._worker is not None and self._worker.isRunning():
                self._worker.wait(10000)
            self._save_progress = QProgressDialog(
                self._t("status.saving"), None, 0, 0, self
            )
            self._save_progress.setWindowModality(Qt.WindowModality.WindowModal)
            self._save_progress.setCancelButton(None)
            self._save_progress.show()
            QApplication.processEvents()
            try:
                if edits:
                    # Transforms + edits: COPY transformed result to a temp
                    # file, then read it with pandas, apply edits, rewrite.
                    import tempfile
                    from parvu.core.edit_queue import (
                        apply_edits_to_dataframe,
                        read_source_file,
                        write_source_file,
                    )
                    suffix = output_path.suffix or ".parquet"
                    tmp = tempfile.NamedTemporaryFile(suffix=suffix, delete=False)
                    tmp.close()
                    tmp_path = Path(tmp.name)
                    try:
                        tab.engine.export_results(tmp_path)
                        df = read_source_file(tmp_path)
                        df = apply_edits_to_dataframe(df, edits)
                        write_source_file(df, output_path)
                        del df
                    finally:
                        tmp_path.unlink(missing_ok=True)
                else:
                    # Transforms only: direct DuckDB COPY to the output file.
                    tab.engine.export_results(output_path)
            except Exception as e:
                self._save_progress.close()
                self._save_progress = None
                logger.exception(f"Transformed save failed: {e}")
                self._on_save_error(str(e))
                return
            self._on_save_finished(output_path)
            return

        # Edits-only fast path — pandas read of original file, apply edits, write back.
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
        """Called when save completes successfully (sync or background)."""
        if getattr(self, "_save_progress", None) is not None:
            self._save_progress.close()
            self._save_progress = None
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
        """Called when save fails (sync or background)."""
        if getattr(self, "_save_progress", None) is not None:
            self._save_progress.close()
            self._save_progress = None
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

        msg_key = (
            "warning.unsaved_changes_msg_single"
            if count == 1
            else "warning.unsaved_changes_msg"
        )
        dialog = UnsavedChangesDialog(
            self._t("warning.unsaved_changes"),
            self._t(msg_key, count=count),
            self,
        )
        dialog.exec()

        if dialog.get_result() == UnsavedChangesDialog.SAVE:
            self._save_file()
            return not tab.edit_queue.is_dirty()
        elif dialog.get_result() == UnsavedChangesDialog.DISCARD:
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

    def _on_column_stats(self, column_name: str) -> None:
        """Compute and show summary statistics for the chosen column."""
        tab = self._active_tab()
        if not tab:
            return
        if hasattr(self, "_worker") and self._worker is not None and self._worker.isRunning():
            self._worker.wait(10000)
        self.statusBar().showMessage(self._t("column_stats.computing"))
        QApplication.processEvents()
        try:
            stats = tab.engine.get_column_stats(column_name)
        except Exception as e:
            logger.exception(f"Column stats failed for '{column_name}': {e}")
            QMessageBox.warning(
                self,
                self._t("column_stats.error_title"),
                self._t("column_stats.error_msg", column=column_name, error=str(e)),
            )
            self._update_status_bar()
            return
        self._update_status_bar()
        from parvu.presentation.dialogs.column_stats_dialog import ColumnStatsDialog
        ColumnStatsDialog(column_name, stats, translator=self._t, parent=self).exec()

    def _on_op_drop_null(self) -> None:
        """Open drop-null dialog from Operations menu."""
        self._run_drop_null_dialog()

    def _on_column_drop_null(self, column_name: str) -> None:
        """Open drop-null dialog from column header context menu."""
        self._run_drop_null_dialog(selected_column=column_name)

    def _run_drop_null_dialog(self, selected_column: str | None = None) -> None:
        """Shared helper to run DropNullDialog and apply the transform."""
        tab = self._active_tab()
        if not tab:
            return
        cols = tab.engine.get_columns()
        if not cols:
            return
        dialog = DropNullDialog(
            tab.name, cols,
            translator=self._t,
            selected_column=selected_column,
            parent=self,
        )
        if dialog.exec() != QDialog.DialogCode.Accepted:
            return
        result = dialog.get_result()
        if result is None:
            return
        col, null_value = result
        query, value_repr = self._build_drop_null_query(tab, col, null_value)
        step = self._t("step.drop_null", column=col, value=value_repr)
        self._apply_transform(tab, query, step)

    @staticmethod
    def _build_drop_null_query(tab, col: str, null_value: object) -> tuple[str, str]:
        """Build the SQL for DROP_NULL and a human-readable value label.

        Returns (sql, value_repr).
        """
        quoted_col = f'"{col}"'
        if null_value is None:
            sql = (
                f"SELECT * FROM ({tab.engine.current_query}) "
                f"WHERE {quoted_col} IS NOT NULL"
            )
            return sql, "NULL"
        # User-typed sentinel: try numeric first, fall back to quoted string.
        raw = str(null_value)
        try:
            int(raw)
            literal_sql = raw
        except ValueError:
            try:
                float(raw)
                literal_sql = raw
            except ValueError:
                escaped = raw.replace("'", "''")
                literal_sql = f"'{escaped}'"
        sql = (
            f"SELECT * FROM ({tab.engine.current_query}) "
            f"WHERE {quoted_col} IS NULL OR {quoted_col} <> {literal_sql}"
        )
        return sql, raw

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
                    self._t("dialog.confirm_close.message", tabs=names),
                    parent=self,
                    translator=self._t,
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
                        self._t("dialog.confirm_close.setting_saved"),
                        self._t("dialog.confirm_close.setting_saved_msg"),
                    )

        # An export holds a cursor on the shared connection — let it finish
        # before tearing the connection down.
        worker = getattr(self, "_export_worker", None)
        while worker is not None and worker.isRunning():
            QApplication.processEvents()
            worker.wait(50)

        for tab in self._tabs:
            tab.engine.close()
        self._shared_conn.close()
        logger.info("Window closed, shared connection released")
        if self._window_service:
            self._window_service.remove_window(self)
        event.accept()
