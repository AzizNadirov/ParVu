"""
Qt stylesheet generator for ParVu themes.

Extracted from Theme.generate_stylesheet to separate styling logic
from theme data models.
"""
from __future__ import annotations

from parvu.infrastructure.themes.models import Theme


def generate_stylesheet(theme: Theme) -> str:
    """Generate Qt stylesheet from theme."""
    c = theme.colors
    l = theme.layout  # noqa: E741

    return f"""
    /* Main Window */
    QMainWindow {{
        background-color: {c.background};
        color: {c.foreground};
        font-family: {l.default_font_family};
        font-size: {l.default_font_size}pt;
    }}

    /* Buttons */
    QPushButton {{
        background-color: {c.button_background};
        color: {c.button_foreground};
        border: 1px solid {c.table_grid};
        border-radius: {l.button_border_radius}px;
        padding: 4px 12px;
        min-width: {l.button_min_width}px;
        min-height: {l.button_height}px;
        margin: 2px;
    }}
    QPushButton:hover {{
        background-color: {c.button_hover};
    }}
    QPushButton:pressed {{
        background-color: {c.button_pressed};
    }}
    QPushButton:checked {{
        background-color: {c.accent_primary};
        color: white;
        border: 2px solid {c.accent_secondary};
    }}
    QPushButton:disabled {{
        background-color: {c.table_alternate_row};
        color: {c.editor_comment};
    }}

    /* Line Edit */
    QLineEdit {{
        background-color: {c.table_background};
        color: {c.foreground};
        border: 1px solid {c.table_grid};
        border-radius: 3px;
        padding: 5px;
    }}
    QLineEdit:focus {{
        border: 2px solid {c.accent_primary};
    }}

    /* Text Edit (SQL Editor) */
    QTextEdit {{
        background-color: {c.editor_background};
        color: {c.editor_foreground};
        font-family: {l.code_font_family};
        font-size: {l.code_font_size}pt;
        border: 1px solid {c.table_grid};
        border-radius: 3px;
        selection-background-color: {c.editor_selection};
    }}

    /* Table Widget */
    QTableWidget {{
        background-color: {c.table_background};
        color: {c.table_foreground};
        font-family: {l.table_font_family};
        font-size: {l.table_font_size}pt;
        gridline-color: {c.table_grid if l.show_grid else 'transparent'};
        selection-background-color: {c.table_selection};
        selection-color: white;
        alternate-background-color: {c.table_alternate_row if l.alternate_row_colors else c.table_background};
    }}
    QTableWidget::item {{
        padding: 3px;
    }}
    QHeaderView::section {{
        background-color: {c.table_header_background};
        color: {c.table_header_foreground};
        padding: 5px;
        border: 1px solid {c.table_grid};
        font-weight: bold;
    }}
    QHeaderView::section:hover {{
        background-color: {c.button_hover};
    }}

    /* Menu Bar */
    QMenuBar {{
        background-color: {c.menu_background};
        color: {c.menu_foreground};
        border-bottom: 1px solid {c.table_grid};
    }}
    QMenuBar::item {{
        padding: 5px 10px;
    }}
    QMenuBar::item:selected {{
        background-color: {c.menu_hover};
    }}
    QMenu {{
        background-color: {c.menu_background};
        color: {c.menu_foreground};
        border: 1px solid {c.table_grid};
        icon-size: 16px;
    }}
    QMenu::item {{
        padding: 6px 20px 6px 10px;
        color: {c.menu_foreground};
    }}
    QMenu::item:selected {{
        background-color: {c.menu_hover};
        color: {c.menu_foreground};
    }}

    /* Status Bar */
    QStatusBar {{
        background-color: {c.status_background};
        color: {c.status_foreground};
        border-top: 1px solid {c.table_grid};
    }}

    /* Labels */
    QLabel {{
        color: {c.foreground};
    }}

    /* Dialog */
    QDialog {{
        background-color: {c.background};
        color: {c.foreground};
    }}

    /* Tab Bar Container */
    #TabBar {{
        background-color: {c.status_background};
        border-top: 1px solid {c.tab_inactive_border};
    }}

    /* List Widget */
    QListWidget {{
        background-color: {c.table_background};
        color: {c.foreground};
        border: 1px solid {c.table_grid};
        selection-background-color: {c.table_selection};
        selection-color: white;
    }}

    /* Scroll Bar */
    QScrollBar:vertical {{
        background-color: {c.table_alternate_row};
        width: 12px;
        border: none;
    }}
    QScrollBar::handle:vertical {{
        background-color: {c.table_grid};
        border-radius: 6px;
        min-height: 20px;
    }}
    QScrollBar::handle:vertical:hover {{
        background-color: {c.editor_comment};
    }}
    QScrollBar:horizontal {{
        background-color: {c.table_alternate_row};
        height: 12px;
        border: none;
    }}
    QScrollBar::handle:horizontal {{
        background-color: {c.table_grid};
        border-radius: 6px;
        min-width: 20px;
    }}
    QScrollBar::handle:horizontal:hover {{
        background-color: {c.editor_comment};
    }}
    """
