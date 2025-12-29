# Build script for ParVu - Windows build
# Creates both an installable .exe and a portable version

param(
    [switch]$SkipInstaller = $false
)

Write-Host "===================================" -ForegroundColor Blue
Write-Host "ParVu Windows Build Script" -ForegroundColor Blue
Write-Host "===================================" -ForegroundColor Blue

# Get version from pyproject.toml
$version = (Get-Content pyproject.toml | Select-String '^version').ToString().Split('"')[1]
Write-Host "Building ParVu v$version" -ForegroundColor Cyan

# Clean previous builds
Write-Host "`nCleaning previous builds..." -ForegroundColor Yellow
Remove-Item -Recurse -Force build, dist -ErrorAction SilentlyContinue
Remove-Item ParVu-*.exe -ErrorAction SilentlyContinue

# Install build dependencies
Write-Host "`nInstalling build dependencies..." -ForegroundColor Yellow
uv sync --extra build

# Create spec file if it doesn't exist
if (-not (Test-Path "parvu.spec")) {
    Write-Host "`nCreating parvu.spec file..." -ForegroundColor Yellow
    @'
# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec file for ParVu - Parquet Viewer
Builds a cross-platform application bundle
"""

import sys
from PyInstaller.utils.hooks import collect_data_files, collect_submodules

block_cipher = None

# Collect all data files from important packages
datas = []
datas += collect_data_files('duckdb')
datas += collect_data_files('pyarrow')

# Add data directories from src
datas += [('src/settings', 'settings')]
datas += [('src/static', 'static')]
datas += [('src/history', 'history')]

# Collect hidden imports that PyInstaller might miss
hiddenimports = [
    'PyQt6.QtCore',
    'PyQt6.QtGui',
    'PyQt6.QtWidgets',
    'duckdb',
    'pyarrow',
    'pandas',
    'openpyxl',
    'loguru',
    'pydantic',
    'dateutil',
]

# Add all submodules from key packages
hiddenimports += collect_submodules('duckdb')
hiddenimports += collect_submodules('pyarrow')
hiddenimports += collect_submodules('pandas')

a = Analysis(
    ['src/app.py'],
    pathex=['src'],
    binaries=[],
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[
        'matplotlib',
        'tkinter',
        'numpy.distutils',
        'scipy',
    ],
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name='parvu',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon='assets/parvu.ico',
)

coll = COLLECT(
    exe,
    a.binaries,
    a.zipfiles,
    a.datas,
    strip=False,
    upx=True,
    upx_exclude=[],
    name='parvu',
)
'@ | Out-File -Encoding UTF8 "parvu.spec"
    Write-Host "✓ Created parvu.spec" -ForegroundColor Green
}

# Build with PyInstaller
Write-Host "`nBuilding with PyInstaller..." -ForegroundColor Yellow
uv run pyinstaller parvu.spec --clean --noconfirm

# Check if build succeeded
if (Test-Path "dist\parvu") {
    Write-Host "✓ Build successful!" -ForegroundColor Green
} else {
    Write-Host "ERROR: Build failed!" -ForegroundColor Red
    exit 1
}

# Create portable version (zip)
Write-Host "`nCreating portable version..." -ForegroundColor Yellow
$portableName = "ParVu-$version-portable-win64.zip"
Compress-Archive -Path "dist\parvu\*" -DestinationPath $portableName -Force
Write-Host "✓ Portable version created: $portableName" -ForegroundColor Green

# Create installer with Inno Setup (if available and not skipped)
if (-not $SkipInstaller) {
    Write-Host "`nCreating installer..." -ForegroundColor Yellow

    # Check if Inno Setup is installed
    $innoPath = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"

    if (Test-Path $innoPath) {
        # Create Inno Setup script
        $issScript = @"
#define MyAppName "ParVu"
#define MyAppVersion "$version"
#define MyAppPublisher "ParVu Developers"
#define MyAppURL "https://github.com/AzizNadirov/ParVu"
#define MyAppExeName "parvu.exe"

[Setup]
AppId={{A5B6C7D8-E9F0-1234-5678-9ABCDEF01234}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir=.
OutputBaseFilename=ParVu-{#MyAppVersion}-setup-win64
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64
PrivilegesRequired=admin
SetupIconFile=assets\parvu.ico
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "associatefiles"; Description: "Associate .parquet, .csv, and .json files"; GroupDescription: "File associations:"; Flags: unchecked

[Files]
Source: "dist\parvu\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Registry]
; Parquet file associations
Root: HKCR; Subkey: ".parquet"; ValueType: string; ValueName: ""; ValueData: "ParVu.ParquetFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".pq"; ValueType: string; ValueName: ""; ValueData: "ParVu.ParquetFile"; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.ParquetFile"; ValueType: string; ValueName: ""; ValueData: "Parquet File"; Flags: uninsdeletekey; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.ParquetFile"; ValueType: string; ValueName: "FriendlyTypeName"; ValueData: "Apache Parquet File"; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.ParquetFile\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\{#MyAppExeName},0"; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.ParquetFile\shell\open"; ValueType: string; ValueName: ""; ValueData: "Open with ParVu"; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.ParquetFile\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associatefiles

; CSV file associations (optional - many users prefer Excel)
Root: HKCR; Subkey: "Applications\{#MyAppExeName}\SupportedTypes"; ValueType: string; ValueName: ".csv"; ValueData: ""; Tasks: associatefiles
Root: HKCR; Subkey: "Applications\{#MyAppExeName}\SupportedTypes"; ValueType: string; ValueName: ".json"; ValueData: ""; Tasks: associatefiles
Root: HKCR; Subkey: "Applications\{#MyAppExeName}\SupportedTypes"; ValueType: string; ValueName: ".parquet"; ValueData: ""; Tasks: associatefiles
Root: HKCR; Subkey: "Applications\{#MyAppExeName}\SupportedTypes"; ValueType: string; ValueName: ".pq"; ValueData: ""; Tasks: associatefiles

; Add to "Open With" context menu for CSV and JSON files
Root: HKCR; Subkey: ".csv\OpenWithProgids"; ValueType: string; ValueName: "ParVu.CSVFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associatefiles
Root: HKCR; Subkey: ".json\OpenWithProgids"; ValueType: string; ValueName: "ParVu.JSONFile"; ValueData: ""; Flags: uninsdeletevalue; Tasks: associatefiles

; CSV handler
Root: HKCR; Subkey: "ParVu.CSVFile"; ValueType: string; ValueName: ""; ValueData: "CSV File"; Flags: uninsdeletekey; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.CSVFile\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associatefiles

; JSON handler
Root: HKCR; Subkey: "ParVu.JSONFile"; ValueType: string; ValueName: ""; ValueData: "JSON File"; Flags: uninsdeletekey; Tasks: associatefiles
Root: HKCR; Subkey: "ParVu.JSONFile\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#MyAppExeName}"" ""%1"""; Tasks: associatefiles
"@

        $issScript | Out-File -Encoding UTF8 "parvu_installer.iss"

        # Build installer
        & $innoPath "parvu_installer.iss"

        if ($LASTEXITCODE -eq 0) {
            Write-Host "✓ Installer created successfully!" -ForegroundColor Green
        } else {
            Write-Host "⚠ Installer creation failed" -ForegroundColor Yellow
        }

        # Cleanup
        Remove-Item "parvu_installer.iss" -ErrorAction SilentlyContinue
    } else {
        Write-Host "⚠ Inno Setup not found at: $innoPath" -ForegroundColor Yellow
        Write-Host "  Download from: https://jrsoftware.org/isdl.php" -ForegroundColor Yellow
        Write-Host "  Installer creation skipped." -ForegroundColor Yellow
    }
}

Write-Host "`n===================================" -ForegroundColor Green
Write-Host "Build completed successfully!" -ForegroundColor Green
Write-Host "===================================" -ForegroundColor Green
Write-Host "`nOutputs:"
Write-Host "  1. Standalone folder: dist\parvu\" -ForegroundColor Cyan
Write-Host "  2. Portable version: $portableName" -ForegroundColor Cyan

if (-not $SkipInstaller -and (Test-Path "ParVu-$version-setup-win64.exe")) {
    Write-Host "  3. Installer: ParVu-$version-setup-win64.exe" -ForegroundColor Cyan
}

Write-Host "`nTo run the application:" -ForegroundColor Yellow
Write-Host "  cd dist\parvu" -ForegroundColor White
Write-Host "  .\parvu.exe" -ForegroundColor White
