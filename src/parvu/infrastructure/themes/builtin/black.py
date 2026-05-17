"""
ParVu Black - Visual Studio Code Dark inspired theme.
"""
from parvu.infrastructure.themes.models import Theme, ColorScheme, LayoutConfig


black_theme = Theme(
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
    ),
)
