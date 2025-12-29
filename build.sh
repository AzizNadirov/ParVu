#!/bin/bash
# Build script for ParVu - Linux build
# Creates both a standalone binary and a .deb package

set -e  # Exit on error

echo "==================================="
echo "ParVu Linux Build Script"
echo "==================================="

# Get version from pyproject.toml
VERSION=$(grep "^version" pyproject.toml | cut -d'"' -f2)
echo "Building ParVu v${VERSION}"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Clean previous builds
echo -e "${BLUE}Cleaning previous builds...${NC}"
rm -rf build dist ParVu*.deb

# Install build dependencies
echo -e "${BLUE}Installing build dependencies...${NC}"
uv sync --extra build

# Create spec file if it doesn't exist
if [ ! -f "parvu.spec" ]; then
    echo -e "${BLUE}Creating parvu.spec file...${NC}"
    cat > parvu.spec << 'SPECEOF'
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
    icon=None,
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
SPECEOF
    echo -e "${GREEN}✓ Created parvu.spec${NC}"
fi

# Build with PyInstaller
echo -e "${BLUE}Building standalone binary with PyInstaller...${NC}"
uv run pyinstaller parvu.spec --clean --noconfirm

# Check if build succeeded
if [ -d "dist/parvu" ]; then
    echo -e "${GREEN}✓ Standalone binary built successfully${NC}"
    echo "Location: dist/parvu/"
else
    echo "ERROR: Build failed!"
    exit 1
fi

# Create .deb package structure
echo -e "${BLUE}Creating .deb package...${NC}"

DEB_DIR="ParVu-${VERSION}-deb"
mkdir -p ${DEB_DIR}/DEBIAN
mkdir -p ${DEB_DIR}/usr/local/bin
mkdir -p ${DEB_DIR}/usr/share/applications
mkdir -p ${DEB_DIR}/usr/share/parvu
mkdir -p ${DEB_DIR}/usr/share/doc/parvu

# Copy binary
cp -r dist/parvu/* ${DEB_DIR}/usr/share/parvu/

# Create launcher script
cat > ${DEB_DIR}/usr/local/bin/parvu << 'EOF'
#!/bin/bash
cd /usr/share/parvu
exec ./parvu "$@"
EOF
chmod +x ${DEB_DIR}/usr/local/bin/parvu

# Create desktop entry with file associations
cat > ${DEB_DIR}/usr/share/applications/parvu.desktop << EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=ParVu
GenericName=Parquet File Viewer
Comment=View and query Parquet, CSV, and JSON files
Exec=/usr/local/bin/parvu %F
Icon=parvu
Terminal=false
Categories=Development;Utility;Database;
Keywords=parquet;csv;json;viewer;data;database;query;sql;
MimeType=application/x-parquet;application/vnd.apache.parquet;text/csv;text/comma-separated-values;application/json;
StartupNotify=true
EOF

# Create MIME type definitions for Parquet files
mkdir -p ${DEB_DIR}/usr/share/mime/packages
cat > ${DEB_DIR}/usr/share/mime/packages/parvu.xml << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-parquet">
    <comment>Apache Parquet file</comment>
    <comment xml:lang="en">Apache Parquet file</comment>
    <glob pattern="*.parquet"/>
    <glob pattern="*.pq"/>
    <magic priority="50">
      <match type="string" offset="0" value="PAR1"/>
    </magic>
    <icon name="parvu"/>
    <generic-icon name="x-office-spreadsheet"/>
  </mime-type>
  <mime-type type="application/vnd.apache.parquet">
    <comment>Apache Parquet file</comment>
    <glob pattern="*.parquet"/>
    <glob pattern="*.pq"/>
    <icon name="parvu"/>
  </mime-type>
</mime-info>
EOF

# Create postinst script to update MIME database and desktop database
cat > ${DEB_DIR}/DEBIAN/postinst << 'EOF'
#!/bin/bash
set -e

# Update MIME database
if [ -x /usr/bin/update-mime-database ]; then
    update-mime-database /usr/share/mime || true
fi

# Update desktop database
if [ -x /usr/bin/update-desktop-database ]; then
    update-desktop-database /usr/share/applications || true
fi

# Update icon cache
if [ -x /usr/bin/gtk-update-icon-cache ]; then
    gtk-update-icon-cache -f /usr/share/icons/hicolor || true
fi

echo "ParVu installed successfully!"
echo "You can now set ParVu as the default application for .parquet, .csv, and .json files"
echo "Right-click on a file → Properties → Open With → ParVu"

exit 0
EOF
chmod +x ${DEB_DIR}/DEBIAN/postinst

# Create postrm script to clean up
cat > ${DEB_DIR}/DEBIAN/postrm << 'EOF'
#!/bin/bash
set -e

if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
    # Update MIME database
    if [ -x /usr/bin/update-mime-database ]; then
        update-mime-database /usr/share/mime || true
    fi

    # Update desktop database
    if [ -x /usr/bin/update-desktop-database ]; then
        update-desktop-database /usr/share/applications || true
    fi
fi

exit 0
EOF
chmod +x ${DEB_DIR}/DEBIAN/postrm

# Create control file
cat > ${DEB_DIR}/DEBIAN/control << EOF
Package: parvu
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: amd64
Maintainer: ParVu Developers <https://github.com/AzizNadirov/ParVu>
Description: Parquet, CSV, and JSON File Viewer
 ParVu is a powerful desktop application for viewing and querying
 large Parquet, CSV, and JSON files. Built with PyQt6 and DuckDB
 for efficient data handling.
 .
 Features:
  - Support for Parquet, CSV, and JSON files
  - SQL querying with DuckDB
  - Lazy loading for huge files (8GB+)
  - Excel-like filtering and sorting
  - Multiple export formats
  - Theme system with customization
  - Multi-language support
EOF

# Copy documentation
cp README.md ${DEB_DIR}/usr/share/doc/parvu/
if [ -f "CHANGELOG.md" ]; then
    cp CHANGELOG.md ${DEB_DIR}/usr/share/doc/parvu/
fi

# Build .deb package
dpkg-deb --build ${DEB_DIR}
mv ${DEB_DIR}.deb ParVu-${VERSION}-amd64.deb

# Cleanup
rm -rf ${DEB_DIR}

echo -e "${GREEN}==================================="
echo -e "Build completed successfully!"
echo -e "===================================${NC}"
echo ""
echo "Outputs:"
echo "  1. Standalone binary: dist/parvu/"
echo "  2. Debian package: ParVu-${VERSION}-amd64.deb"
echo ""
echo "To install the .deb package:"
echo "  sudo dpkg -i ParVu-${VERSION}-amd64.deb"
echo ""
echo "To run the standalone binary:"
echo "  cd dist/parvu && ./parvu"
