# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller spec file for ParVu - Parquet/CSV/JSON Viewer.

Builds a cross-platform application bundle. Resources are bundled at
``parvu/resources/...`` to match ``parvu.infrastructure.paths.RESOURCES_DIR``
(which resolves to ``<package_root>/resources`` at runtime).
"""

import sys
from PyInstaller.utils.hooks import collect_data_files, collect_submodules

block_cipher = None

datas = []
datas += collect_data_files('duckdb')
datas += collect_data_files('pyarrow')

datas += [('src/parvu/resources/settings', 'parvu/resources/settings')]
datas += [('src/parvu/resources/static',   'parvu/resources/static')]
datas += [('src/parvu/resources/history',  'parvu/resources/history')]

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
    'sqlglot',
    'lark',
]

hiddenimports += collect_submodules('duckdb')
hiddenimports += collect_submodules('pyarrow')
hiddenimports += collect_submodules('pandas')
hiddenimports += collect_submodules('sqlglot')
hiddenimports += collect_submodules('lark')
hiddenimports += collect_submodules('parvu')

a = Analysis(
    ['src/parvu/__main__.py'],
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
        'IPython',
        'jupyter',
        'notebook',
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
    icon='assets/parvu.png' if sys.platform.startswith('linux') else 'assets/parvu.ico',
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
