"""
SQL Editor Widget with Syntax Highlighting and Auto-Completion
"""
from PyQt6.QtWidgets import QTextEdit, QCompleter
from PyQt6.QtGui import QSyntaxHighlighter, QTextCharFormat, QColor, QFont, QTextCursor
from PyQt6.QtCore import Qt, QRegularExpression, QStringListModel
from typing import Optional

from schemas import settings


class SQLSyntaxHighlighter(QSyntaxHighlighter):
    """Syntax highlighter for SQL with keyword highlighting"""

    def __init__(self, parent=None, theme=None):
        super().__init__(parent)
        self.theme = theme
        self._highlighting_rules = []
        self._setup_highlighting()

    def _setup_highlighting(self):
        """Setup highlighting rules with theme colors"""
        self._highlighting_rules.clear()

        # Get colors from theme or use defaults
        if self.theme:
            keyword_color = self.theme.colors.editor_keyword
            string_color = self.theme.colors.editor_string
            number_color = self.theme.colors.editor_number
            comment_color = self.theme.colors.editor_comment
        else:
            keyword_color = "#0066CC"
            string_color = "#00AA00"
            number_color = "#AA00AA"
            comment_color = "#808080"

        # Keyword format (blue, bold)
        keyword_format = QTextCharFormat()
        keyword_format.setForeground(QColor(keyword_color))
        keyword_format.setFontWeight(QFont.Weight.Bold)

        keywords = settings.sql_keywords + [settings.render_vars(settings.default_data_var_name).upper()]

        for keyword in keywords:
            pattern = QRegularExpression(f"\\b{keyword}\\b", QRegularExpression.PatternOption.CaseInsensitiveOption)
            self._highlighting_rules.append((pattern, keyword_format))

        # String format
        string_format = QTextCharFormat()
        string_format.setForeground(QColor(string_color))
        pattern = QRegularExpression("'[^']*'")
        self._highlighting_rules.append((pattern, string_format))

        # Number format
        number_format = QTextCharFormat()
        number_format.setForeground(QColor(number_color))
        pattern = QRegularExpression("\\b[0-9]+\\.?[0-9]*\\b")
        self._highlighting_rules.append((pattern, number_format))

        # Comment format
        comment_format = QTextCharFormat()
        comment_format.setForeground(QColor(comment_color))
        comment_format.setFontItalic(True)
        pattern = QRegularExpression("--[^\n]*")
        self._highlighting_rules.append((pattern, comment_format))

    def update_theme(self, theme):
        """Update theme and rehighlight"""
        self.theme = theme
        self._setup_highlighting()
        self.rehighlight()

    def highlightBlock(self, text):
        """Apply highlighting rules to text block"""
        for pattern, format in self._highlighting_rules:
            iterator = pattern.globalMatch(text)
            while iterator.hasNext():
                match = iterator.next()
                self.setFormat(match.capturedStart(), match.capturedLength(), format)


class SQLEditor(QTextEdit):
    """
    SQL Editor with auto-completion and syntax highlighting
    """

    def __init__(self, parent=None, theme=None):
        super().__init__(parent)
        self.column_names = []
        self.theme = theme

        # Set monospace font from theme or settings
        if theme:
            font = QFont(theme.layout.code_font_family, theme.layout.code_font_size)
        else:
            font = QFont(settings.default_sql_font, int(settings.default_sql_font_size))
        self.setFont(font)

        # Set initial text
        self.setPlainText(settings.render_vars(settings.default_sql_query))

        # Apply syntax highlighting
        self.highlighter = SQLSyntaxHighlighter(self.document(), theme)

        # Set up auto-completer with string list model
        self.completer_model = QStringListModel()
        self.completer = QCompleter(self.completer_model, self)
        self.completer.setWidget(self)
        self.completer.setCaseSensitivity(Qt.CaseSensitivity.CaseInsensitive)
        self.completer.setCompletionMode(QCompleter.CompletionMode.PopupCompletion)
        self.completer.activated.connect(self.insert_completion)

        # Initial completion list (SQL keywords only)
        self.update_completions([])

    def update_completions(self, column_names: list):
        """
        Update auto-completion list with column names

        Args:
            column_names: List of column names from loaded table
        """
        self.column_names = column_names

        # Combine SQL keywords and column names
        keywords = [kw.upper() for kw in settings.sql_keywords]
        table_name = settings.render_vars(settings.default_data_var_name).upper()
        completions = keywords + [table_name] + column_names

        # Update the model's string list
        self.completer_model.setStringList(completions)

    def insert_completion(self, completion):
        """Insert the selected completion"""
        cursor = self.textCursor()
        extra = len(completion) - len(self.completer.completionPrefix())
        cursor.insertText(completion[len(completion) - extra:])
        self.setTextCursor(cursor)

    def text_under_cursor(self):
        """Get the word under cursor for auto-completion"""
        cursor = self.textCursor()
        cursor.select(QTextCursor.SelectionType.WordUnderCursor)
        return cursor.selectedText()

    def keyPressEvent(self, event):
        """Handle key press events for auto-completion"""
        # If completer is visible and user pressed Enter/Tab, insert completion
        if self.completer.popup().isVisible():
            if event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Tab):
                event.ignore()
                return

        # Normal key press handling
        super().keyPressEvent(event)

        # Don't show completer for these keys
        if event.key() in (Qt.Key.Key_Return, Qt.Key.Key_Enter, Qt.Key.Key_Escape,
                           Qt.Key.Key_Tab, Qt.Key.Key_Backtab):
            self.completer.popup().hide()
            return

        # Get word under cursor
        completion_prefix = self.text_under_cursor()

        # Show completer if we have at least 2 characters
        if len(completion_prefix) < 2:
            self.completer.popup().hide()
            return

        # Update completer
        if completion_prefix != self.completer.completionPrefix():
            self.completer.setCompletionPrefix(completion_prefix)
            self.completer.popup().setCurrentIndex(
                self.completer.completionModel().index(0, 0)
            )

        # Show completer popup
        cursor_rect = self.cursorRect()
        cursor_rect.setWidth(
            self.completer.popup().sizeHintForColumn(0) +
            self.completer.popup().verticalScrollBar().sizeHint().width()
        )
        self.completer.complete(cursor_rect)

    def get_query(self) -> str:
        """Get the current SQL query text"""
        return self.toPlainText().strip()

    def set_query(self, query: str):
        """Set the SQL query text"""
        self.setPlainText(query)

    def update_theme(self, theme):
        """Update editor theme"""
        self.theme = theme
        # Update font
        font = QFont(theme.layout.code_font_family, theme.layout.code_font_size)
        self.setFont(font)
        # Update highlighter
        self.highlighter.update_theme(theme)
