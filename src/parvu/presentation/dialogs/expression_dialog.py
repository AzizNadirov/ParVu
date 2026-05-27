"""
Expression Dialog — visual expression builder with live SQL preview.

Replaces raw SQL input for Add Column / Math Operation with the
ParVu DSL: users type DAX-like expressions and see the compiled
DuckDB SQL in real time.
"""
from __future__ import annotations

from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QHBoxLayout, QLabel, QLineEdit,
    QPushButton, QMessageBox,
)
from PyQt6.QtCore import Qt
from loguru import logger

from parvu.core.dsl.parser import DSLParser, ParseError
from parvu.core.dsl.resolver import Resolver, ResolutionError
from parvu.core.dsl.compiler import Compiler, CompileError
from parvu.core.dsl.catalog import Catalog
from parvu.core.dsl.registry import FunctionRegistry
from parvu.presentation.widgets.expression_editor import ExpressionEditor


class ExpressionDialog(QDialog):
    """Dialog for entering a ParVu DSL expression."""

    def __init__(
        self,
        catalog: Catalog,
        registry: FunctionRegistry,
        title: str = "Expression",
        default_expr: str = "",
        parent=None,
    ):
        super().__init__(parent)
        self._catalog = catalog
        self._registry = registry
        self._parser = DSLParser()
        self._resolver = Resolver(catalog, registry)
        self._compiler = Compiler(registry)

        self.setWindowTitle(title)
        self.setMinimumWidth(500)
        self._setup_ui()

        if default_expr:
            self._expr_edit.setText(default_expr)
            self._update_preview()

    def _setup_ui(self) -> None:
        layout = QVBoxLayout(self)
        layout.setSpacing(10)

        # Column name
        name_row = QHBoxLayout()
        name_row.addWidget(QLabel("Column name:"))
        self._name_edit = QLineEdit()
        name_row.addWidget(self._name_edit)
        layout.addLayout(name_row)

        # Expression
        layout.addWidget(QLabel("Expression:"))
        self._expr_edit = ExpressionEditor(self._catalog, self._registry)
        layout.addWidget(self._expr_edit)

        # Live preview
        layout.addWidget(QLabel("Preview SQL:"))
        self._preview = QLabel("(enter an expression)")
        self._preview.setWordWrap(True)
        self._preview.setTextInteractionFlags(Qt.TextInteractionFlag.TextSelectableByMouse)
        self._preview.setStyleSheet("QLabel { color: #555; font-family: monospace; }")
        layout.addWidget(self._preview)

        # Error label
        self._error_label = QLabel()
        self._error_label.setStyleSheet("QLabel { color: #c00; }")
        self._error_label.setWordWrap(True)
        layout.addWidget(self._error_label)

        layout.addStretch()

        # Buttons
        btn_row = QHBoxLayout()
        btn_row.addStretch()

        cancel_btn = QPushButton("Cancel")
        cancel_btn.clicked.connect(self.reject)
        btn_row.addWidget(cancel_btn)

        ok_btn = QPushButton("OK")
        ok_btn.clicked.connect(self._on_ok)
        btn_row.addWidget(ok_btn)

        layout.addLayout(btn_row)

        # Live preview on text change
        self._expr_edit.textChanged.connect(self._update_preview)

    def _update_preview(self) -> None:
        """Compile the current expression and show the SQL preview."""
        text = self._expr_edit.text().strip()
        if not text:
            self._preview.setText("(enter an expression)")
            self._error_label.clear()
            return

        try:
            tree = self._parser.parse(text)
            expr = self._resolver.resolve(tree)
            sql = self._compiler.compile(expr)
            self._preview.setText(sql)
            self._error_label.clear()
        except (ParseError, ResolutionError, CompileError) as e:
            logger.debug(f"Expression preview error: {e}")
            self._preview.setText("(invalid expression)")
            self._error_label.setText(str(e))
        except Exception as e:
            logger.warning(f"Expression preview unexpected error: {e}")
            self._preview.setText("(invalid expression)")
            self._error_label.setText(f"Unexpected error: {e}")

    def _on_ok(self) -> None:
        """Validate before accepting."""
        name = self._name_edit.text().strip()
        expr_text = self._expr_edit.text().strip()

        if not name:
            QMessageBox.warning(self, "Validation", "Please enter a column name.")
            return
        if not expr_text:
            QMessageBox.warning(self, "Validation", "Please enter an expression.")
            return

        try:
            tree = self._parser.parse(expr_text)
            self._resolver.resolve(tree)
        except Exception as e:
            logger.warning(f"Expression validation failed for '{name}': {e}")
            QMessageBox.warning(self, "Validation", f"Expression is invalid:\n{e}")
            return

        logger.info(f"Expression dialog accepted: column='{name}', expr='{expr_text[:40]}...'")
        self.accept()

    def get_result(self) -> tuple[str, str] | None:
        """Return (column_name, compiled_sql) or None if invalid."""
        name = self._name_edit.text().strip()
        expr_text = self._expr_edit.text().strip()
        if not name or not expr_text:
            return None

        try:
            tree = self._parser.parse(expr_text)
            expr = self._resolver.resolve(tree)
            sql = self._compiler.compile(expr)
            return name, sql
        except Exception:
            return None
