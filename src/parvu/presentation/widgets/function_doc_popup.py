"""
Function documentation popup — tooltip-like widget for DSL function help.

Shows when the user navigates the expression editor completer or clicks
on a function name in the editor. Displays signature, description,
parameters, example and SQL mapping.
"""
from __future__ import annotations

from PyQt6.QtWidgets import QFrame, QVBoxLayout, QLabel
from PyQt6.QtCore import Qt, QPoint
from PyQt6.QtGui import QFont

from parvu.core.dsl.registry import FunctionDef
from parvu.infrastructure.themes.models import Theme


class FunctionDocPopup(QFrame):
    """Floating documentation card for a DSL function."""

    def __init__(self, parent=None, theme: Theme | None = None):
        super().__init__(parent)
        self._theme = theme

        self.setWindowFlags(
            Qt.WindowType.ToolTip
            | Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowDoesNotAcceptFocus
        )
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating)
        self.setFrameShape(QFrame.Shape.StyledPanel)
        self.setLineWidth(1)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(10, 10, 10, 10)
        layout.setSpacing(6)

        self._label = QLabel(self)
        self._label.setTextFormat(Qt.TextFormat.RichText)
        self._label.setWordWrap(True)
        self._label.setOpenExternalLinks(False)

        font = QFont(self._theme.layout.code_font_family if self._theme else "Courier New", 9)
        self._label.setFont(font)

        layout.addWidget(self._label)
        self._apply_style()
        self.hide()

    # ------------------------------------------------------------------
    # Styling
    # ------------------------------------------------------------------

    def _apply_style(self) -> None:
        if self._theme:
            bg = self._theme.colors.editor_background
            fg = self._theme.colors.editor_foreground
            border = self._theme.colors.table_grid
            accent = self._theme.colors.accent_primary
            dim = self._theme.colors.editor_comment
            code_font = self._theme.layout.code_font_family
        else:
            bg = "#2D2D30"
            fg = "#CCCCCC"
            border = "#555555"
            accent = "#4FC1FF"
            dim = "#808080"
            code_font = "Courier New"

        self.setStyleSheet(f"""
            QFrame {{
                background-color: {bg};
                color: {fg};
                border: 1px solid {border};
                border-radius: 4px;
            }}
            QLabel {{
                color: {fg};
                font-family: {code_font};
                font-size: 10pt;
            }}
        """)
        self._accent = accent
        self._dim = dim

    def apply_theme(self, theme: Theme) -> None:
        """Update colors from the given theme."""
        self._theme = theme
        self._apply_style()

    # ------------------------------------------------------------------
    # Show / hide
    # ------------------------------------------------------------------

    def show_for_function(self, func: FunctionDef, pos: QPoint) -> None:
        """Populate the popup and show it at *pos* (global coordinates)."""
        self._label.setText(self._build_html(func))
        self.adjustSize()

        # Keep inside available screen geometry
        screen = self.screen()
        if screen:
            scr = screen.availableGeometry()
            x = pos.x()
            y = pos.y()
            if x + self.width() > scr.right():
                x = scr.right() - self.width() - 5
            if y + self.height() > scr.bottom():
                y = pos.y() - self.height() - 5
            self.move(max(scr.left() + 5, x), max(scr.top() + 5, y))
        else:
            self.move(pos)

        self.show()

    def hide_popup(self) -> None:
        """Hide the popup if visible."""
        self.hide()

    # ------------------------------------------------------------------
    # HTML builder
    # ------------------------------------------------------------------

    def _build_html(self, func: FunctionDef) -> str:
        accent = self._accent
        dim = self._dim

        # Signature line
        params_str = ", ".join(
            f'{p.name}: {p.logical_type.name}'
            + ("" if p.required else " = ?")
            for p in func.params
        )
        sig = f'<b style="color:{accent};">{func.name}</b>({params_str})'
        if func.return_type:
            sig += f' → <span style="color:{accent};">{func.return_type.name}</span>'

        lines: list[str] = [sig, ""]

        if func.description:
            lines.append(f"<span>{func.description}</span>")
            lines.append("")

        if func.params:
            lines.append(f'<span style="color:{dim};">Parameters:</span>')
            for p in func.params:
                req = "" if p.required else " (optional)"
                desc = f" — {p.description}" if p.description else ""
                lines.append(
                    f'  • <b>{p.name}</b>: {p.logical_type.name}{req}{desc}'
                )
            lines.append("")

        if func.example:
            lines.append(f'<span style="color:{dim};">Example:</span>')
            lines.append(f'<code style="color:{accent};">{func.example}</code>')
            lines.append("")

        if func.sqlglot_name:
            lines.append(
                f'<span style="color:{dim};">SQL:</span> <code>{func.sqlglot_name}</code>'
            )

        return "<br>".join(lines)
