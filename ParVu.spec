# -*- mode: python ; coding: utf-8 -*-


a = Analysis(
    ['src/main.py'],
    pathex=[],
    binaries=[],
    datas=[
        ('src/schemas.py', '.'),
        ('src/query_revisor.py', '.'),
        ('src/utils.py', '.'),
        ('src/core.py', '.'),
        ('src/gui_tools.py', '.'),
        ('src/static/loading-thinking.gif', './static/'),
        ('src/static/logo.jpg', './static/'),
        ('src/static/help.md', './static/'),
        ('src/settings/settings.json', './settings/'),
        ('src/settings/default_settings.json', './settings/'),
        ('src/history/recents.json', './history/'),
    ],
    hiddenimports=[
        'pydantic', 'loguru'
    ],
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name='ParVu',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    contents_directory='src',
    version='0.0.3',
)
