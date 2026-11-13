# ParVu Releases

## Downloads

| Platform | File | Size |
|---|---|---|
| Linux (Ubuntu/Debian, 64-bit) | `parvu_X.Y.Z_amd64.deb` | about 30 MB |
| Windows 10/11 (64-bit) | `parvu_X.Y.Z_x64-setup.exe` | about 15 MB |

## Linux

```bash
sudo apt install ./parvu_X.Y.Z_amd64.deb
parvu
```

`apt` installs the two system libraries ParVu needs (WebKitGTK and GTK) if they are missing. `sudo dpkg -i` also works when they are already installed.

The package replaces any earlier ParVu `.deb`, including the Python versions (0.x). Settings, recent files and themes in `~/.ParVu` are kept.

Uninstall with `sudo apt remove parvu`.

### Beta builds

Beta packages are named `parvu-next` and install next to `parvu`, so you can try a new version without removing the current one. Start the beta with `parvu-next`. Both share `~/.ParVu`.

## Windows

Run `parvu_X.Y.Z_x64-setup.exe` and follow the wizard. It adds a Start menu entry and registers ParVu for Parquet, CSV, JSON and Excel files. Uninstall from **Settings → Apps**.

ParVu uses Microsoft Edge WebView2, which is part of Windows 10 and 11. If it is missing, the installer downloads it.

## Troubleshooting

**Linux: the window is blank or the app does not start.** Run `parvu` from a terminal to see errors. On some graphics drivers this helps:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 parvu
```

**Windows: "Windows protected your PC".** The installer is not code-signed yet. Click **More info → Run anyway**.

**Logs** are in `~/.ParVu/logs/` on both platforms (`%USERPROFILE%\.ParVu\logs` on Windows). See [docs/CRASH_REPORTING.md](docs/CRASH_REPORTING.md).

## Verifying downloads

```bash
sha256sum parvu_*.deb
```

```powershell
Get-FileHash parvu_*.exe -Algorithm SHA256
```

Compare with the checksum in the release notes.

## Building a release

Releases are built and published by GitHub Actions (`.github/workflows/release.yml`):

1. Set the new version in `Cargo.toml`, `crates/parvu-app/tauri.conf.json` and `ui/package.json`, and add a `## [<version>]` section to `CHANGELOG.md`.
2. Merge that to `main`, then tag the commit and push the tag:
   ```bash
   git tag -a v1.0.0 -m "ParVu 1.0.0"
   git push origin v1.0.0
   ```
3. The workflow checks that the tag matches the version, runs the tests, builds the deb and the Windows installer, and publishes a GitHub release with both files, `SHA256SUMS.txt`, and the changelog section as notes. Versions with a `-` (such as `1.0.0-alpha.1`) are marked as pre-releases. A tag that doesn't match the version fails before anything is built.

Re-running a failed release workflow replaces the files of the existing release. To build installers locally, see [docs/BUILDING.md](docs/BUILDING.md).

## Support

- [README.md](README.md)
- [Issues](https://github.com/AzizNadirov/ParVu/issues)
