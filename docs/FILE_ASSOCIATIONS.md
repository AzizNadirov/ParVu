# File Associations Guide

ParVu can be set as the default application or added to "Open With" menu for Parquet, CSV, and JSON files.

## Supported File Types

ParVu supports opening:
- **Parquet files**: `.parquet`, `.pq`
- **CSV files**: `.csv`
- **JSON files**: `.json`

## Linux (Ubuntu/Debian)

### Automatic Setup (.deb package)

When you install ParVu via the `.deb` package, file associations are automatically configured:

```bash
sudo dpkg -i ParVu-0.2.0-amd64.deb
```

The installer will:
- ✅ Register MIME types for Parquet files
- ✅ Add ParVu to the "Open With" menu
- ✅ Update the desktop database
- ✅ Make ParVu available for file associations

### Setting Default Application

#### Method 1: Right-Click Menu (Recommended)

1. Right-click on a `.parquet`, `.csv`, or `.json` file
2. Select **Properties**
3. Go to the **Open With** tab
4. Select **ParVu** from the list
5. Click **Set as default**

#### Method 2: File Manager Settings

**GNOME Files (Nautilus):**
1. Right-click file → **Open With Other Application**
2. Select **ParVu** or click **View All Applications**
3. Find and select **ParVu**
4. Check **Set as default**
5. Click **Select**

**KDE Dolphin:**
1. Right-click file → **Properties**
2. Click **File Type Options**
3. Select **ParVu** from the application list
4. Click **Apply**

#### Method 3: Command Line

Set ParVu as default for Parquet files:
```bash
xdg-mime default parvu.desktop application/x-parquet
xdg-mime default parvu.desktop application/vnd.apache.parquet
```

Set for CSV files (optional):
```bash
xdg-mime default parvu.desktop text/csv
```

Set for JSON files (optional):
```bash
xdg-mime default parvu.desktop application/json
```

Verify associations:
```bash
xdg-mime query default application/x-parquet
# Should output: parvu.desktop
```

### Manual Installation (Standalone Binary)

If using the standalone binary, you can manually install the desktop file:

```bash
# Copy desktop file
sudo cp /path/to/dist/parvu/parvu.desktop /usr/share/applications/

# Update desktop database
sudo update-desktop-database /usr/share/applications/
```

Then follow the "Setting Default Application" steps above.

## Windows

### Automatic Setup (Installer)

When installing ParVu using the Windows installer, you'll see an option:

**☐ Associate .parquet, .csv, and .json files**

If you check this box during installation:
- ✅ `.parquet` files will open with ParVu by default
- ✅ ParVu will appear in "Open With" menu for CSV and JSON files
- ✅ File icons will show ParVu's icon

### Manual Setup

#### Method 1: Right-Click (Easiest)

1. Right-click on a `.parquet`, `.csv`, or `.json` file
2. Select **Open with** → **Choose another app**
3. Click **More apps** ↓
4. Scroll down and click **Look for another app on this PC**
5. Navigate to ParVu installation folder (usually `C:\Program Files\ParVu\`)
6. Select `parvu.exe`
7. Check **☑ Always use this app to open .parquet files**
8. Click **OK**

#### Method 2: Default Apps Settings

**Windows 10/11:**
1. Open **Settings** (Win + I)
2. Go to **Apps** → **Default apps**
3. Scroll down and click **Choose default apps by file type**
4. Find `.parquet` in the list
5. Click the current app (or **Choose a default**)
6. Select **ParVu**

#### Method 3: File Properties

1. Right-click on a file
2. Select **Properties**
3. Click **Change** next to "Opens with:"
4. Select **ParVu** from the list
5. Click **OK**

### Portable Version

The portable version doesn't automatically register file associations. Use the manual methods above to associate files with ParVu.

## macOS (Future Support)

macOS support is planned for future releases. The process will be similar to other platforms:

1. Right-click file → **Get Info**
2. Under **Open With**, select **ParVu**
3. Click **Change All** to apply to all similar files

## Usage Examples

### Opening Files from File Manager

Once configured, you can:
- **Double-click** any associated file to open it in ParVu
- **Right-click** → **Open with ParVu** for quick access
- **Drag and drop** files onto the ParVu window

### Opening from Command Line

**Linux:**
```bash
parvu file.parquet
parvu data.csv
parvu config.json
```

**Windows:**
```powershell
# If installed
parvu.exe file.parquet

# Portable version
C:\path\to\parvu.exe file.parquet
```

### Opening Multiple Files

ParVu currently opens one file at a time. Opening a second file will replace the current file in the viewer.

## Troubleshooting

### Linux: "ParVu doesn't appear in Open With menu"

**Solution:**
```bash
# Update desktop database
sudo update-desktop-database /usr/share/applications/

# Update MIME database
sudo update-mime-database /usr/share/mime/

# Clear icon cache
rm -rf ~/.cache/icon-theme.cache
```

Log out and log back in for changes to take effect.

### Linux: "File association resets after reboot"

**Check if desktop file exists:**
```bash
ls -la /usr/share/applications/parvu.desktop
```

**If missing, reinstall:**
```bash
sudo dpkg -i ParVu-*.deb
```

### Windows: "ParVu not in Open With list"

**Solution 1: Add manually**
1. Right-click file → **Open with** → **Choose another app**
2. Click **Look for another app on this PC**
3. Navigate to ParVu installation

**Solution 2: Reinstall with file associations**
- Uninstall ParVu
- Reinstall and check "Associate files" option

### Windows: "File icons don't change"

**Refresh icon cache:**
1. Open Command Prompt as Administrator
2. Run:
   ```cmd
   ie4uinit.exe -show
   ```
3. Restart Windows Explorer:
   ```cmd
   taskkill /f /im explorer.exe
   start explorer.exe
   ```

### Application Opens Wrong File

**Verify the file path:**
- Make sure the file path doesn't contain special characters
- Try copying the file to a simpler path (e.g., Desktop)
- Check file permissions

### Opening Multiple Files Shows Only Last One

This is expected behavior. ParVu is designed to view one file at a time. To view multiple files, open multiple instances of ParVu.

## File Association Details

### Linux MIME Types

ParVu registers these MIME types:
- `application/x-parquet` - Standard Parquet MIME type
- `application/vnd.apache.parquet` - Apache Parquet vendor type
- `text/csv` - CSV files (shared with other apps)
- `text/comma-separated-values` - Alternative CSV type
- `application/json` - JSON files (shared with other apps)

### Windows Registry Keys

The installer creates these registry entries (when file associations are enabled):

**Parquet files:**
- `HKCR\.parquet` → `ParVu.ParquetFile`
- `HKCR\.pq` → `ParVu.ParquetFile`
- `HKCR\ParVu.ParquetFile\shell\open\command` → ParVu executable path

**CSV/JSON files:**
- Added to `Applications\parvu.exe\SupportedTypes`
- Added to `OpenWithProgids` for "Open With" menu

## Best Practices

### Recommended File Associations

**Always associate:**
- ✅ `.parquet` files - ParVu's primary purpose
- ✅ `.pq` files - Short Parquet extension

**Optional (depends on workflow):**
- ⚠️ `.csv` files - You may prefer Excel/LibreOffice
- ⚠️ `.json` files - You may prefer text editor/IDE

### Multi-Purpose Files (CSV, JSON)

For CSV and JSON files that may be used by multiple applications:

**Linux:**
- Don't set ParVu as default
- Use right-click → "Open With" → ParVu when needed

**Windows:**
- Don't check "Associate files" during installation
- Add ParVu to "Open With" menu manually (see Method 1 above)
- Use Shift + Right-click for extended "Open With" menu

## Uninstalling File Associations

### Linux

```bash
# Remove file associations
xdg-mime default org.gnome.gedit.desktop application/x-parquet

# Or remove parvu.desktop entirely
sudo rm /usr/share/applications/parvu.desktop
sudo update-desktop-database /usr/share/applications/
```

### Windows

**If installed with file associations:**
- Uninstall ParVu via Control Panel
- File associations will be automatically removed

**If manually associated:**
1. Right-click file → **Open with** → **Choose another app**
2. Select different application
3. Check "Always use this app"

Or use Default Apps settings to change associations.

## Advanced: System-Wide vs User-Only

### Linux

**System-wide** (requires sudo):
```bash
sudo cp parvu.desktop /usr/share/applications/
```

**User-only:**
```bash
mkdir -p ~/.local/share/applications
cp parvu.desktop ~/.local/share/applications/
update-desktop-database ~/.local/share/applications/
```

### Windows

Windows installer creates **system-wide** associations (all users).

For **user-only** associations with portable version:
- Use the manual right-click method
- Don't use installer's association option

## Security Note

When setting file associations:
- ✅ Only associate ParVu with data files (.parquet, .csv, .json)
- ❌ Never associate with executable file types (.exe, .sh, .bat)
- ✅ Verify ParVu's authenticity before associating files
- ✅ Download ParVu only from official sources

## See Also

- [README.md](README.md) - Main documentation
- [RELEASES.md](RELEASES.md) - Installation guide
- [BUILD.md](BUILD.md) - Building from source
