"""
Expression Editor Widget — QPlainTextEdit with DSL-aware auto-completion.

Provides context-sensitive suggestions based on the ParVu DSL grammar:
- table[… → column names for that table
- expr.… → method names from the FunctionRegistry
- top-level → functions and table names

Also shows a documentation popup when the user navigates the completer
or clicks on a recognised function / method name in the editor.
"""
from __future__ import annotations

import re

from PyQt6.QtWidgets import QPlainTextEdit, QCompleter
from PyQt6.QtCore import Qt, QStringListModel, QPoint
from loguru import logger

from parvu.core.dsl.catalog import Catalog
from parvu.core.dsl.registry import FunctionRegistry
from parvu.infrastructure.themes.models import Theme
from parvu.presentation.widgets.function_doc_popup import FunctionDocPopup


class ExpressionEditor(QPlainTextEdit):
    """Multi-line expression editor with context-aware auto-completion."""

    def __init__(
        self,
        catalog: Catalog,
        registry: FunctionRegistry,
        theme: Theme | None = None,
        parent=None,
    ):
        super().__init__(parent)
        self.setMinimumHeight(60)
        self.setLineWrapMode(QPlainTextEdit.LineWrapMode.NoWrap)
        self._catalog = catalog
        self._registry = registry
        self._theme = theme

        self._model = QStringListModel()
        self._completer = QCompleter(self._model, self)
        self._completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        self._completer.setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
        self._completer.activated.connect(self._insert_completion)
        self._completer.setWidget(self)

        # Documentation popup
        self._doc_popup = FunctionDocPopup(self, theme)
        self._completer.highlighted[str].connect(self._on_completion_highlighted)

        self.textChanged.connect(self._on_text_changed)
        self.setPlaceholderText("Enter expression...")

    # ------------------------------------------------------------------
    # Completion logic
    # ------------------------------------------------------------------

    def _on_text_changed(self) -> None:
        """Update the completer model whenever text changes."""
        text = self.toPlainText()
        pos = self.textCursor().position()
        prefix, suggestions = self._get_suggestions(text, pos)

        if not suggestions or len(prefix) < 1:
            self._completer.popup().hide()
            self._doc_popup.hide_popup()
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
            self.insertPlainText(completion[-extra:])
            logger.debug(f"Autocomplete inserted: '{completion}'")
        self._doc_popup.hide_popup()

    # ------------------------------------------------------------------
    # Documentation popup
    # ------------------------------------------------------------------

    def _on_completion_highlighted(self, text: str) -> None:
        """Show documentation for the highlighted completer item."""
        func = self._resolve_doc_function(text)
        if func is None:
            self._doc_popup.hide_popup()
            return

        popup = self._completer.popup()
        pos = popup.mapToGlobal(popup.rect().topRight() + QPoint(2, 0))
        self._doc_popup.show_for_function(func, pos)

    def _resolve_doc_function(self, text: str):
        """Try to find a FunctionDef for *text* (completion string or editor word)."""
        if text.endswith("("):
            return self._registry.lookup(text[:-1])
        if text.endswith("["):
            return None
        # Could be a function or a method
        func = self._registry.lookup(text)
        if func is not None:
            return func
        return self._registry.lookup_method(text)

    def _show_doc_at_cursor(self) -> None:
        """Show documentation for the word under the text cursor, if any."""
        text = self.toPlainText()
        pos = self.textCursor().position()

        # Find word boundaries
        start = pos
        while start > 0 and (text[start - 1].isalnum() or text[start - 1] == "_"):
            start -= 1
        end = pos
        while end < len(text) and (text[end].isalnum() or text[end] == "_"):
            end += 1

        word = text[start:end]
        if not word:
            self._doc_popup.hide_popup()
            return

        func = self._resolve_doc_function(word)
        if func is not None:
            cursor_rect = self.cursorRect()
            global_pos = self.mapToGlobal(cursor_rect.bottomLeft() + QPoint(0, 2))
            self._doc_popup.show_for_function(func, global_pos)
        else:
            self._doc_popup.hide_popup()

    def apply_theme(self, theme: Theme) -> None:
        """Update the documentation popup theme."""
        self._theme = theme
        self._doc_popup.apply_theme(theme)

    # ------------------------------------------------------------------
    # Events
    # ------------------------------------------------------------------

    def mousePressEvent(self, event):
        super().mousePressEvent(event)
        self._show_doc_at_cursor()

    def keyPressEvent(self, event):
        if event.key() == Qt.Key.Key_Escape:
            self._doc_popup.hide_popup()
        super().keyPressEvent(event)

    def focusOutEvent(self, event):
        self._doc_popup.hide_popup()
        super().focusOutEvent(event)
