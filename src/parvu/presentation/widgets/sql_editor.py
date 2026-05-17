"""
SQL Editor Widget with Syntax Highlighting and Auto-Completion.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QTextEdit, QCompleter
from PyQt6.QtGui import QSyntaxHighlighter, QTextCharFormat, QColor, QFont, QTextCursor
from PyQt6.QtCore import Qt, QRegularExpression, QStringListModel

from parvu.infrastructure.themes.models import Theme


class SQLSyntaxHighlighter(QSyntaxHighlighter):
    """Syntax highlighter for SQL with keyword highlighting."""

    def __init__(self, parent=None, theme: Theme | None = None):
        super().__init__(parent)
        self._theme = theme
        self._highlighting_rules: list[tuple[QRegularExpression, QTextCharFormat]] = []
        self._setup_highlighting()

    def _setup_highlighting(self) -> None:
        self._highlighting_rules.clear()

        if self._theme:
            keyword_color = self._theme.colors.editor_keyword
            string_color = self._theme.colors.editor_string
            number_color = self._theme.colors.editor_number
            comment_color = self._theme.colors.editor_comment
        else:
            keyword_color = "#0066CC"
            string_color = "#00AA00"
            number_color = "#AA00AA"
            comment_color = "#808080"

        keyword_format = QTextCharFormat()
        keyword_format.setForeground(QColor(keyword_color))
        keyword_format.setFontWeight(QFont.Weight.Bold)

        for keyword in self._get_keywords():
            pattern = QRegularExpression(
                f"\\b{keyword}\\b",
                QRegularExpression.PatternOption.CaseInsensitiveOption,
            )
            self._highlighting_rules.append((pattern, keyword_format))

        string_format = QTextCharFormat()
        string_format.setForeground(QColor(string_color))
        self._highlighting_rules.append(
            (QRegularExpression("'[^']*'"), string_format)
        )

        number_format = QTextCharFormat()
        number_format.setForeground(QColor(number_color))
        self._highlighting_rules.append(
            (QRegularExpression("\\b[0-9]+\\.?[0-9]*\\b"), number_format)
        )

        comment_format = QTextCharFormat()
        comment_format.setForeground(QColor(comment_color))
        comment_format.setFontItalic(True)
        self._highlighting_rules.append(
            (QRegularExpression("--[^\\n]*"), comment_format)
        )

    def _get_keywords(self) -> list[str]:
        """Get SQL keywords. Override or extend as needed."""
        return [
            "SELECT", "FROM", "WHERE", "AND", "OR", "NOT", "INSERT", "UPDATE",
            "DELETE", "CREATE", "DROP", "TABLE", "INDEX", "VIEW", "JOIN",
            "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "ON", "GROUP", "BY",
            "ORDER", "HAVING", "LIMIT", "OFFSET", "UNION", "ALL", "DISTINCT",
            "AS", "IN", "BETWEEN", "LIKE", "IS", "NULL", "CASE", "WHEN",
            "THEN", "ELSE", "END", "ASC", "DESC", "COUNT", "SUM", "AVG",
            "MIN", "MAX", "CAST", "EXPLAIN", "DESCRIBE", "SHOW", "PRAGMA",
        ]

    def update_theme(self, theme: Theme) -> None:
        self._theme = theme
        self._setup_highlighting()
        self.rehighlight()

    def highlightBlock(self, text: str) -> None:
        for pattern, fmt in self._highlighting_rules:
            iterator = pattern.globalMatch(text)
            while iterator.hasNext():
                match = iterator.next()
                self.setFormat(
                    match.capturedStart(), match.capturedLength(), fmt
                )


class SQLEditor(QTextEdit):
    """SQL Editor with auto-completion and syntax highlighting."""

    def __init__(self, parent=None, theme: Theme | None = None):
        super().__init__(parent)
        self._column_names: list[str] = []
        self._theme = theme

        font = QFont(
            theme.layout.code_font_family if theme else "Courier New",
            theme.layout.code_font_size if theme else 12,
        )
        self.setFont(font)
        self.setPlaceholderText("Enter SQL query...")

        self._highlighter = SQLSyntaxHighlighter(self.document(), theme)

        self._completer_model = QStringListModel()
        self._completer = QCompleter(self._completer_model, self)
        self._completer.setWidget(self)
        self._completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        self._completer.setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
        self._completer.activated.connect(self._insert_completion)

        self.update_completions([])

    def update_completions(self, column_names: list[str], table_name: str = "") -> None:
        """Update auto-completion list with column names."""
        self._column_names = column_names
        keywords = [kw.upper() for kw in self._highlighter._get_keywords()]
        completions = keywords + ([table_name.upper()] if table_name else []) + column_names
        self._completer_model.setStringList(completions)

    def _insert_completion(self, completion: str) -> None:
        cursor = self.textCursor()
        extra = len(completion) - len(self._completer.completionPrefix())
        cursor.insertText(completion[len(completion) - extra :])
        self.setTextCursor(cursor)

    def _text_under_cursor(self) -> str:
        cursor = self.textCursor()
        cursor.select(QTextCursor.SelectionType.WordUnderCursor)
        return cursor.selectedText()

    def keyPressEvent(self, event) -> None:
        if self._completer.popup().isVisible():
            if event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Tab):
                event.ignore()
                return

        super().keyPressEvent(event)

        if event.key() in (
            Qt.Key.Key_Return,
            Qt.Key.Key_Enter,
            Qt.Key.Key_Escape,
            Qt.Key.Key_Tab,
            Qt.Key.Key_Backtab,
        ):
            self._completer.popup().hide()
            return

        prefix = self._text_under_cursor()
        if len(prefix) < 2:
            self._completer.popup().hide()
            return

        if prefix != self._completer.completionPrefix():
            self._completer.setCompletionPrefix(prefix)
            self._completer.popup().setCurrentIndex(
                self._completer.completionModel().index(0, 0)
            )

        cursor_rect = self.cursorRect()
        cursor_rect.setWidth(
            self._completer.popup().sizeHintForColumn(0)
            + self._completer.popup().verticalScrollBar().sizeHint().width()
        )
        self._completer.complete(cursor_rect)

    def get_query(self) -> str:
        return self.toPlainText().strip()

    def set_query(self, query: str) -> None:
        self.setPlainText(query)

    def apply_theme(self, theme: Theme) -> None:
        self._theme = theme
        font = QFont(theme.layout.code_font_family, theme.layout.code_font_size)
        self.setFont(font)
        self._highlighter.update_theme(theme)
