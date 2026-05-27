# Building ParVu on Windows

This page documents the short, day-to-day path. The script ([build.ps1](../build.ps1)) does all the work.

For platform-agnostic detail, see [BUILDING.md](BUILDING.md).

---

## TL;DR

```powershell
# From the repo root, in PowerShell:
.\build.ps1
```

Produces three artifacts next to the repo root:

| File | What it is |
|---|---|
| `dist\parvu\` | Standalone folder bundle (run `parvu.exe` directly) |
| `ParVu-<version>-portable.zip` | Portable ZIP — extract anywhere, run |
| `ParVu-<version>-setup.exe` | Inno Setup installer (desktop shortcut + file associations checked by default, Start Menu, uninstaller) |

The `<version>` token is read from `pyproject.toml`.

---

## Prerequisites

1. **PowerShell 5.1+** (preinstalled on Windows 10/11).
2. **uv** (Python package manager) — provides Python 3.13 automatically.
   ```powershell
   powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
   ```
3. **Inno Setup 6** — only required for the installer. The script will skip
   the installer step (with a warning) if Inno Setup is missing.
   - Direct download (6.7.3): https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe
   - Releases page: https://jrsoftware.org/isdl.php
   - Default install path expected: `C:\Program Files (x86)\Inno Setup 6\ISCC.exe`

If you only want the portable ZIP, you can skip Inno Setup entirely.

---

## Usage

### Full build (portable + installer)

```powershell
.\build.ps1
```

### Portable only (no Inno Setup)

```powershell
.\build.ps1 -SkipInstaller
```

### Installer only

```powershell
.\build.ps1 -SkipPortable
```

### Incremental rebuild (keep previous `build\`)

```powershell
.\build.ps1 -NoClean
```

### Help

```powershell
.\build.ps1 -Help
```

---

## What the script does

1. Reads the version from `pyproject.toml`.
2. Cleans `build\`, `dist\`, and stale `ParVu-*-{portable,setup}.*` files (unless `-NoClean`).
3. Runs `uv sync --extra build` to install PyInstaller + Pillow.
4. Uses the on-disk [parvu.spec](../parvu.spec); writes a fresh one only if missing.
5. Invokes `pyinstaller parvu.spec --clean --noconfirm`.
6. Zips `dist\parvu\` into `ParVu-<version>-portable.zip`.
7. Generates a temporary `.iss` script and compiles it with `ISCC.exe` to produce
   `ParVu-<version>-setup.exe`.

The Inno Setup script registers:

- `.parquet` and `.pq` as primary ParVu file types (optional install task).
- `.csv` and `.json` under **Open With → ParVu** (does not override the user's default app).
- Optional desktop shortcut and Start Menu entry.

---

## Verifying the build

```powershell
# Run the bundled exe directly
.\dist\parvu\parvu.exe

# Or open a file with it
.\dist\parvu\parvu.exe "C:\path\to\file.parquet"
```

If the app fails to start, check:

- `%USERPROFILE%\.ParVu\logs\parvu_*.log`
- `build\parvu\warn-parvu.txt` (PyInstaller import warnings)

---

## Customising the build

Edit [parvu.spec](../parvu.spec) directly — `build.ps1` does not overwrite an
existing spec. Common tweaks:

- **Icon**: change the `icon='assets/parvu.ico'` line in `EXE(...)`.
- **Hidden imports**: extend the `hiddenimports = [...]` list if a module is
  missing at runtime (look for `ModuleNotFoundError` in the logs).
- **Bundled data**: add tuples to the `datas` list as
  `('src/relative/path', 'dest/inside/bundle')`.
- **Console output for debugging**: set `console=True` in `EXE(...)` to keep
  a console window open for `print`/`traceback` visibility.

Reset to defaults by deleting `parvu.spec` — `build.ps1` will regenerate it on
the next run.

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `uv: command not found` | Install uv (see Prerequisites), then reopen PowerShell. |
| `Inno Setup 6 not found at ...` | Install Inno Setup, or use `-SkipInstaller`. |
| `ModuleNotFoundError` at runtime | Add the missing module to `hiddenimports` in `parvu.spec` and rebuild. |
| `ImportError: DLL load failed` | Install the Microsoft Visual C++ Redistributable: https://aka.ms/vs/17/release/vc_redist.x64.exe |
| Resources missing (logo, default settings) | Confirm `src/parvu/resources/{static,settings,history}` exist and the `datas` entries in `parvu.spec` point to them. |
| Build succeeds but exe is huge | Expected (~280–320 MB uncompressed). PyInstaller bundles the full Python runtime and all deps. |

---

## Releasing

```powershell
# 1. Bump version in pyproject.toml, commit
# 2. Build artifacts
.\build.ps1

# 3. Tag and push (CI will also build on tag if configured)
git tag -a v<version> -m "Release <version>"
git push origin v<version>
```

Upload `ParVu-<version>-portable.zip` and
`ParVu-<version>-setup.exe` to the GitHub release.
