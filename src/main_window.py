"""
Main Application Window for ParVu
Complete rewrite with PyQt6
"""
from pathlib import Path
from PyQt6.QtWidgets import (QMainWindow, QWidget, QVBoxLayout, QHBoxLayout, QPushButton,
                             QLabel, QLineEdit, QFileDialog, QMessageBox, QProgressDialog,
                             QDialog, QTextBrowser, QFormLayout)
from PyQt6.QtCore import Qt, QThread, pyqtSignal
from PyQt6.QtGui import QIcon, QFont, QAction
import pandas as pd
from loguru import logger

from engine import QueryEngine
from sql_editor import SQLEditor
from table_view import DataTableView, UniqueValuesDialog
from schemas import settings, recents, Settings
from themes import theme_manager, Theme
from settings_dialog import SettingsDialog
from i18n import t


class QueryWorker(QThread):
    """Background thread for executing queries"""

    finished = pyqtSignal(pd.DataFrame)
    error = pyqtSignal(str)

    def __init__(self, engine: QueryEngine, page_num: int, query: str = None):
        super().__init__()
        self.engine = engine
        self.page_num = page_num
        self.query = query

    def run(self):
        """Execute query or load page"""
        try:
            if self.query:
                # Execute new query
                success, error_msg = self.engine.execute_query(self.query)
                if not success:
                    self.error.emit(f"Query Error:\n\n{error_msg}")
                    return

            # Load page
            df = self.engine.get_page(self.page_num)
            self.finished.emit(df)

        except Exception as e:
            self.error.emit(f"Error: {str(e)}")


class MainWindow(QMainWindow):
    """Main application window"""

    def __init__(self, file_path: str = None, window_manager=None):
        super().__init__()
        self.engine = None
        self.current_page = 1
        self.window_manager = window_manager

        # Load theme
        self.load_theme()

        self.setup_ui()
        self.setup_menu()

        # Apply theme
        self.apply_current_theme()

        # Load file if provided
        if file_path:
            self.load_file(file_path)

        logger.info("MainWindow initialized")

    def setup_ui(self):
        """Setup main UI components"""
        self.setWindowTitle(t("app.title"))
        self.setGeometry(100, 100, 1400, 900)

        # Create central widget
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        layout = QVBoxLayout(central_widget)

        # File selection section
        file_layout = QHBoxLayout()

        self.file_path_edit = QLineEdit()
        self.file_path_edit.setPlaceholderText(t("label.file_placeholder"))
        file_layout.addWidget(self.file_path_edit, stretch=3)

        self.browse_btn = QPushButton(t("btn.browse"))
        self.browse_btn.setStyleSheet(
            "QPushButton {"
            "    background-color: #4CAF50;"
            "    color: white;"
            "    font-weight: bold;"
            "    padding: 5px 15px;"
            "    border-radius: 3px;"
            "}"
            "QPushButton:hover {"
            "    background-color: #45a049;"
            "}"
            "QPushButton:pressed {"
            "    background-color: #3d8b40;"
            "}"
        )
        self.browse_btn.clicked.connect(self.browse_file)
        file_layout.addWidget(self.browse_btn)

        self.load_btn = QPushButton(t("btn.reload"))
        self.load_btn.setStyleSheet(
            "QPushButton {"
            "    background-color: #FF9800;"
            "    color: white;"
            "    font-weight: bold;"
            "    padding: 5px 15px;"
            "    border-radius: 3px;"
            "}"
            "QPushButton:hover {"
            "    background-color: #FB8C00;"
            "}"
            "QPushButton:pressed {"
            "    background-color: #F57C00;"
            "}"
        )
        self.load_btn.clicked.connect(self.load_file_from_input)
        file_layout.addWidget(self.load_btn)

        layout.addLayout(file_layout)

        # SQL Editor section
        sql_label = QLabel(t("label.sql_query", table_name=settings.render_vars(settings.default_data_var_name)))
        sql_label.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        layout.addWidget(sql_label)

        self.sql_editor = SQLEditor(theme=self.current_theme)
        if self.current_theme:
            self.sql_editor.setMaximumHeight(self.current_theme.layout.sql_editor_height)
        else:
            self.sql_editor.setMaximumHeight(100)
        layout.addWidget(self.sql_editor)

        # Execute button
        execute_layout = QHBoxLayout()

        self.execute_btn = QPushButton(t("btn.execute"))
        self.execute_btn.setStyleSheet(
            "QPushButton {"
            "    background-color: #0288D1;"
            "    color: white;"
            "    font-weight: bold;"
            "    padding: 5px 15px;"
            "    border-radius: 3px;"
            "}"
            "QPushButton:hover {"
            "    background-color: #0277BD;"
            "}"
            "QPushButton:pressed {"
            "    background-color: #01579B;"
            "}"
            "QPushButton:disabled {"
            "    background-color: #B0BEC5;"
            "    color: #78909C;"
            "}"
        )
        self.execute_btn.clicked.connect(self.execute_query)
        self.execute_btn.setEnabled(False)
        execute_layout.addWidget(self.execute_btn)

        self.reset_btn = QPushButton(t("btn.reset"))
        self.reset_btn.clicked.connect(self.reset_query)
        self.reset_btn.setEnabled(False)
        execute_layout.addWidget(self.reset_btn)

        self.table_info_btn = QPushButton(t("btn.table_info"))
        self.table_info_btn.setStyleSheet(f"background-color: {settings.colour_tableInfoButton}")
        self.table_info_btn.clicked.connect(self.show_table_info)
        self.table_info_btn.setEnabled(False)
        execute_layout.addWidget(self.table_info_btn)

        execute_layout.addStretch()
        layout.addLayout(execute_layout)

        # Results section
        results_label = QLabel(t("label.results"))
        results_label.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        layout.addWidget(results_label)

        # Table view
        self.table_view = DataTableView(theme=self.current_theme)
        self.table_view.sort_requested.connect(self.on_sort_requested)
        self.table_view.unique_values_requested.connect(self.on_unique_values_requested)
        layout.addWidget(self.table_view)

        # Pagination section
        pagination_layout = QHBoxLayout()

        self.prev_btn = QPushButton(t("btn.previous"))
        self.prev_btn.clicked.connect(self.prev_page)
        self.prev_btn.setEnabled(False)
        pagination_layout.addWidget(self.prev_btn)

        self.page_label = QLabel(t("label.page"))
        self.page_label.setFont(QFont("Arial", 10, QFont.Weight.Bold))
        self.page_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        pagination_layout.addWidget(self.page_label, stretch=1)

        self.next_btn = QPushButton(t("btn.next"))
        self.next_btn.clicked.connect(self.next_page)
        self.next_btn.setEnabled(False)
        pagination_layout.addWidget(self.next_btn)

        layout.addLayout(pagination_layout)

        # Status bar
        self.statusBar().showMessage(t("status.ready"))

    def setup_menu(self):
        """Setup menu bar"""
        menubar = self.menuBar()

        # File menu
        file_menu = menubar.addMenu(t("menu.file"))

        # New Window action
        new_window_action = QAction(t("menu.file.new_window"), self)
        new_window_action.setShortcut("Ctrl+N")
        new_window_action.triggered.connect(self.create_new_window)
        file_menu.addAction(new_window_action)

        file_menu.addSeparator()

        # Open file action
        open_action = QAction(t("menu.file.open"), self)
        open_action.setShortcut("Ctrl+O")
        open_action.triggered.connect(self.browse_file)
        file_menu.addAction(open_action)

        file_menu.addSeparator()

        # Export action
        export_action = QAction(t("menu.file.export"), self)
        export_action.triggered.connect(self.export_results)
        file_menu.addAction(export_action)

        file_menu.addSeparator()

        # Settings action (includes theme selection)
        settings_action = QAction(t("menu.file.settings"), self)
        settings_action.triggered.connect(self.edit_settings)
        file_menu.addAction(settings_action)

        # Recents submenu
        self.recents_menu = file_menu.addMenu(t("menu.file.recent_files"))
        self.update_recents_menu()

        file_menu.addSeparator()

        # Exit action
        exit_action = QAction(t("menu.file.exit"), self)
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        # Help menu
        help_menu = menubar.addMenu(t("menu.help"))
        help_action = QAction(t("menu.help.about"), self)
        help_action.triggered.connect(self.show_help)
        help_menu.addAction(help_action)

    def create_new_window(self):
        """Create a new window"""
        if self.window_manager:
            self.window_manager.create_window()
        else:
            # Fallback if no window manager (shouldn't happen)
            new_window = MainWindow()
            new_window.show()

    def browse_file(self):
        """Open file browser dialog"""
        file_path, _ = QFileDialog.getOpenFileName(
            self,
            t("dialog.open_file"),
            "",
            "Data Files (*.parquet *.csv *.json);;Parquet Files (*.parquet);;CSV Files (*.csv);;JSON Files (*.json);;All Files (*)"
        )

        if file_path:
            # Open file in new window if window manager is available
            if self.window_manager:
                self.window_manager.create_window(file_path)
            else:
                # Fallback: load in current window
                self.file_path_edit.setText(file_path)
                self.load_file(file_path)

    def load_file_from_input(self):
        """Load file from path in input field"""
        file_path = self.file_path_edit.text().strip()
        if file_path:
            self.load_file(file_path)
        else:
            QMessageBox.warning(self, t("error.no_file"), t("error.no_file_msg"))

    def load_file(self, file_path: str):
        """
        Load file into engine

        Args:
            file_path: Path to file
        """
        file_path = Path(file_path)

        if not file_path.exists():
            QMessageBox.critical(self, t("error.file_not_found"), t("error.file_not_found_msg", path=file_path))
            return

        try:
            # Close existing engine
            if self.engine:
                self.engine.close()

            # Create new engine
            page_size = int(settings.result_pagination_rows_per_page)
            self.engine = QueryEngine(file_path, page_size)

            # Update UI
            self.file_path_edit.setText(str(file_path))
            self.current_page = 1

            # Update SQL editor completions
            columns = self.engine.get_columns()
            self.sql_editor.update_completions(columns)

            # Enable buttons
            self.execute_btn.setEnabled(True)
            self.reset_btn.setEnabled(True)
            self.table_info_btn.setEnabled(True)

            # Add to recents
            if settings.save_file_history in ("True", "true", "1", True, 1):
                recents.add_recent(str(file_path))
                self.update_recents_menu()

            # Load first page
            self.load_page()

            self.statusBar().showMessage(t("status.loaded", filename=file_path.name, rows=self.engine.total_rows))
            logger.info(f"File loaded: {file_path}")

        except Exception as e:
            QMessageBox.critical(self, t("error.load_error"), t("error.load_error_msg", error=str(e)))
            logger.error(f"Failed to load file: {e}")

    def load_page(self, query: str = None):
        """
        Load current page data

        Args:
            query: Optional SQL query to execute first
        """
        if not self.engine:
            return

        # Show loading status
        self.statusBar().showMessage(t("status.loading"))

        # Create worker thread
        self.worker = QueryWorker(self.engine, self.current_page, query)
        self.worker.finished.connect(self.on_page_loaded)
        self.worker.error.connect(self.on_query_error)
        self.worker.start()

    def on_page_loaded(self, df: pd.DataFrame):
        """Handle page load completion"""
        self.table_view.load_data(df)
        self.update_pagination_ui()
        self.statusBar().showMessage(t("status.page_info", page=self.current_page, total_pages=self.engine.total_pages))

    def on_query_error(self, error_msg: str):
        """Handle query execution error"""
        QMessageBox.critical(self, t("error.query_error"), t("error.query_error_msg", error_msg=error_msg))
        self.statusBar().showMessage(t("status.query_failed"))
        logger.error(f"Query error: {error_msg}")

    def execute_query(self):
        """Execute SQL query from editor"""
        if not self.engine:
            return

        query = self.sql_editor.get_query()
        if not query:
            QMessageBox.warning(self, t("error.empty_query"), t("error.empty_query_msg"))
            return

        self.current_page = 1
        self.load_page(query)

    def reset_query(self):
        """Reset to original table view"""
        if not self.engine:
            return

        self.engine.reset_query()
        self.sql_editor.set_query(settings.render_vars(settings.default_sql_query))
        self.current_page = 1
        self.load_page()

    def prev_page(self):
        """Navigate to previous page"""
        if self.current_page > 1:
            self.current_page -= 1
            self.load_page()

    def next_page(self):
        """Navigate to next page"""
        if self.current_page < self.engine.total_pages:
            self.current_page += 1
            self.load_page()

    def update_pagination_ui(self):
        """Update pagination buttons and label"""
        if not self.engine:
            return

        total_pages = self.engine.total_pages
        self.page_label.setText(f"Page {self.current_page} of {total_pages:,} ({self.engine.total_rows:,} rows)")

        self.prev_btn.setEnabled(self.current_page > 1)
        self.next_btn.setEnabled(self.current_page < total_pages)

    def on_sort_requested(self, column: str, ascending: bool):
        """Handle sort request from table view"""
        if not self.engine:
            return

        success, error_msg = self.engine.sort_by_column(column, ascending)
        if success:
            self.current_page = 1
            self.load_page()
            direction = "ascending" if ascending else "descending"
            self.statusBar().showMessage(t("status.sorted", column=column, direction=direction))
        else:
            QMessageBox.warning(self, t("error.sort_failed"), t("error.sort_failed_msg", column=column, error_msg=error_msg))

    def on_unique_values_requested(self, column: str):
        """Handle unique values request from table view"""
        if not self.engine:
            return

        # Check if warning is needed based on settings
        if settings.enable_large_dataset_warning:
            should_warn = False
            warning_message = ""

            if settings.warning_criteria == "rows":
                if self.engine.total_rows > settings.warning_threshold_rows:
                    should_warn = True
                    warning_message = (
                        f"This dataset has {self.engine.total_rows:,} rows "
                        f"(threshold: {settings.warning_threshold_rows:,}).\n"
                        f"Calculating unique values may take some time.\n\n"
                        f"Continue?"
                    )

            elif settings.warning_criteria == "cells":
                num_columns = len(self.engine.get_columns())
                total_cells = self.engine.total_rows * num_columns
                if total_cells > settings.warning_threshold_cells:
                    should_warn = True
                    warning_message = (
                        f"This dataset has {total_cells:,} cells "
                        f"({self.engine.total_rows:,} rows × {num_columns} columns, "
                        f"threshold: {settings.warning_threshold_cells:,}).\n"
                        f"Calculating unique values may take some time.\n\n"
                        f"Continue?"
                    )

            elif settings.warning_criteria == "filesize":
                file_size_mb = self.engine.file_path.stat().st_size / (1024 * 1024)
                if file_size_mb > settings.warning_threshold_filesize_mb:
                    should_warn = True
                    warning_message = (
                        f"This file is {file_size_mb:.1f} MB "
                        f"(threshold: {settings.warning_threshold_filesize_mb} MB).\n"
                        f"Calculating unique values may take some time.\n\n"
                        f"Continue?"
                    )

            if should_warn:
                reply = QMessageBox.question(
                    self,
                    t("warning.large_dataset"),
                    warning_message,
                    QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
                )

                if reply == QMessageBox.StandardButton.No:
                    return

        # Show progress dialog
        progress = QProgressDialog(t("status.calculating"), t("btn.cancel"), 0, 0, self)
        progress.setWindowModality(Qt.WindowModality.WindowModal)
        progress.show()

        try:
            unique_values = self.engine.get_unique_values(column)
            progress.close()

            if unique_values:
                dialog = UniqueValuesDialog(column, unique_values, self)
                dialog.values_selected.connect(self.on_filter_by_values)
                dialog.exec()
            else:
                QMessageBox.information(self, t("error.no_values"), t("error.no_values_msg", column=column))

        except Exception as e:
            progress.close()
            QMessageBox.critical(self, t("error.unique_values_error"), t("error.unique_values_error_msg", error=str(e)))

    def on_filter_by_values(self, column: str, values: list):
        """Filter table by multiple column values"""
        # Build query with IN clause for multiple values
        if len(values) == 1:
            # Single value - use simple equality
            query = f"SELECT * FROM {self.engine.table_name} WHERE {column} = '{values[0]}'"
        else:
            # Multiple values - use IN clause
            values_str = ", ".join(f"'{v}'" for v in values)
            query = f"SELECT * FROM {self.engine.table_name} WHERE {column} IN ({values_str})"

        self.sql_editor.set_query(query)
        self.execute_query()

    def export_results(self):
        """Export current query results"""
        if not self.engine:
            QMessageBox.warning(self, t("warning.no_data"), t("warning.no_data_msg"))
            return

        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "Export Results",
            "",
            "CSV Files (*.csv);;Parquet Files (*.parquet);;JSON Files (*.json);;All Files (*)"
        )

        if file_path:
            try:
                success = self.engine.export_results(file_path)
                if success:
                    QMessageBox.information(self, t("success.export_complete"), t("success.export_complete_msg", path=file_path))
                else:
                    QMessageBox.critical(self, t("success.export_failed"), t("success.export_failed_msg"))
            except Exception as e:
                QMessageBox.critical(self, t("error.export_error"), t("error.export_error_msg", error=str(e)))

    def show_table_info(self):
        """Show table information dialog"""
        if not self.engine:
            return

        info = self.engine.get_table_info()
        columns = self.engine.get_column_types()

        # Build markdown info
        md_text = f"# Table Information\n\n"
        md_text += f"**File:** {info['file_path']}\n\n"
        md_text += f"**Total Rows:** {info['total_rows']:,}\n\n"
        md_text += f"**Total Pages:** {info['total_pages']:,}\n\n"
        md_text += f"**Page Size:** {info['page_size']}\n\n"
        md_text += f"## Columns ({len(columns)})\n\n"
        md_text += "| Column | Type |\n"
        md_text += "|--------|------|\n"
        for col_name, col_type in columns:
            md_text += f"| {col_name} | {col_type} |\n"

        # Show dialog
        dialog = QDialog(self)
        dialog.setWindowTitle(t("dialog.table_info"))
        dialog.resize(600, 500)

        layout = QVBoxLayout()
        browser = QTextBrowser()
        browser.setMarkdown(md_text)
        layout.addWidget(browser)

        dialog.setLayout(layout)
        dialog.exec()

    def edit_settings(self):
        """Open settings editor with theme selection"""
        current_theme = self.current_theme.name if self.current_theme else None
        dialog = SettingsDialog(current_theme, self)
        dialog.theme_changed.connect(self.change_theme)
        dialog.settings_changed.connect(self.on_settings_changed)
        dialog.exec()

    def on_settings_changed(self):
        """Handle settings changes"""
        # Reload settings
        self.statusBar().showMessage("Settings saved. Some changes may require restart.", 5000)

    def update_recents_menu(self):
        """Update recent files menu"""
        self.recents_menu.clear()

        for recent in recents.recents:
            action = QAction(recent, self)
            action.triggered.connect(lambda checked, path=recent: self.load_recent(path))
            self.recents_menu.addAction(action)

        if recents.recents:
            self.recents_menu.addSeparator()
            clear_action = QAction(t("menu.file.clear_recents"), self)
            clear_action.triggered.connect(self.clear_recents)
            self.recents_menu.addAction(clear_action)

    def load_recent(self, file_path: str):
        """Load a recent file"""
        if Path(file_path).exists():
            # Open recent file in new window if window manager is available
            if self.window_manager:
                self.window_manager.create_window(file_path)
            else:
                # Fallback: load in current window
                self.load_file(file_path)
        else:
            reply = QMessageBox.question(
                self,
                t("error.file_not_found"),
                t("warning.file_not_found_recent", path=file_path),
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No
            )

            if reply == QMessageBox.StandardButton.Yes:
                recents.recents.remove(file_path)
                recents.save_recents()
                self.update_recents_menu()

    def clear_recents(self):
        """Clear recent files list"""
        recents.recents = []
        recents.save_recents()
        self.update_recents_menu()

    def show_help(self):
        """Show help/about dialog"""
        # Build help text from translations
        help_text = f"""
# {t("about.title")}

{t("about.description")}

## {t("about.features")}

- {t("about.feature.lazy")}
- {t("about.feature.sql")}
- {t("about.feature.autocomplete")}
- {t("about.feature.pagination")}
- {t("about.feature.editing")}
- {t("about.feature.operations")}
- {t("about.feature.export")}
- {t("about.feature.themes")}

## {t("about.quick_start")}

{t("about.step1")}
{t("about.step2")}
{t("about.step3")}
{t("about.step3_sub")}
{t("about.step4")}
{t("about.step5")}
{t("about.step6")}
{t("about.step7")}

## {t("about.sql_reference")}

{t("about.sql_description")}

```sql
-- View all data
SELECT * FROM data

-- Filter rows
SELECT * FROM data WHERE age > 25

-- Aggregate data
SELECT category, COUNT(*) as count
FROM data GROUP BY category

-- Multiple value filter
SELECT * FROM data WHERE status IN ('active', 'pending')
```

For complete SQL documentation, visit:
**[DuckDB SQL Query Syntax](https://duckdb.org/docs/stable/sql/query_syntax/select)**

## {t("about.shortcuts")}

- {t("about.shortcuts.tab")}
- {t("about.shortcuts.double")}
- {t("about.shortcuts.right")}

## {t("about.resources")}

- **GitHub Repository**: [https://github.com/AzizNadirov/ParVu](https://github.com/AzizNadirov/ParVu)
- **DuckDB Documentation**: [https://duckdb.org/docs/stable/sql/query_syntax/select](https://duckdb.org/docs/stable/sql/query_syntax/select)
- **Report Issues**: [GitHub Issues](https://github.com/AzizNadirov/ParVu/issues)

---

**{t("about.version")}**: {t("about.version_number")}
**{t("about.built_with")}**: {t("about.built_with_text")}
**{t("about.license")}**: {t("about.license_text")}
"""

        dialog = QDialog(self)
        dialog.setWindowTitle(t("dialog.about"))
        dialog.resize(700, 650)

        layout = QVBoxLayout()
        browser = QTextBrowser()
        browser.setOpenExternalLinks(True)  # Enable clickable links
        browser.setMarkdown(help_text)
        layout.addWidget(browser)

        close_btn = QPushButton(t("btn.close"))
        close_btn.clicked.connect(dialog.close)
        layout.addWidget(close_btn)

        dialog.setLayout(layout)
        dialog.exec()

    def load_theme(self):
        """Load theme from settings"""
        # Try to load saved theme name from settings
        theme_name = getattr(settings, 'current_theme', 'ParVu Light')

        # Set theme in theme manager
        if not theme_manager.set_theme(theme_name):
            # Fallback to default theme
            theme_manager.set_theme('ParVu Light')

        self.current_theme = theme_manager.current_theme
        logger.info(f"Loaded theme: {self.current_theme.name if self.current_theme else 'None'}")

    def apply_current_theme(self):
        """Apply current theme to the application"""
        if not self.current_theme:
            return

        # Apply stylesheet
        stylesheet = self.current_theme.generate_stylesheet()
        self.setStyleSheet(stylesheet)

        # Apply window size constraints from theme
        self.setMinimumSize(
            self.current_theme.layout.window_min_width,
            self.current_theme.layout.window_min_height
        )

        logger.info(f"Applied theme: {self.current_theme.name}")

    def change_theme(self, theme_name: str):
        """Change application theme"""
        if theme_manager.set_theme(theme_name):
            self.current_theme = theme_manager.current_theme

            # Apply new theme
            self.apply_current_theme()

            # Update components
            if hasattr(self, 'sql_editor'):
                self.sql_editor.update_theme(self.current_theme)

            if hasattr(self, 'table_view'):
                self.table_view.update_theme(self.current_theme)

            # Save theme preference
            self.save_theme_preference(theme_name)

            self.statusBar().showMessage(f"Theme changed to: {theme_name}", 3000)
            logger.info(f"Theme changed to: {theme_name}")

    def save_theme_preference(self, theme_name: str):
        """Save current theme to settings"""
        try:
            # Update settings object
            settings.current_theme = theme_name
            # Save to file
            settings.save_settings()
            logger.info(f"Saved theme preference: {theme_name}")
        except Exception as e:
            logger.error(f"Failed to save theme preference: {e}")

    def closeEvent(self, event):
        """Handle window close"""
        if self.engine:
            self.engine.close()

        # Notify window manager
        if self.window_manager:
            self.window_manager.remove_window(self)

        event.accept()
