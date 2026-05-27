"""
Dual Query Editor — combines SQL editor and DSL Expression editor.

Users can toggle between:
- SQL mode: free-form SQL with syntax highlighting
- Expression mode: DAX-like DSL with autocomplete + live SQL preview
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QStackedWidget,
)
from PyQt6.QtCore import Qt, pyqtSignal
from loguru import logger

from parvu.infrastructure.themes.models import Theme
from parvu.presentation.widgets.sql_editor import SQLEditor
from parvu.presentation.widgets.expression_editor import ExpressionEditor
from parvu.core.dsl.catalog import Catalog
from parvu.core.dsl.registry import FunctionRegistry
from parvu.core.dsl.parser import DSLParser
from parvu.core.dsl.resolver import Resolver
from parvu.core.dsl.compiler import Compiler


class QueryEditor(QWidget):
    """Widget that hosts both SQL and DSL expression editors with a mode toggle."""

    mode_changed = pyqtSignal(bool)  # is_expression_mode

    def __init__(
        self,
        catalog: Catalog | None = None,
        registry: FunctionRegistry | None = None,
        theme: Theme | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self._catalog = catalog
        self._registry = registry or FunctionRegistry()
        self._parser = DSLParser()
        self._resolver = Resolver(catalog, self._registry) if catalog else None
        self._compiler = Compiler(self._registry)
        self._theme = theme

        self._is_expression_mode = False
        self._setup_ui()
        self._update_expr_button_state()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(4)

        # Mode toggle row
        toggle_row = QHBoxLayout()
        self._mode_label = QLabel("Mode:")
        toggle_row.addWidget(self._mode_label)

        self._sql_btn = QPushButton("SQL")
        self._sql_btn.setCheckable(True)
        self._sql_btn.setChecked(True)
        self._sql_btn.clicked.connect(self._set_sql_mode)
        toggle_row.addWidget(self._sql_btn)

        self._expr_btn = QPushButton("Expression")
        self._expr_btn.setCheckable(True)
        self._expr_btn.setEnabled(False)
        self._expr_btn.setToolTip("Load a data file to enable expression mode")
        self._expr_btn.clicked.connect(self._set_expr_mode)
        toggle_row.addWidget(self._expr_btn)

        toggle_row.addStretch()
        layout.addLayout(toggle_row)

        # Stacked editor
        self._stack = QStackedWidget()

        # SQL editor
        self._sql_editor = SQLEditor(theme=self._theme)
        self._stack.addWidget(self._sql_editor)

        # Expression editor container
        expr_container = QWidget()
        expr_layout = QVBoxLayout(expr_container)
        expr_layout.setContentsMargins(0, 0, 0, 0)
        expr_layout.setSpacing(2)

        self._expr_editor = ExpressionEditor(
            self._catalog or Catalog(),
            self._registry,
        )
        self._expr_editor.setPlaceholderText(
            "Type a DSL expression, e.g. SUM(sales[revenue]) / COUNT(sales[id])"
        )
        expr_layout.addWidget(self._expr_editor)

        self._preview_label = QLabel("Preview: (enter an expression)")
        self._preview_label.setStyleSheet(
            "QLabel { color: #666; font-family: monospace; font-size: 11px; }"
        )
        self._preview_label.setWordWrap(True)
        expr_layout.addWidget(self._preview_label)

        self._stack.addWidget(expr_container)
        layout.addWidget(self._stack)

        # Live preview on text change
        self._expr_editor.textChanged.connect(self._update_preview)

    # ------------------------------------------------------------------
    # Mode switching
    # ------------------------------------------------------------------

    def _update_expr_button_state(self) -> None:
        """Enable/disable Expression button based on catalog availability."""
        has_catalog = self._catalog is not None and bool(self._catalog.tables())
        self._expr_btn.setEnabled(has_catalog)
        self._expr_btn.setToolTip(
            "" if has_catalog else "Load a data file to enable expression mode"
        )

    def _set_sql_mode(self) -> None:
        self._is_expression_mode = False
        self._sql_btn.setChecked(True)
        self._expr_btn.setChecked(False)
        self._stack.setCurrentIndex(0)
        self.mode_changed.emit(False)
        logger.debug("QueryEditor switched to SQL mode")

    def _set_expr_mode(self) -> None:
        if self._catalog is None or not self._catalog.tables():
            # Can't use expression mode without a catalog
            self._set_sql_mode()
            return
        self._is_expression_mode = True
        self._sql_btn.setChecked(False)
        self._expr_btn.setChecked(True)
        self._stack.setCurrentIndex(1)
        self.mode_changed.emit(True)
        logger.debug("QueryEditor switched to Expression mode")

    # ------------------------------------------------------------------
    # Expression preview
    # ------------------------------------------------------------------

    def _update_preview(self) -> None:
        text = self._expr_editor.text().strip()
        if not text or self._resolver is None:
            self._preview_label.setText("Preview: (enter an expression)")
            return
        try:
            tree = self._parser.parse(text)
            expr = self._resolver.resolve(tree)
            sql = self._compiler.compile(expr)
            self._preview_label.setText(f"Preview: {sql}")
        except Exception as e:
            self._preview_label.setText(f"Preview: (invalid — {e})")

    # ------------------------------------------------------------------
    # Public API (mirrors SQLEditor)
    # ------------------------------------------------------------------

    def get_query(self) -> str:
        """Return the current query text."""
        if self._is_expression_mode:
            expr_text = self._expr_editor.text().strip()
            if not expr_text or self._resolver is None:
                return ""
            try:
                tree = self._parser.parse(expr_text)
                expr = self._resolver.resolve(tree)
                compiled = self._compiler.compile(expr)
                # DSL expressions are scalar — wrap them in a SELECT
                if self._catalog and self._catalog.tables():
                    table = self._catalog.tables()[0]
                    return f"SELECT {compiled} FROM {table}"
                return compiled
            except Exception as e:
                logger.warning(f"Expression compilation failed: {e}")
                return ""
        return self._sql_editor.get_query()

    def set_query(self, query: str) -> None:
        """Set the query text."""
        if self._is_expression_mode:
            self._expr_editor.setText(query)
        else:
            self._sql_editor.set_query(query)

    def update_completions(self, column_names: list[str], table_name: str = "") -> None:
        """Update completion lists for both editors."""
        self._sql_editor.update_completions(column_names, table_name)

    def apply_theme(self, theme: Theme) -> None:
        """Apply theme to editors."""
        self._sql_editor.apply_theme(theme)

    def set_expression_catalog(self, catalog: Catalog) -> None:
        """Update the catalog used for expression autocomplete."""
        self._catalog = catalog
        self._resolver = Resolver(catalog, self._registry)
        self._expr_editor._catalog = catalog
        self._update_expr_button_state()

    @property
    def is_expression_mode(self) -> bool:
        return self._is_expression_mode
