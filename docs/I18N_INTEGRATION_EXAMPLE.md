# i18n Integration Examples

This document shows how to integrate i18n into the existing ParVu codebase.

## Example 1: Updating main_window.py

### Original Code (Lines 77-92)

```python
def setup_ui(self):
    """Setup main UI components"""
    self.setWindowTitle("ParVu - Parquet/CSV Viewer")
    self.setGeometry(100, 100, 1400, 900)

    # Create central widget
    central_widget = QWidget()
    self.setCentralWidget(central_widget)
    layout = QVBoxLayout(central_widget)

    # File selection section
    file_layout = QHBoxLayout()

    self.file_path_edit = QLineEdit()
    self.file_path_edit.setPlaceholderText("Select a Parquet, CSV, or JSON file...")
    file_layout.addWidget(self.file_path_edit, stretch=3)

    self.browse_btn = QPushButton("Browse & Load...")
```

### Updated Code with i18n

```python
from i18n import t  # Add this import at the top

def setup_ui(self):
    """Setup main UI components"""
    self.setWindowTitle(t("app.title"))  # ✅ Translated
    self.setGeometry(100, 100, 1400, 900)

    # Create central widget
    central_widget = QWidget()
    self.setCentralWidget(central_widget)
    layout = QVBoxLayout(central_widget)

    # File selection section
    file_layout = QHBoxLayout()

    self.file_path_edit = QLineEdit()
    self.file_path_edit.setPlaceholderText(t("label.file_placeholder"))  # ✅ Translated
    file_layout.addWidget(self.file_path_edit, stretch=3)

    self.browse_btn = QPushButton(t("btn.browse"))  # ✅ Translated
```

## Example 2: Updating Menu Bar (main_window.py lines 224-260)

### Original Code

```python
def setup_menu(self):
    """Setup menu bar"""
    menubar = self.menuBar()

    # File menu
    file_menu = menubar.addMenu("File")

    # Open file
    open_action = file_menu.addAction("Open File...")
    open_action.triggered.connect(self.browse_file)

    # Export results
    export_action = file_menu.addAction("Export Results...")
    export_action.triggered.connect(self.export_results)

    # Settings
    settings_action = file_menu.addAction("Settings...")
    settings_action.triggered.connect(self.show_settings)

    # Recent files submenu
    self.recent_menu = file_menu.addMenu("Recent Files")
```

### Updated Code with i18n

```python
from i18n import t  # Add at top of file

def setup_menu(self):
    """Setup menu bar"""
    menubar = self.menuBar()

    # File menu
    file_menu = menubar.addMenu(t("menu.file"))  # ✅ Translated

    # Open file
    open_action = file_menu.addAction(t("menu.file.open"))  # ✅ Translated
    open_action.triggered.connect(self.browse_file)

    # Export results
    export_action = file_menu.addAction(t("menu.file.export"))  # ✅ Translated
    export_action.triggered.connect(self.export_results)

    # Settings
    settings_action = file_menu.addAction(t("menu.file.settings"))  # ✅ Translated
    settings_action.triggered.connect(self.show_settings)

    # Recent files submenu
    self.recent_menu = file_menu.addMenu(t("menu.file.recent_files"))  # ✅ Translated
```

## Example 3: Updating Error Messages (main_window.py)

### Original Code (Lines 280-285)

```python
def browse_file(self):
    """Open file browser dialog"""
    if not self.file_path_edit.text().strip():
        QMessageBox.warning(
            self,
            "No File",
            "Please enter or browse for a file."
        )
        return
```

### Updated Code with i18n

```python
from i18n import t  # Add at top

def browse_file(self):
    """Open file browser dialog"""
    if not self.file_path_edit.text().strip():
        QMessageBox.warning(
            self,
            t("error.no_file"),  # ✅ Translated title
            t("error.no_file_msg")  # ✅ Translated message
        )
        return
```

## Example 4: Dynamic Text with Variables (main_window.py)

### Original Code (Line 327)

```python
self.statusBar().showMessage(f"Loaded: {file_path.name} ({row_count:,} rows)", 5000)
```

### Updated Code with i18n

```python
from i18n import t

self.statusBar().showMessage(
    t("status.loaded", filename=file_path.name, rows=row_count),
    5000
)
```

## Example 5: Adding Language Selector to Settings Dialog

### Add to settings_dialog.py

```python
from language_selector import SimpleLanguageSelector
from i18n import t, get_i18n

class SettingsDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.i18n = get_i18n()
        # ... existing code ...

    def setup_general_tab(self):
        """Setup General settings tab"""
        # ... existing code ...

        # Language selector
        lang_group = QGroupBox(t("language.label"))  # ✅ Translated
        lang_layout = QVBoxLayout()

        self.lang_selector = SimpleLanguageSelector()
        self.lang_selector.language_changed.connect(self.on_language_changed)
        lang_layout.addWidget(self.lang_selector)

        lang_group.setLayout(lang_layout)
        layout.addWidget(lang_group)

        # ... rest of code ...

    def on_language_changed(self, locale_code):
        """Handle language change"""
        # Update i18n
        self.i18n.set_locale(locale_code)

        # Save to settings
        from schemas import settings
        settings.current_language = locale_code
        settings.save_settings()

        # Notify user to restart
        QMessageBox.information(
            self,
            t("dialog.settings"),
            "Please restart ParVu for the language change to take effect."
        )
```

## Example 6: Updating Button Labels

### Original Code (settings_dialog.py lines 59-63)

```python
self.save_btn = QPushButton("Save")
self.save_btn.clicked.connect(self.save_settings)
button_layout.addWidget(self.save_btn)

self.cancel_btn = QPushButton("Cancel")
```

### Updated Code

```python
from i18n import t

self.save_btn = QPushButton(t("btn.save"))  # ✅ Translated
self.save_btn.clicked.connect(self.save_settings)
button_layout.addWidget(self.save_btn)

self.cancel_btn = QPushButton(t("btn.cancel"))  # ✅ Translated
```

## Example 7: Context Menu (table_view.py lines 212-236)

### Original Code

```python
def show_context_menu(self, pos):
    """Show context menu for column operations"""
    # ... code ...

    menu = QMenu(self)

    # Copy column name
    copy_action = menu.addAction("Copy Column Name")
    copy_action.triggered.connect(lambda: self.copy_column_name(column_name))

    # Sort ascending
    sort_asc_action = menu.addAction("Sort Ascending")
    sort_asc_action.triggered.connect(lambda: self.sort_by_column(column_name, True))

    # Sort descending
    sort_desc_action = menu.addAction("Sort Descending")
```

### Updated Code

```python
from i18n import t

def show_context_menu(self, pos):
    """Show context menu for column operations"""
    # ... code ...

    menu = QMenu(self)

    # Copy column name
    copy_action = menu.addAction(t("context.copy_column"))  # ✅ Translated
    copy_action.triggered.connect(lambda: self.copy_column_name(column_name))

    # Sort ascending
    sort_asc_action = menu.addAction(t("context.sort_asc"))  # ✅ Translated
    sort_asc_action.triggered.connect(lambda: self.sort_by_column(column_name, True))

    # Sort descending
    sort_desc_action = menu.addAction(t("context.sort_desc"))  # ✅ Translated
```

## Example 8: Theme Selector Dialog

### Original Code (theme_selector.py lines 75-91)

```python
def setup_ui(self):
    """Setup theme selector UI"""
    self.setWindowTitle("Theme Selector")
    self.setModal(True)
    self.resize(800, 600)

    layout = QVBoxLayout(self)

    # Title
    title = QLabel("Select a theme for ParVu")
    title.setStyleSheet("font-size: 14pt; font-weight: bold; padding: 10px;")
    layout.addWidget(title)

    # Main content
    content_layout = QHBoxLayout()

    # Left: Theme list
    list_layout = QVBoxLayout()
    list_label = QLabel("Available Themes:")
```

### Updated Code

```python
from i18n import t

def setup_ui(self):
    """Setup theme selector UI"""
    self.setWindowTitle(t("dialog.theme_selector"))  # ✅ Translated
    self.setModal(True)
    self.resize(800, 600)

    layout = QVBoxLayout(self)

    # Title
    title = QLabel(t("theme.selector.title"))  # ✅ Translated
    title.setStyleSheet("font-size: 14pt; font-weight: bold; padding: 10px;")
    layout.addWidget(title)

    # Main content
    content_layout = QHBoxLayout()

    # Left: Theme list
    list_layout = QVBoxLayout()
    list_label = QLabel(t("theme.selector.available"))  # ✅ Translated
```

## Complete Integration Checklist

### For each Python file with UI:

1. ✅ Add import at top: `from i18n import t`

2. ✅ Replace all hardcoded strings:
   - Window titles → `t("dialog.*")`
   - Button labels → `t("btn.*")`
   - Menu items → `t("menu.*")`
   - Labels → `t("label.*")` or `t("settings.label.*")`
   - Error messages → `t("error.*")`
   - Status messages → `t("status.*")`
   - Tooltips → `t("settings.tooltip.*")`

3. ✅ Update formatted strings:
   ```python
   # Before
   f"Loaded: {filename} ({rows} rows)"

   # After
   t("status.loaded", filename=filename, rows=rows)
   ```

4. ✅ Test all three languages

## Files to Update

| File | Priority | Estimated Strings |
|------|----------|-------------------|
| main_window.py | HIGH | 40+ |
| settings_dialog.py | HIGH | 50+ |
| theme_selector.py | MEDIUM | 15+ |
| table_view.py | MEDIUM | 10+ |
| sql_editor.py | LOW | 0 (syntax only) |

## Testing Procedure

1. **English (en)**
   ```python
   from i18n import get_i18n
   i18n = get_i18n()
   i18n.set_locale("en")
   # Launch app and test all dialogs
   ```

2. **Russian (ru)**
   ```python
   i18n.set_locale("ru")
   # Verify Cyrillic text displays correctly
   # Check that text fits in buttons/labels
   ```

3. **Azerbaijani (az)**
   ```python
   i18n.set_locale("az")
   # Verify Latin script with special characters
   # Test all dialogs and menus
   ```

## Common Patterns

### Pattern 1: Simple Labels
```python
# Before
QLabel("Table Variable Name:")

# After
QLabel(t("settings.label.table_var"))
```

### Pattern 2: Buttons
```python
# Before
QPushButton("Browse & Load...")

# After
QPushButton(t("btn.browse"))
```

### Pattern 3: Dialog Messages
```python
# Before
QMessageBox.warning(self, "Error", "File not found")

# After
QMessageBox.warning(
    self,
    t("error.file_not_found"),
    t("error.file_not_found_msg", path=file_path)
)
```

### Pattern 4: Status Bar
```python
# Before
self.statusBar().showMessage("Ready")

# After
self.statusBar().showMessage(t("status.ready"))
```

### Pattern 5: Placeholders
```python
# Before
self.edit.setPlaceholderText("Search values...")

# After
self.edit.setPlaceholderText(t("label.search_placeholder"))
```

## Notes

- The `t()` function is a shorthand for `get_i18n().t()`
- All translation keys are defined in `src/i18n.py`
- Missing translations will return the key itself (e.g., `"missing.key"`)
- Formatting errors are logged but won't crash the app
- Language changes require app restart for full effect

For questions, see [I18N.md](I18N.md) or open an issue.
