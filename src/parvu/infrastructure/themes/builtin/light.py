"""
ParVu Light - Default light theme.
"""
from parvu.infrastructure.themes.models import Theme, ColorScheme, LayoutConfig


light_theme = Theme(
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
    ),
)
