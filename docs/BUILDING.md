# ParVu Build Guide

Complete guide for building ParVu cross-platform applications for Linux and Windows.

## Table of Contents

- [Quick Start](#quick-start)
- [Prerequisites](#prerequisites)
- [Building](#building)
- [Build Outputs](#build-outputs)
- [File Associations](#file-associations)
- [Distribution](#distribution)
- [Advanced Topics](#advanced-topics)
- [Troubleshooting](#troubleshooting)

---

## Quick Start

### Linux
```bash
./build.sh
```

### Windows
```powershell
.\build.ps1
```

That's it! The build scripts handle everything automatically.

---

## Prerequisites

### All Platforms
- **uv** package manager (recommended) or Python 3.13+
- **Git** (for cloning the repository)

**Install uv:**
```bash
# Linux/macOS
curl -LsSf https://astral.sh/uv/install.sh | sh

# Windows
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

### Linux Specific
- `build-essential` (usually pre-installed)
- `dpkg-deb` (for creating .deb packages)

### Windows Specific
- **PowerShell 5.0+** (pre-installed on Windows 10+)
- **Inno Setup 6** (optional, for creating installers)
  - Download: https://jrsoftware.org/isdl.php

---

## Building

### First-Time Setup

1. **Clone the repository:**
   ```bash
   git clone https://github.com/AzizNadirov/ParVu.git
   cd ParVu
   ```

2. **Install dependencies:**
   ```bash
   uv sync --extra build
   ```

3. **Build the application:**
   ```bash
   # Linux
   ./build.sh

   # Windows
   .\build.ps1
   ```

### Build from Scratch

If you've deleted everything or are building for the first time:

```bash
# Clean everything
rm -rf build dist *.deb *.zip parvu.spec

# Build (script auto-creates parvu.spec if missing)
./build.sh
```

### Build Scripts

#### Linux: `build.sh`

```bash
./build.sh
```

**What it does:**
1. ✅ Installs build dependencies
2. ✅ Auto-creates `parvu.spec` if missing
3. ✅ Builds standalone binary with PyInstaller
4. ✅ Creates Debian (.deb) package
5. ✅ Sets up file associations
6. ✅ Creates post-install scripts

**Creates:**
- `dist/parvu/` - Standalone binary (~467 MB)
- `ParVu-0.2.0-amd64.deb` - Debian package (~131 MB)

#### Windows: `build.ps1`

```powershell
.\build.ps1

# Skip installer creation
.\build.ps1 -SkipInstaller
```

**What it does:**
1. ✅ Installs build dependencies
2. ✅ Auto-creates `parvu.spec` if missing
3. ✅ Builds standalone executable with PyInstaller
4. ✅ Creates portable ZIP archive
5. ✅ Creates installer with Inno Setup (if installed)
6. ✅ Sets up file associations

**Creates:**
- `dist\parvu\` - Standalone folder (~280-320 MB)
- `ParVu-0.2.0-portable-win64.zip` - Portable ZIP (~90-110 MB)
- `ParVu-0.2.0-setup-win64.exe` - Full installer (~90-110 MB)

### Using Makefile (Linux)

```bash
make install    # Install dependencies
make build      # Build standalone binary only
make deb        # Build binary + .deb package
make clean      # Clean build artifacts
make run        # Run from source (development)
make test       # Test built application
```

### Manual PyInstaller Build

```bash
# Install dependencies
uv sync --extra build

# Build
uv run pyinstaller parvu.spec --clean --noconfirm

# Result in dist/parvu/
```

---

## Build Outputs

### Linux

#### 1. Standalone Binary
- **Location**: `dist/parvu/`
- **Size**: ~467 MB (uncompressed)
- **Usage**:
  ```bash
  cd dist/parvu
  ./parvu [file.parquet]
  ```
- **Distribution**: Package as `.tar.gz`

#### 2. Debian Package
- **File**: `ParVu-0.2.0-amd64.deb`
- **Size**: ~131 MB (compressed)
- **Installation**:
  ```bash
  sudo dpkg -i ParVu-0.2.0-amd64.deb
  ```
- **Launch**: `parvu` command or application menu
- **Uninstall**:
  ```bash
  sudo apt remove parvu
  ```

**Features:**
- ✅ System-wide installation
- ✅ Desktop integration
- ✅ File associations (Parquet, CSV, JSON)
- ✅ Application menu entry
- ✅ MIME type registration

### Windows

#### 1. Standalone Folder
- **Location**: `dist\parvu\`
- **Size**: ~280-320 MB
- **Usage**: Run `parvu.exe`
- **Distribution**: Use the portable ZIP

#### 2. Portable ZIP
- **File**: `ParVu-0.2.0-portable-win64.zip`
- **Size**: ~90-110 MB (compressed)
- **Usage**:
  - Extract anywhere
  - Run `parvu.exe`
- **Pros**: No installation, runs from USB, no admin rights needed

#### 3. Installer (Optional)
- **File**: `ParVu-0.2.0-setup-win64.exe`
- **Size**: ~90-110 MB
- **Requires**: Inno Setup 6 to build
- **Features**:
  - Professional installation wizard
  - Start menu shortcuts
  - Desktop icon (optional)
  - File associations (optional)
  - Clean uninstaller

---

## File Associations

ParVu can be set as the default application for data files.

### Supported File Types

| Extension | Linux | Windows | Notes |
|-----------|-------|---------|-------|
| `.parquet` | ✅ | ✅ | Primary file type |
| `.pq` | ✅ | ✅ | Short Parquet extension |
| `.csv` | ✅ | ✅ | Optional (many prefer Excel) |
| `.json` | ✅ | ✅ | Optional (many prefer text editor) |

### Linux Setup

**Automatic (via .deb package):**

The .deb package automatically:
- Registers MIME types for Parquet files
- Adds ParVu to "Open With" menu
- Updates desktop and MIME databases

**Set as default:**
1. Right-click on a `.parquet` file
2. Select **Properties** → **Open With** tab
3. Select **ParVu**
4. Click **Set as default**

**Command line:**
```bash
xdg-mime default parvu.desktop application/x-parquet
```

### Windows Setup

**Automatic (via installer):**

During installation, check:
```
☑ Associate .parquet, .csv, and .json files
```

**Manual setup:**
1. Right-click on a file
2. **Open with** → **Choose another app**
3. Click **More apps** → **Look for another app on this PC**
4. Navigate to ParVu installation
5. Select `parvu.exe`
6. Check **Always use this app**

### Usage After Association

```bash
# Double-click files to open in ParVu
# Or command line:

# Linux
parvu file.parquet

# Windows
parvu.exe file.parquet
```

**See [FILE_ASSOCIATIONS.md](FILE_ASSOCIATIONS.md) for detailed guide.**

---

## Distribution

### For Linux Users

**Ubuntu/Debian/Mint users:**
- Distribute: `ParVu-0.2.0-amd64.deb`
- Installation: `sudo dpkg -i ParVu-0.2.0-amd64.deb`

**Other distributions:**
- Distribute: `.tar.gz` archive of `dist/parvu/`
- Extraction: `tar xzf ParVu-0.2.0-linux-x86_64.tar.gz`
- Usage: `cd parvu && ./parvu`

### For Windows Users

**Recommended:**
- Distribute: `ParVu-0.2.0-setup-win64.exe` (installer)
- Best user experience, proper Windows integration

**Alternative:**
- Distribute: `ParVu-0.2.0-portable-win64.zip`
- No installation required, runs from any folder

### Creating Release Archives

```bash
# Linux portable archive
cd dist
tar czf ../ParVu-0.2.0-linux-x86_64.tar.gz parvu/

# Verify
tar tzf ParVu-0.2.0-linux-x86_64.tar.gz | head
```

---

## Advanced Topics

### Customization

#### Change Version

Edit `pyproject.toml`:
```toml
version = "0.3.0"
```

Rebuild - all outputs automatically use new version.

#### Add Application Icon

1. Create/obtain icon file:
   - Windows: `.ico` format
   - Linux: `.png` or `.svg` format

2. Edit `parvu.spec`:
   ```python
   exe = EXE(
       ...
       icon='path/to/icon.ico',
   )
   ```

3. Rebuild

#### Reduce Build Size

Edit `parvu.spec`:
```python
excludes=[
    'matplotlib',
    'tkinter',
    'numpy.distutils',
    'scipy',
    'IPython',    # Add more unused packages
    'jupyter',
],
```

#### Enable UPX Compression

Already enabled by default:
```python
upx=True,  # In both EXE and COLLECT
```

### Build Configuration

The `parvu.spec` file is automatically generated but you can customize:

**Key sections:**
- `datas` - Data files to include
- `hiddenimports` - Modules PyInstaller might miss
- `excludes` - Packages to exclude
- `console=False` - GUI mode (no console window)

### CI/CD Integration

GitHub Actions workflow included: `.github/workflows/build.yml`

**Automatic builds on:**
- Push to main branch
- Pull requests
- Git tags (creates releases)

**To create a release:**
```bash
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin v0.2.0
```

GitHub Actions will automatically:
- Build for Linux and Windows
- Create a GitHub release
- Upload all distribution files

---

## Troubleshooting

### Build Issues

#### "Spec file not found"

The build script should auto-create it. If not:
```bash
# Verify build.sh is executable
chmod +x build.sh

# Run build
./build.sh
```

#### "Module not found" during build

```bash
# Reinstall dependencies
uv sync --extra build

# Clean and rebuild
rm -rf build dist
./build.sh
```

#### Build succeeds but app won't run

**Linux - Check for missing libraries:**
```bash
ldd dist/parvu/parvu

# Install common dependencies
sudo apt install libxcb-cursor0 libtiff5
```

**Check application logs:**
```bash
# Linux
tail -f ~/.ParVu/logs/parvu_*.log

# Windows
type %APPDATA%\ParVu\logs\parvu_*.log
```

#### "Permission denied" (Linux)

```bash
chmod +x build.sh
chmod +x dist/parvu/parvu
```

### Runtime Issues

#### Application won't start

1. **Check Python version in build:**
   ```bash
   uv run python --version  # Should be 3.13+
   ```

2. **Verify all files extracted:**
   ```bash
   ls -la dist/parvu/_internal/  # Should contain many files
   ```

3. **Run with debug mode:**

   Edit `parvu.spec`:
   ```python
   exe = EXE(
       ...
       console=True,   # Show console for debugging
       debug=True,
   )
   ```

#### Missing DLLs (Windows)

Install [Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)

#### Import errors at runtime

Add missing module to `hiddenimports` in `parvu.spec`:
```python
hiddenimports = [
    'PyQt6.QtCore',
    'missing_module',  # Add here
    ...
]
```

### Platform-Specific Issues

#### Linux: Desktop integration not working

```bash
# Update databases
sudo update-desktop-database /usr/share/applications/
sudo update-mime-database /usr/share/mime/

# Reinstall package
sudo dpkg -i ParVu-*.deb
```

#### Windows: Installer not created

**Check if Inno Setup is installed:**
```powershell
Test-Path "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
```

**If False, either:**
- Install Inno Setup from https://jrsoftware.org/isdl.php
- Or use portable version: `.\build.ps1 -SkipInstaller`

#### Large build size

This is normal for PyInstaller builds:
- All dependencies are bundled
- Ensures app works without Python installation
- Users prefer this over dependency issues

**Typical sizes:**
- Linux: 467 MB uncompressed, 131 MB .deb
- Windows: 280-320 MB uncompressed, 90-110 MB .zip

### Getting Help

1. **Check logs** in `~/.ParVu/logs/` (Linux) or `%APPDATA%\ParVu\logs\` (Windows)
2. **Review PyInstaller docs**: https://pyinstaller.org/
3. **Check warnings** in `build/parvu/warn-parvu.txt`
4. **Open an issue**: https://github.com/AzizNadirov/ParVu/issues

---

## Build System Architecture

```
ParVu/
├── build.sh              # Linux build script
├── build.ps1             # Windows build script
├── parvu.spec           # PyInstaller config (auto-generated)
├── Makefile             # Convenience commands (Linux)
├── pyproject.toml       # Project config + dependencies
├── .github/
│   └── workflows/
│       └── build.yml    # CI/CD automation
└── src/                 # Source code
    ├── app.py          # Entry point
    ├── settings/       # Default settings
    ├── static/         # Static assets
    └── history/        # Recent files
```

### Build Process Flow

```
1. Install dependencies (uv sync --extra build)
2. Create parvu.spec (if missing)
3. Run PyInstaller
4. Create distribution packages
   ├── Linux: .deb with file associations
   └── Windows: .zip + installer
5. Success!
```

---

## Summary

### What You Need
- Source code
- `pyproject.toml`
- `build.sh` or `build.ps1`
- uv package manager

### What Gets Auto-Created
- `parvu.spec` (PyInstaller configuration)
- `dist/parvu/` (Application bundle)
- Distribution packages (.deb, .zip, .exe)

### One Command Build
```bash
./build.sh      # Linux
.\build.ps1     # Windows
```

### Zero Configuration
- No manual setup required
- Auto-generates all needed files
- Handles dependencies automatically
- Creates ready-to-distribute packages

---

## Quick Reference

### Common Commands

```bash
# Linux
./build.sh                    # Full build
make build                    # Binary only
make deb                      # Binary + .deb
make clean                    # Clean artifacts
parvu file.parquet           # Run installed version

# Windows
.\build.ps1                   # Full build
.\build.ps1 -SkipInstaller   # Skip installer
.\dist\parvu\parvu.exe       # Run portable version

# Clean build
rm -rf build dist *.deb *.zip parvu.spec
./build.sh
```

### File Sizes (Approximate)

| Output | Compressed | Uncompressed |
|--------|-----------|--------------|
| Linux .deb | 131 MB | 467 MB |
| Linux binary | - | 467 MB |
| Windows .zip | 90-110 MB | 280-320 MB |
| Windows installer | 90-110 MB | 280-320 MB |

### Build Time

- **First build**: 3-5 minutes (downloads dependencies)
- **Subsequent builds**: 1-2 minutes (uses cache)

---

**For detailed file association setup, see [FILE_ASSOCIATIONS.md](FILE_ASSOCIATIONS.md)**

**For technical implementation details, see [FILE_ASSOCIATIONS_SUMMARY.md](FILE_ASSOCIATIONS_SUMMARY.md)**
