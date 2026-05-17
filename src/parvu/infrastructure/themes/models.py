"""
Theme data models for ParVu.
"""
from __future__ import annotations

from pathlib import Path

from pydantic import BaseModel, Field
from loguru import logger


class ColorScheme(BaseModel):
    """Color scheme for the application."""

    # Main window colors
    background: str = "#FFFFFF"
    foreground: str = "#000000"

    # Button colors
    button_background: str = "#E8F5E9"
    button_foreground: str = "#000000"
    button_hover: str = "#C8E6C9"
    button_pressed: str = "#A5D6A7"

    # SQL Editor colors
    editor_background: str = "#FFF9C4"
    editor_foreground: str = "#000000"
    editor_keyword: str = "#0066CC"
    editor_string: str = "#00AA00"
    editor_number: str = "#AA00AA"
    editor_comment: str = "#808080"
    editor_selection: str = "#B3E5FC"

    # Table colors
    table_background: str = "#FFFFFF"
    table_foreground: str = "#000000"
    table_alternate_row: str = "#F5F5F5"
    table_header_background: str = "#E0E0E0"
    table_header_foreground: str = "#000000"
    table_grid: str = "#DDDDDD"
    table_selection: str = "#2196F3"

    # Menu colors
    menu_background: str = "#FFFFFF"
    menu_foreground: str = "#000000"
    menu_hover: str = "#E3F2FD"

    # Status bar colors
    status_background: str = "#F5F5F5"
    status_foreground: str = "#000000"

    # Accent colors
    accent_primary: str = "#2196F3"
    accent_secondary: str = "#4CAF50"
    accent_warning: str = "#FF9800"
    accent_error: str = "#F44336"


class LayoutConfig(BaseModel):
    """Layout configuration for UI components."""

    # Window settings
    window_min_width: int = 1200
    window_min_height: int = 800

    # Component heights
    sql_editor_height: int = 100
    toolbar_height: int = 40
    status_bar_height: int = 25

    # Margins and spacing
    margin: int = 10
    spacing: int = 5

    # Font settings
    default_font_family: str = "Arial"
    default_font_size: int = 10
    code_font_family: str = "Courier New"
    code_font_size: int = 10
    table_font_family: str = "Courier"
    table_font_size: int = 9

    # Table settings
    table_row_height: int = 25
    table_header_height: int = 30
    show_grid: bool = True
    alternate_row_colors: bool = True

    # Button settings
    button_min_width: int = 80
    button_height: int = 30
    button_border_radius: int = 4

    # Toolbar visibility
    show_file_path_label: bool = True
    show_table_info_button: bool = True
    show_reset_button: bool = True


class Theme(BaseModel):
    """Complete theme definition."""

    name: str
    description: str = ""
    author: str = "ParVu"
    version: str = "1.0"

    colors: ColorScheme = Field(default_factory=ColorScheme)
    layout: LayoutConfig = Field(default_factory=LayoutConfig)

    def to_json(self) -> str:
        """Export theme to JSON string."""
        return self.model_dump_json(indent=2)

    @classmethod
    def from_json(cls, json_str: str) -> Theme:
        """Import theme from JSON string."""
        return cls.model_validate_json(json_str)

    def save_to_file(self, file_path: Path) -> None:
        """Save theme to file."""
        with open(file_path, "w") as f:
            f.write(self.to_json())
        logger.info(f"Theme saved to {file_path}")

    @classmethod
    def load_from_file(cls, file_path: Path) -> Theme:
        """Load theme from file."""
        with open(file_path, "r") as f:
            return cls.from_json(f.read())
