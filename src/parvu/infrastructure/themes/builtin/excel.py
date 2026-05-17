"""
Excel theme - Microsoft Excel inspired.
"""
from parvu.infrastructure.themes.models import Theme, ColorScheme, LayoutConfig


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
    ),
)
