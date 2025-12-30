# ParVu Theme System

Complete guide to using and creating themes for ParVu.

## Overview

ParVu features a comprehensive theme system that allows you to customize:
- **Colors** - All UI element colors (background, foreground, accents, syntax highlighting)
- **Layout** - Component sizes, fonts, spacing, and visibility
- **Component Styling** - Button styles, table appearance, editor settings

## Built-in Themes

ParVu comes with 3 professionally designed themes:

### 1. ParVu Light (Default)
- Clean, modern light theme
- Blue and green accents
- Optimized for long viewing sessions

### 2. Excel
- Microsoft Excel-inspired design
- Classic green color scheme
- Familiar layout for Excel users
- Professional business appearance

### 3. ParVu Black
- Dark theme based on Visual Studio Code
- Easy on the eyes in low-light environments
- Syntax colors optimized for code readability
- Modern developer aesthetic

## Using Themes

### Changing Themes

**Via Menu:**
1. Open ParVu
2. Go to **File → Change Theme...**
3. Select a theme from the list
4. Click **Apply**

**Current Theme Persistence:**
- Your selected theme is automatically saved
- Persists across application restarts
- Stored in `~/.ParVu/settings/settings.json`

### Theme Preview

The theme selector shows:
- Theme name, author, and description
- Color scheme preview
- Font and layout settings
- Built-in vs custom indication

## Managing Themes

### Importing Themes

1. **File → Change Theme...**
2. Click **Import...**
3. Select a `.json` theme file
4. Theme appears in your list

**Import Sources:**
- Community themes
- Custom themes you've created
- Exported themes from other ParVu installations

### Exporting Themes

1. **File → Change Theme...**
2. Select the theme to export
3. Click **Export...**
4. Choose save location
5. Share the `.json` file

**Use Cases:**
- Backup your custom themes
- Share themes with team members
- Distribute themes to the community

### Deleting Themes

1. **File → Change Theme...**
2. Select a custom theme
3. Click **Delete**
4. Confirm deletion

**Note:** Built-in themes (ParVu Light, Excel, ParVu Black) cannot be deleted.

## Creating Custom Themes

### Theme File Structure

Themes are JSON files with this structure:

```json
{
  "name": "My Theme",
  "description": "A custom theme",
  "author": "Your Name",
  "version": "1.0",
  "colors": {
    "background": "#FFFFFF",
    "foreground": "#000000",
    ...
  },
  "layout": {
    "default_font_family": "Arial",
    "default_font_size": 10,
    ...
  }
}
```

### Color Scheme Options

All colors use hex format (#RRGGBB):

**Main Window:**
- `background` - Main window background
- `foreground` - Main text color

**Buttons:**
- `button_background` - Button background
- `button_foreground` - Button text
- `button_hover` - Hover state
- `button_pressed` - Pressed state

**SQL Editor:**
- `editor_background` - Editor background
- `editor_foreground` - Default text
- `editor_keyword` - SQL keywords (SELECT, FROM, etc.)
- `editor_string` - String literals
- `editor_number` - Numeric values
- `editor_comment` - Comments
- `editor_selection` - Selected text highlight

**Table:**
- `table_background` - Table background
- `table_foreground` - Table text
- `table_alternate_row` - Alternate row color
- `table_header_background` - Header background
- `table_header_foreground` - Header text
- `table_grid` - Grid line color
- `table_selection` - Selected cell color

**Menu:**
- `menu_background` - Menu background
- `menu_foreground` - Menu text
- `menu_hover` - Menu hover state

**Status Bar:**
- `status_background` - Status bar background
- `status_foreground` - Status bar text

**Accents:**
- `accent_primary` - Primary accent color
- `accent_secondary` - Secondary accent
- `accent_warning` - Warning messages
- `accent_error` - Error messages

### Layout Configuration

**Window:**
- `window_min_width` - Minimum window width (default: 1200)
- `window_min_height` - Minimum window height (default: 800)

**Component Heights:**
- `sql_editor_height` - SQL editor height (default: 100)
- `toolbar_height` - Toolbar height (default: 40)
- `status_bar_height` - Status bar height (default: 25)

**Margins & Spacing:**
- `margin` - Default margin (default: 10)
- `spacing` - Default spacing (default: 5)

**Fonts:**
- `default_font_family` - Main UI font (e.g., "Arial")
- `default_font_size` - Main UI font size
- `code_font_family` - SQL editor font (e.g., "Courier New")
- `code_font_size` - SQL editor font size
- `table_font_family` - Table data font
- `table_font_size` - Table data font size

**Table:**
- `table_row_height` - Row height (default: 25)
- `table_header_height` - Header height (default: 30)
- `show_grid` - Show grid lines (true/false)
- `alternate_row_colors` - Alternate row colors (true/false)

**Buttons:**
- `button_min_width` - Minimum button width (default: 80)
- `button_height` - Button height (default: 30)
- `button_border_radius` - Button corner radius (default: 4)

**Toolbar:**
- `show_file_path_label` - Show file path label (true/false)
- `show_table_info_button` - Show table info button (true/false)
- `show_reset_button` - Show reset button (true/false)

### Example: Creating a Custom Theme

1. **Export an existing theme as a template:**
   ```bash
   File → Change Theme... → Select "ParVu Light" → Export...
   Save as: my_theme.json
   ```

2. **Edit the JSON file:**
   ```json
   {
     "name": "Ocean Blue",
     "description": "A calming ocean-inspired theme",
     "author": "Your Name",
     "version": "1.0",
     "colors": {
       "background": "#E3F2FD",
       "foreground": "#01579B",
       "button_background": "#0288D1",
       "button_foreground": "#FFFFFF",
       "button_hover": "#0277BD",
       "button_pressed": "#01579B",
       "editor_background": "#F1F8FE",
       "editor_foreground": "#01579B",
       "editor_keyword": "#0277BD",
       "editor_string": "#00897B",
       "editor_number": "#6A1B9A",
       "editor_comment": "#78909C",
       "editor_selection": "#B3E5FC",
       "table_background": "#FFFFFF",
       "table_foreground": "#01579B",
       "table_alternate_row": "#E1F5FE",
       "table_header_background": "#0288D1",
       "table_header_foreground": "#FFFFFF",
       "table_grid": "#B3E5FC",
       "table_selection": "#0288D1",
       "menu_background": "#FFFFFF",
       "menu_foreground": "#01579B",
       "menu_hover": "#E1F5FE",
       "status_background": "#0288D1",
       "status_foreground": "#FFFFFF",
       "accent_primary": "#0288D1",
       "accent_secondary": "#00ACC1",
       "accent_warning": "#FB8C00",
       "accent_error": "#E53935"
     },
     "layout": {
       "default_font_family": "Segoe UI",
       "default_font_size": 10,
       "code_font_family": "Consolas",
       "code_font_size": 11,
       "table_font_family": "Segoe UI",
       "table_font_size": 10,
       "show_grid": true,
       "alternate_row_colors": true
     }
   }
   ```

3. **Import your theme:**
   ```bash
   File → Change Theme... → Import... → Select my_theme.json
   ```

4. **Apply and enjoy!**

## Theme Best Practices

### Colors

**Contrast:**
- Ensure sufficient contrast between foreground and background
- Text should be easily readable
- Minimum ratio: 4.5:1 for normal text, 3:1 for large text

**Consistency:**
- Use a coherent color palette
- Limit to 3-5 main colors plus variations
- Keep accent colors consistent with overall theme

**Accessibility:**
- Consider color-blind users
- Don't rely solely on color to convey information
- Test with various display settings

### Fonts

**Readability:**
- Use clear, professional fonts
- Monospace fonts for code/data (Courier, Consolas, Monaco)
- Sans-serif for UI (Arial, Segoe UI, Helvetica)

**Sizing:**
- UI text: 9-12pt
- Code editor: 10-14pt
- Table data: 9-11pt

**Cross-platform:**
- Provide fallback fonts
- Common choices: Arial, Helvetica, Segoe UI
- Monospace: Courier New, Consolas, Monaco

### Layout

**Spacing:**
- Maintain consistent margins and padding
- Ensure clickable elements are appropriately sized
- Leave adequate whitespace

**Component Sizing:**
- SQL editor: 80-120px height
- Table rows: 20-30px height
- Buttons: minimum 30px height

## Theme Locations

**Built-in Themes:**
- Stored in: `~/.ParVu/themes/`
- Files: `parvu_light.json`, `excel.json`, `parvu_black.json`
- Auto-generated on first run

**Custom Themes:**
- Same location: `~/.ParVu/themes/`
- Any `.json` file in this directory

**Active Theme:**
- Saved in: `~/.ParVu/settings/settings.json`
- Field: `current_theme`

## Troubleshooting

**Theme not loading:**
1. Check JSON syntax (use a validator)
2. Ensure all required fields are present
3. Verify color values are valid hex codes
4. Check logs: `~/.ParVu/logs/parvu_*.log`

**Colors look wrong:**
- Verify hex color format: `#RRGGBB`
- Check for typos in color names
- Test colors in a color picker tool

**Theme changes not applying:**
1. Close and reopen ParVu
2. Re-import the theme
3. Check for errors in logs

**Can't delete theme:**
- Built-in themes cannot be deleted
- Only custom themes can be removed

## Sharing Themes

### Creating a Theme Package

1. Export your theme
2. Include a README with:
   - Theme name and description
   - Screenshot/preview
   - Installation instructions
   - Your contact info

3. Share on:
   - GitHub
   - ParVu community forums
   - Theme repositories

### Installing Community Themes

1. Download `.json` file
2. Import via **File → Change Theme... → Import...**
3. Apply the theme

## Advanced Topics

### Programmatic Theme Creation

You can create themes programmatically using Python:

```python
from src.themes import Theme, ColorScheme, LayoutConfig

# Create custom theme
theme = Theme(
    name="My Custom Theme",
    description="Created programmatically",
    author="Script",
    colors=ColorScheme(
        background="#1E1E1E",
        foreground="#D4D4D4",
        # ... other colors
    ),
    layout=LayoutConfig(
        default_font_family="Arial",
        # ... other settings
    )
)

# Save to file
from pathlib import Path
theme.save_to_file(Path.home() / ".ParVu/themes/custom.json")
```

### Dynamic Themes

Themes are loaded at startup but can be changed at runtime via the theme selector.

## FAQ

**Q: Can I have different themes for different files?**
A: No, themes are global and apply to the entire application.

**Q: Do themes affect performance?**
A: No, themes are purely visual and don't impact query or rendering performance.

**Q: Can I edit built-in themes?**
A: Export a built-in theme, modify it, and import as a new custom theme.

**Q: What happens to my theme if I update ParVu?**
A: Custom themes in `~/.ParVu/themes/` are preserved across updates.

**Q: Can I create a theme  with animations?**
A: No, the current theme system only supports static styling.

## Contributing Themes

We welcome theme contributions! To submit a theme:

1. Create a well-designed, tested theme
2. Export to JSON
3. Create a GitHub issue or pull request
4. Include:
   - Theme file
   - Screenshot
   - Description
   - Use case/target audience

Popular community themes may be included as built-ins in future releases!

## Support

- **Documentation:** This file
- **Issues:** https://github.com/AzizNadirov/ParVu/issues
- **Logs:** `~/.ParVu/logs/parvu_*.log`
- **Contact:** See README.md

---

Happy theming! 🎨
