# ParVu Releases

## Downloads

| Platform | File | Size |
|---|---|---|
| Linux (Ubuntu/Debian, 64-bit) | `parvu_X.Y.Z_amd64.deb` | about 30 MB |
| Windows 10/11 (64-bit) | `ParVu_X.Y.Z_x64-setup.exe` | about 15 MB |

## Linux

```bash
sudo apt install ./parvu_X.Y.Z_amd64.deb
parvu
```

`apt` installs the two system libraries ParVu needs (WebKitGTK and GTK) if they are missing. `sudo dpkg -i` also works when they are already installed.

### Installer script

`install.sh` works on Debian, Ubuntu and their derivatives (Debian 12 / Ubuntu 22.04 or newer):

```bash
curl -fsSL https://raw.githubusercontent.com/AzizNadirov/ParVu/main/install.sh | bash -s -- --prebuilt
```

downloads the latest release, checks its SHA-256 and installs it. Without `--prebuilt` it builds from source instead: it installs the build packages with apt, Rust (in `~/.cargo`) and Node.js (in `~/.cache/parvu/node`) when they are missing or too old, clones the repository to `~/.cache/parvu/src`, runs `build.sh` and installs the result. Run from a checkout (`./install.sh`), it builds that checkout. Other options: `--beta` (build `parvu-next`), `--ref <branch or tag>`, `--src <dir>`, `--dry-run`. The first source build compiles DuckDB and takes 10–20 minutes.

The package replaces any earlier ParVu `.deb`, including the Python versions (0.x). Settings, recent files and themes in `~/.ParVu` are kept.

Uninstall with `sudo apt remove parvu`.

### Using ParVu from AI clients

The `parvu` command also runs an MCP server for Claude Code, Claude Desktop and VS Code (`parvu mcp --allow-dir <folder>`). See [docs/MCP.md](docs/MCP.md).

### Beta builds

Beta packages are named `parvu-next` and install next to `parvu`, so you can try a new version without removing the current one. Start the beta with `parvu-next`; it has its own menu entry. Both share `~/.ParVu`. Remove the beta with `sudo apt remove parvu-next`.

## Snap (Ubuntu App Center)

```bash
sudo snap install parvu            # stable releases
sudo snap install parvu --beta     # pre-releases (1.1.0-beta.1 and so on)
parvu
```

or search for ParVu in the App Center. The snap runs the same program as the deb, inside the snap sandbox, and updates itself.

- **AI keys in the keychain:** the snap can use the system keychain only after you allow it once:
  ```bash
  sudo snap connect parvu:password-manager-service
  ```
  Without it, ParVu reads keys from environment variables (see [docs/AI.md](docs/AI.md)).
- **Files ParVu can open:** those in your home folder, except hidden files and folders (such as `~/.local`), and, after `sudo snap connect parvu:removable-media`, those in `/media` and `/mnt`. A file you pick elsewhere in the Open dialog is passed in by the desktop's file portal.
- **Settings and data:** the snap keeps its own `~/.ParVu` in `~/snap/parvu/current/.ParVu`, so it doesn't share settings, recent files, themes or custom tools with the deb. To take them over, close the snap's ParVu and copy the folder: `cp -a ~/.ParVu/. ~/snap/parvu/current/.ParVu/`. Logs are in `~/snap/parvu/current/.ParVu/logs/`.
- **AI clients:** `parvu mcp --attach` and `parvu mcp --allow-dir <folder>` work as with the deb, through `/snap/bin/parvu` (see [docs/MCP.md](docs/MCP.md)). Allowed folders must be in your home folder, or in `/media` or `/mnt` with removable-media.
- **File types:** the snap registers ParVu for CSV, JSON and Excel files. Ubuntu doesn't know the Parquet file type by itself (the deb adds it), so "Open With" may not offer ParVu for `.parquet` files; open them from ParVu or drop them onto its window.

Remove it with `sudo snap remove parvu` (add `--purge` to delete its data in `~/snap/parvu` too). The deb and the snap can be installed together; in a terminal, `parvu` then runs the deb (`/usr/bin` comes before `/snap/bin` on the `PATH`), and `/snap/bin/parvu` the snap.

## Windows

Run `ParVu_X.Y.Z_x64-setup.exe` and follow the wizard. It adds a Start menu entry and registers ParVu for Parquet, CSV, JSON and Excel files. Uninstall from **Settings → Apps**.

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
Get-FileHash ParVu_*.exe -Algorithm SHA256
```

Compare with the checksum in the release notes or in `SHA256SUMS.txt`. In release file names, the Debian version's `~` appears as `-` (`parvu_1.0.0-alpha.2_amd64.deb`); the package itself keeps `1.0.0~alpha.2`.

## Building a release

Releases are built and published by GitHub Actions (`.github/workflows/release.yml`):

1. Set the new version in `Cargo.toml`, `crates/parvu-app/tauri.conf.json` and `ui/package.json`, and add a `## [<version>]` section to `CHANGELOG.md`.
2. Merge that to `main`, then tag the commit and push the tag:
   ```bash
   git tag -a v1.0.0 -m "ParVu 1.0.0"
   git push origin v1.0.0
   ```
3. The workflow checks that the tag matches the version, runs the tests, builds the deb and the Windows installer, and publishes a GitHub release with both files, `SHA256SUMS.txt`, and the changelog section as notes. The notes start with install commands for the release's own files. Versions with a `-` (such as `1.1.0-beta.1`) are marked as pre-releases once a stable 1.x release exists; until then each alpha or beta becomes the Latest release, so the releases page doesn't point at the 0.x Python app. A tag that doesn't match the version fails before anything is built.

4. After publishing, the workflow builds the snap from the release's deb (`.github/workflows/snap.yml`) and publishes it to the Snap Store: the `stable` channel for versions without a `-`, `beta` for pre-releases. Without the `SNAPCRAFT_STORE_CREDENTIALS` secret it only builds the snap and keeps it as the run artifact `parvu-snap`. To build or publish the snap of an existing release again: `gh workflow run Snap -f tag=v1.0.0`.

Re-running a failed release workflow replaces the files of the existing release. To build installers locally, see [docs/BUILDING.md](docs/BUILDING.md).

## Support

- [README.md](README.md)
- [Issues](https://github.com/AzizNadirov/ParVu/issues)
