"""
Theme System for ParVu
Supports custom themes with colors, layouts, and component styling
"""
from pathlib import Path
from typing import Optional, Dict
from pydantic import BaseModel, Field
from loguru import logger

from schemas import settings


class ColorScheme(BaseModel):
    """Color scheme for the application"""
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
    """Layout configuration for UI components"""
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
    """Complete theme definition"""
    name: str
    description: str = ""
    author: str = "ParVu"
    version: str = "1.0"

    colors: ColorScheme = Field(default_factory=ColorScheme)
    layout: LayoutConfig = Field(default_factory=LayoutConfig)

    def to_json(self) -> str:
        """Export theme to JSON string"""
        return self.model_dump_json(indent=2)

    @classmethod
    def from_json(cls, json_str: str) -> 'Theme':
        """Import theme from JSON string"""
        return cls.model_validate_json(json_str)

    def save_to_file(self, file_path: Path):
        """Save theme to file"""
        with open(file_path, 'w') as f:
            f.write(self.to_json())
        logger.info(f"Theme saved to {file_path}")

    @classmethod
    def load_from_file(cls, file_path: Path) -> 'Theme':
        """Load theme from file"""
        with open(file_path, 'r') as f:
            return cls.from_json(f.read())

    def generate_stylesheet(self) -> str:
        """Generate Qt stylesheet from theme"""
        c = self.colors
        l = self.layout

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
            padding: 5px 15px;
            min-width: {l.button_min_width}px;
            min-height: {l.button_height}px;
        }}

        QPushButton:hover {{
            background-color: {c.button_hover};
        }}

        QPushButton:pressed {{
            background-color: {c.button_pressed};
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
        }}

        QMenu::item {{
            padding: 5px 30px;
        }}

        QMenu::item:selected {{
            background-color: {c.menu_hover};
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


class ThemeManager:
    """Manages themes for the application"""

    def __init__(self):
        self.themes_dir = settings.user_app_settings_dir / "themes"
        self.themes_dir.mkdir(parents=True, exist_ok=True)

        self.current_theme: Optional[Theme] = None
        self.builtin_themes: Dict[str, Theme] = {}

        # Initialize built-in themes
        self._create_builtin_themes()

        logger.info(f"ThemeManager initialized with {len(self.builtin_themes)} built-in themes")

    def _create_builtin_themes(self):
        """Create built-in themes"""
        # Excel-inspired theme
        excel_theme = Theme(
            name="Excel",
            description="Microsoft Excel inspired theme with green accents",
            colors=ColorScheme(
                background="#FFFFFF",
                foreground="#000000",
                button_background="#217346",
                button_foreground="#FFFFFF",
                button_hover="#2D9558",
                button_pressed="#1A5C37",
                editor_background="#F9F9F9",
                editor_foreground="#000000",
                editor_keyword="#0070C0",
                editor_string="#2E7D32",
                editor_number="#C55A11",
                editor_comment="#7F7F7F",
                editor_selection="#C7E0F4",
                table_background="#FFFFFF",
                table_foreground="#000000",
                table_alternate_row="#F2F2F2",
                table_header_background="#217346",
                table_header_foreground="#FFFFFF",
                table_grid="#D0D0D0",
                table_selection="#217346",
                menu_background="#FFFFFF",
                menu_foreground="#000000",
                menu_hover="#E7E6E6",
                status_background="#217346",
                status_foreground="#FFFFFF",
                accent_primary="#217346",
                accent_secondary="#0070C0",
                accent_warning="#C55A11",
                accent_error="#C00000",
            ),
            layout=LayoutConfig(
                default_font_family="Calibri",
                default_font_size=11,
                code_font_family="Consolas",
                code_font_size=10,
                table_font_family="Calibri",
                table_font_size=11,
                show_grid=True,
                alternate_row_colors=True,
                button_border_radius=2,
            )
        )

        # ParVu Black theme
        vscode_theme = Theme(
            name="ParVu Black",
            description="Visual Studio Code Dark theme",
            colors=ColorScheme(
                background="#1E1E1E",
                foreground="#D4D4D4",
                button_background="#0E639C",
                button_foreground="#FFFFFF",
                button_hover="#1177BB",
                button_pressed="#0D5A8F",
                editor_background="#1E1E1E",
                editor_foreground="#D4D4D4",
                editor_keyword="#569CD6",
                editor_string="#CE9178",
                editor_number="#B5CEA8",
                editor_comment="#6A9955",
                editor_selection="#264F78",
                table_background="#252526",
                table_foreground="#CCCCCC",
                table_alternate_row="#2D2D30",
                table_header_background="#3E3E42",
                table_header_foreground="#CCCCCC",
                table_grid="#3E3E42",
                table_selection="#094771",
                menu_background="#252526",
                menu_foreground="#CCCCCC",
                menu_hover="#2A2D2E",
                status_background="#007ACC",
                status_foreground="#FFFFFF",
                accent_primary="#007ACC",
                accent_secondary="#4EC9B0",
                accent_warning="#CE9178",
                accent_error="#F48771",
            ),
            layout=LayoutConfig(
                default_font_family="Segoe UI",
                default_font_size=10,
                code_font_family="Consolas",
                code_font_size=12,
                table_font_family="Consolas",
                table_font_size=10,
                show_grid=True,
                alternate_row_colors=True,
                button_border_radius=3,
            )
        )

        # Default/Light theme (current ParVu style)
        default_theme = Theme(
            name="ParVu Light",
            description="Default ParVu light theme",
            colors=ColorScheme(
                background="#FFFFFF",
                foreground="#000000",
                button_background="#E8F5E9",
                button_foreground="#000000",
                button_hover="#C8E6C9",
                button_pressed="#A5D6A7",
                editor_background="#FFF9C4",
                editor_foreground="#000000",
                editor_keyword="#0066CC",
                editor_string="#00AA00",
                editor_number="#AA00AA",
                editor_comment="#808080",
                editor_selection="#B3E5FC",
                table_background="#FFFFFF",
                table_foreground="#000000",
                table_alternate_row="#F5F5F5",
                table_header_background="#E0E0E0",
                table_header_foreground="#000000",
                table_grid="#DDDDDD",
                table_selection="#2196F3",
                menu_background="#FFFFFF",
                menu_foreground="#000000",
                menu_hover="#E3F2FD",
                status_background="#F5F5F5",
                status_foreground="#000000",
                accent_primary="#2196F3",
                accent_secondary="#4CAF50",
                accent_warning="#FF9800",
                accent_error="#F44336",
            ),
            layout=LayoutConfig(
                default_font_family="Arial",
                code_font_family="Courier New",
                table_font_family="Courier",
            )
        )

        self.builtin_themes = {
            "ParVu Light": default_theme,
            "Excel": excel_theme,
            "ParVu Black": vscode_theme,
        }

        # Save built-in themes to disk for reference
        for name, theme in self.builtin_themes.items():
            theme_file = self.themes_dir / f"{name.lower().replace(' ', '_')}.json"
            if not theme_file.exists():
                theme.save_to_file(theme_file)

    def get_theme(self, name: str) -> Optional[Theme]:
        """Get theme by name (checks built-in first, then custom)"""
        # Check built-in themes
        if name in self.builtin_themes:
            return self.builtin_themes[name]

        # Check custom themes
        theme_file = self.themes_dir / f"{name.lower().replace(' ', '_')}.json"
        if theme_file.exists():
            try:
                return Theme.load_from_file(theme_file)
            except Exception as e:
                logger.error(f"Failed to load theme {name}: {e}")

        return None

    def list_themes(self) -> list[str]:
        """List all available themes"""
        themes = list(self.builtin_themes.keys())

        # Add custom themes
        for theme_file in self.themes_dir.glob("*.json"):
            try:
                theme = Theme.load_from_file(theme_file)
                if theme.name not in themes:
                    themes.append(theme.name)
            except Exception:
                pass

        return sorted(themes)

    def set_theme(self, name: str) -> bool:
        """Set current theme"""
        theme = self.get_theme(name)
        if theme:
            self.current_theme = theme
            logger.info(f"Theme set to: {name}")
            return True
        return False

    def export_theme(self, theme_name: str, export_path: Path) -> bool:
        """Export theme to file"""
        theme = self.get_theme(theme_name)
        if theme:
            try:
                theme.save_to_file(export_path)
                return True
            except Exception as e:
                logger.error(f"Failed to export theme: {e}")
        return False

    def import_theme(self, import_path: Path) -> Optional[str]:
        """Import theme from file, returns theme name if successful"""
        try:
            theme = Theme.load_from_file(import_path)
            # Save to themes directory
            theme_file = self.themes_dir / f"{theme.name.lower().replace(' ', '_')}.json"
            theme.save_to_file(theme_file)
            logger.info(f"Imported theme: {theme.name}")
            return theme.name
        except Exception as e:
            logger.error(f"Failed to import theme: {e}")
            return None

    def delete_theme(self, name: str) -> bool:
        """Delete custom theme (cannot delete built-in themes)"""
        if name in self.builtin_themes:
            logger.warning(f"Cannot delete built-in theme: {name}")
            return False

        theme_file = self.themes_dir / f"{name.lower().replace(' ', '_')}.json"
        if theme_file.exists():
            try:
                theme_file.unlink()
                logger.info(f"Deleted theme: {name}")
                return True
            except Exception as e:
                logger.error(f"Failed to delete theme: {e}")

        return False


# Global theme manager instance
theme_manager = ThemeManager()
