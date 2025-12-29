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
    pathex=['src'],  # Add src to path for imports
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
