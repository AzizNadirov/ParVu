# ParVu Release Builds

## Available Build Types

### Linux

#### 1. Debian Package (.deb)
**Recommended for Ubuntu/Debian/Mint users**

- **File**: `ParVu-X.Y.Z-amd64.deb`
- **Size**: ~80-100 MB
- **Installation**:
  ```bash
  sudo dpkg -i ParVu-X.Y.Z-amd64.deb
  ```
- **Launch**:
  - From terminal: `parvu`
  - From application menu: Search for "ParVu"
- **Uninstall**:
  ```bash
  sudo apt remove parvu
  ```

**Features**:
- ✅ Integrates with system
- ✅ Desktop entry in application menu
- ✅ Can be launched from any directory
- ✅ Clean uninstall process

#### 2. Portable Archive (.tar.gz)
**For other Linux distributions**

- **File**: `ParVu-X.Y.Z-linux-x86_64.tar.gz`
- **Size**: ~80-100 MB
- **Usage**:
  ```bash
  tar xzf ParVu-X.Y.Z-linux-x86_64.tar.gz
  cd parvu
  ./parvu
  ```

**Features**:
- ✅ No installation required
- ✅ Works on any Linux distribution
- ✅ Can run from USB drive
- ❌ No system integration
- ❌ Must run from extracted directory

### Windows

#### 1. Installer (.exe)
**Recommended for most users**

- **File**: `ParVu-X.Y.Z-setup-win64.exe`
- **Size**: ~90-110 MB
- **Installation**: Double-click and follow wizard
- **Launch**: Start menu or desktop shortcut
- **Uninstall**: Control Panel → Programs

**Features**:
- ✅ Professional installation wizard
- ✅ Start menu shortcuts
- ✅ Optional desktop icon
- ✅ Optional file associations (.parquet, .csv, .json)
- ✅ Proper Windows integration
- ✅ Clean uninstall
- ⚠️ Requires administrator rights

#### 2. Portable Version (.zip)
**For users without admin rights**

- **File**: `ParVu-X.Y.Z-portable-win64.zip`
- **Size**: ~90-110 MB
- **Usage**:
  - Extract to any folder
  - Run `parvu.exe`

**Features**:
- ✅ No installation required
- ✅ No admin rights needed
- ✅ Can run from USB drive
- ✅ Multiple versions can coexist
- ❌ No file associations
- ❌ No start menu integration

## System Requirements

### Minimum Requirements
- **OS**: Windows 10+ / Ubuntu 20.04+ / Debian 10+ / Linux with glibc 2.31+
- **RAM**: 4 GB
- **Disk**: 500 MB free space
- **Display**: 1280x720 or higher

### Recommended
- **RAM**: 8 GB+ (for large files)
- **Disk**: 1 GB free space
- **Display**: 1920x1080 or higher

## Which Version Should I Download?

### Linux Users

| If you use... | Download this |
|---------------|---------------|
| Ubuntu, Debian, Linux Mint, Pop!_OS, elementary OS | `.deb` package |
| Fedora, RHEL, CentOS, openSUSE | `.tar.gz` portable |
| Arch, Manjaro | `.tar.gz` portable |
| Other distributions | `.tar.gz` portable |

### Windows Users

| If you... | Download this |
|-----------|---------------|
| Want easy installation and updates | `.exe` installer |
| Have admin rights | `.exe` installer |
| Want file associations | `.exe` installer |
| Don't have admin rights | `.zip` portable |
| Want to run from USB | `.zip` portable |
| Want multiple versions | `.zip` portable |

## First Run

After installing/extracting ParVu:

1. **Launch the application**
   - Linux .deb: Type `parvu` in terminal or find in app menu
   - Linux portable: `cd parvu && ./parvu`
   - Windows installer: Start menu → ParVu
   - Windows portable: Double-click `parvu.exe`

2. **Open a file**
   - Click "Browse" and select a .parquet, .csv, or .json file
   - Or drag and drop a file into the window
   - Or pass file path as argument: `parvu myfile.parquet`

3. **Start querying**
   - View your data in the table
   - Write SQL queries in the editor
   - Export results in multiple formats

## Updating

### Linux .deb
```bash
# Download new version
sudo dpkg -i ParVu-X.Y.Z-amd64.deb
```

### Linux Portable
```bash
# Extract new version to new folder
tar xzf ParVu-X.Y.Z-linux-x86_64.tar.gz
# Delete old version if desired
```

### Windows Installer
- Download and run new installer
- It will update the existing installation

### Windows Portable
- Extract new version to new folder
- Copy your settings from old version (optional)

## Settings Location

Your settings and data are stored separately from the application:

- **Linux**: `~/.ParVu/`
  - Settings: `~/.ParVu/settings/settings.json`
  - Themes: `~/.ParVu/themes/`
  - Logs: `~/.ParVu/logs/`

- **Windows**: `%APPDATA%\ParVu\`
  - Settings: `%APPDATA%\ParVu\settings\settings.json`
  - Themes: `%APPDATA%\ParVu\themes\`
  - Logs: `%APPDATA%\ParVu\logs\`

**Note**: Settings persist across updates and reinstalls.

## Troubleshooting

### Linux

**"Command not found: parvu"**
- If using .deb: Try `sudo dpkg -i --reinstall ParVu-*.deb`
- If using portable: You must run `./parvu` from the extracted directory

**"Permission denied"**
```bash
chmod +x parvu
./parvu
```

**Missing libraries**
```bash
# Ubuntu/Debian
sudo apt install libxcb-cursor0 libtiff5
```

### Windows

**"Windows protected your PC"**
- This is normal for unsigned executables
- Click "More info" → "Run anyway"
- Or right-click → Properties → Unblock

**Application won't start**
- Make sure you've extracted the entire .zip (not just the .exe)
- Try running from a folder without special characters or spaces
- Check logs in `%APPDATA%\ParVu\logs\`

**"VCRUNTIME140.dll is missing"**
- Install [Microsoft Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe)

### Common Issues

**Application is slow to start**
- First launch is always slower (PyQt6 initialization)
- Subsequent launches are faster

**Large file size**
- The application bundles Python and all dependencies
- This ensures it works without requiring Python installation

**Antivirus warnings**
- Some antivirus software flags PyInstaller executables
- This is a false positive
- You can verify the source code at https://github.com/AzizNadirov/ParVu

## Verifying Downloads

### Check file integrity

**Linux**:
```bash
sha256sum ParVu-*.deb
sha256sum ParVu-*.tar.gz
```

**Windows**:
```powershell
Get-FileHash ParVu-*.exe -Algorithm SHA256
Get-FileHash ParVu-*.zip -Algorithm SHA256
```

Compare the hash with the one provided in the release notes.

## Support

- **Documentation**: [README.md](README.md)
- **Build Guide**: [BUILD.md](BUILD.md)
- **Issues**: https://github.com/AzizNadirov/ParVu/issues
- **Discussions**: https://github.com/AzizNadirov/ParVu/discussions

## License

ParVu is open source software. See the LICENSE file for details.
