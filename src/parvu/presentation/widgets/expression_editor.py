"""
Expression Editor Widget — QLineEdit with DSL-aware auto-completion.

Provides context-sensitive suggestions based on the ParVu DSL grammar:
- table[… → column names for that table
- expr.… → method names from the FunctionRegistry
- top-level → functions and table names
"""
from __future__ import annotations

import re

from PyQt6.QtWidgets import QLineEdit, QCompleter
from PyQt6.QtCore import Qt, QStringListModel
from loguru import logger

from parvu.core.dsl.catalog import Catalog
from parvu.core.dsl.registry import FunctionRegistry


class ExpressionEditor(QLineEdit):
    """Single-line expression editor with context-aware auto-completion."""

    def __init__(
        self,
        catalog: Catalog,
        registry: FunctionRegistry,
        parent=None,
    ):
        super().__init__(parent)
        self._catalog = catalog
        self._registry = registry

        self._model = QStringListModel()
        self._completer = QCompleter(self._model, self)
        self._completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        self._completer.setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
        self._completer.activated.connect(self._insert_completion)
        self._completer.setWidget(self)

        self.textChanged.connect(self._on_text_changed)

    # ------------------------------------------------------------------
    # Completion logic
    # ------------------------------------------------------------------

    def _on_text_changed(self) -> None:
        """Update the completer model whenever text changes."""
        text = self.text()
        pos = self.cursorPosition()
        prefix, suggestions = self._get_suggestions(text, pos)

        if not suggestions or len(prefix) < 1:
            self._completer.popup().hide()
            return

        self._model.setStringList(suggestions)
        self._completer.setCompletionPrefix(prefix)
        logger.debug(f"Autocomplete: prefix='{prefix}', {len(suggestions)} suggestions")
        self._completer.complete()

    def _get_suggestions(self, text: str, pos: int) -> tuple[str, list[str]]:
        """Return (prefix, [suggestion, …]) for the cursor position."""
        before = text[:pos]

        # 1. Inside brackets: table[partial_col or table["partial_col
        m = re.search(r"\[(\"?)(\w*)$", before)
        if m:
            quote = m.group(1)
            prefix = m.group(2)
            bracket_start = m.start()
            table_match = re.search(r"(\w+)\[$", before[: bracket_start + 1])
            if table_match:
                table = table_match.group(1)
                if table in self._catalog.tables():
                    cols = self._catalog.columns(table)
                    suggestions = [
                        f'{table}[{quote}{c}{quote}]'
                        for c in cols
                        if c.lower().startswith(prefix.lower())
                    ]
                    return prefix, suggestions
            return prefix, []

        # 2. After a dot: expr.partial_method
        m = re.search(r"\.(\w*)$", before)
        if m:
            prefix = m.group(1)
            methods = self._registry.all_methods()
            suggestions = [
                mth for mth in methods if mth.lower().startswith(prefix.lower())
            ]
            return prefix, suggestions

        # 3. Top-level: partial word → functions + tables
        m = re.search(r"(\w+)$", before)
        if m:
            prefix = m.group(1)
            pfx_lower = prefix.lower()
            suggestions: list[str] = []

            for func in self._registry.all_functions():
                if func.name.lower().startswith(pfx_lower):
                    suggestions.append(func.name + "(")

            for table in self._catalog.tables():
                if table.lower().startswith(pfx_lower):
                    suggestions.append(table + "[")

            return prefix, suggestions

        return "", []

    def _insert_completion(self, completion: str) -> None:
        """Insert the selected completion at the cursor."""
        prefix = self._completer.completionPrefix()
        extra = len(completion) - len(prefix)
        if extra > 0:
            self.insert(completion[-extra:])
            logger.debug(f"Autocomplete inserted: '{completion}'")
