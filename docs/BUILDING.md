# Building ParVu

## Requirements

- [Rust](https://rustup.rs/) 1.89 or newer (stable)
- Node.js 22.12 or newer
- **Linux (Debian/Ubuntu):**
  ```bash
  sudo apt install build-essential libwebkit2gtk-4.1-dev libsoup-3.0-dev \
       libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev
  ```
- **Windows:** Visual Studio Build Tools with the C++ workload, and Git for Windows (for `bash`). WebView2 ships with Windows 10 and 11.

## Installer

```bash
./build.sh           # package "parvu"
./build.sh --beta    # package "parvu-next", which installs next to "parvu"
```

The script installs the UI packages, runs the UI and Rust tests, and builds the installer:

| Platform | Output |
|---|---|
| Linux | `target/release/bundle/deb/parvu_<version>_amd64.deb` |
| Windows | `target/release/bundle/nsis/ParVu_<version>_x64-setup.exe` |

Windows builds only the NSIS installer, because MSI does not accept pre-release versions such as `1.0.0-alpha.1`.

On Linux the deb depends on `libayatana-appindicator3-1` for the tray icon. Tauri adds that dependency itself when it bundles, and finds the library through `pkg-config`, so building the installer needs `libayatana-appindicator3-dev` (`build.sh` checks for it before the long build). Everyday `cargo` builds and tests don't need it: ParVu loads the library at run time and runs without a tray icon when it's missing.

On Linux the pre-release version is written into the deb as `1.0.0~alpha.1`. With a tilde dpkg ranks it below `1.0.0`, so the stable release upgrades it; Tauri's own `1.0.0-alpha.1` would rank above it.

Released debs are also published in ParVu's APT repository on GitHub Pages, built by `.github/workflows/apt.yml` with `packaging/apt/build_repo.py`; its tests run with `python3 -m unittest discover -s packaging/apt`. See [RELEASES.md](../RELEASES.md#the-apt-repository).

## Development

Run the app with live UI reloading:

```bash
npm --prefix ui ci
cd crates/parvu-app
npx --prefix ../../ui tauri dev
```

Tests and lint:

```bash
cargo test -p parvu-core -p parvu-dsl -p parvu-config --features parvu-core/schema
cargo test -p parvu-app            # MCP server tests (needs the Linux packages above)
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix ui run typecheck
npm --prefix ui test
```

The `schema` feature of `parvu-core` generates JSON schemas for the table operations; `parvu-app` always enables it (the MCP server uses them as tool schemas).

### Running the MCP server from source

```bash
cargo run -p parvu-app -- mcp --allow-dir ~/data
```

`cargo run` finds the prebuilt DuckDB library by itself. To start `target/debug/parvu` directly (for example from an MCP client's config), add the folder holding `libduckdb.so` to the library path: `LD_LIBRARY_PATH=$(dirname $(find target/duckdb-download -name libduckdb.so | head -1))`. Installed packages don't need this.

## DuckDB

Everyday builds, tests, clippy and `tauri dev` link the **official prebuilt DuckDB library**, which the `duckdb` crate downloads. No C++ is compiled, and a test run builds in seconds.

Installers link DuckDB **statically**, compiled from C++ source, so the app is a single binary. `build.sh` turns this on with the `bundled-duckdb` feature. Each compiler process needs up to 3 GB of RAM, so `build.sh` sets the number of parallel jobs from the memory available when it starts, between 1 and 4. Close other heavy programs for the first installer build, or set `CARGO_BUILD_JOBS=1` on a machine with 8 GB.

Never ship a build made without `bundled-duckdb`: it depends on a DuckDB library that is not part of the package.

## CI

`.github/workflows/ci.yml`:

- **Every push to `main` or `rust-*` branches, and pull requests:** UI checks, clippy and the Rust tests (including `parvu-app`), with the prebuilt DuckDB (a few minutes). The Rust build cache also keeps the downloaded DuckDB library (`target/duckdb-download`); without it a restored cache fails to link on Windows.
- **`main` and manual runs** (`gh workflow run CI --ref <branch>`): also builds the Linux and Windows installers (`package.yml`), and on Linux installs the `.deb` and starts the app with an empty home directory. The installers are uploaded as run artifacts.
- **`v*` tags** (`release.yml`): the same tests and installers, published as a GitHub release, then the snap (`snap.yml`, below). See [RELEASES.md](../RELEASES.md#building-a-release).

## The snap

The snap (`snap/snapcraft.yaml`) is not built from source: it repackages the release's `.deb`, so it runs exactly the binary the deb installs.

- **Base and runtime:** `core24` with the `gnome` extension. The GNOME 46 runtime (`gnome-46-2404`) provides GTK 3, WebKitGTK 4.1, libsoup 3 and `libayatana-appindicator3` (the tray icon), so the snap stages no libraries; the extension also adds the layout that lets WebKit find its helper processes, `GTK_USE_PORTAL=1` (file dialogs go through the desktop portal) and the `desktop`, `desktop-legacy`, `gsettings`, `opengl`, `wayland` and `x11` plugs.
- **Other plugs:** `home`, `removable-media` (manual connect), `network` (AI providers), `network-bind` (the `127.0.0.1` listeners for the single instance and for AI clients), `password-manager-service` (the keychain, manual connect) and `unity7` (the tray icon's StatusNotifier D-Bus calls).
- **Version and grade** come from the deb: `1.0.0~alpha.1` becomes `1.0.0-alpha.1`, versions with a `-` get grade `devel`, others `stable`.
- **The desktop entry** is the deb's, with the icon given by path inside the snap. The deb's Parquet MIME type is left out: a snap can't register MIME types.
- **Data:** under strict confinement `$HOME` is `~/snap/parvu/<revision>`, so ParVu's `~/.ParVu` is there. `parvu mcp --attach`, started through `/snap/bin/parvu`, sees the same `$HOME`, so it finds the window's `run/mcp.json`. `~` in `parvu mcp` paths means the real home (`$SNAP_REAL_HOME`).

`.github/workflows/snap.yml` runs after each release (from `release.yml`) or by hand (`gh workflow run Snap -f tag=v1.0.0`). It downloads the release's deb, checks it against `SHA256SUMS.txt`, saves it as `snap/local/parvu.deb`, builds with `snapcore/action-build` (snapcraft in LXD), checks that the snap's version matches the tag, uploads the `.snap` as the artifact `parvu-snap` and, when the `SNAPCRAFT_STORE_CREDENTIALS` secret is set, publishes it to `stable` (no `-` in the tag) or `beta`.

To build it locally (snapcraft needs LXD or Multipass):

```bash
cp parvu_1.0.0_amd64.deb snap/local/parvu.deb
snapcraft pack
sudo snap install --dangerous ./parvu_1.0.0_amd64.snap
```

## Packaging files

| File | Purpose |
|---|---|
| `crates/parvu-app/tauri.conf.json` | App name, version, window security, file associations |
| `crates/parvu-app/tauri.linux.conf.json` | Linux package name, desktop entry, install scripts |
| `crates/parvu-app/tauri.next.conf.json` | Overrides for the `parvu-next` beta |
| `crates/parvu-app/packaging/` | Desktop entries, MIME type for Parquet, `postinst`/`postrm` |
| `crates/parvu-app/icons/` | App icons |
| `snap/snapcraft.yaml` | The snap, made from the release's deb |

The version is set in `Cargo.toml` (`workspace.package.version`), `crates/parvu-app/tauri.conf.json` and `ui/package.json`.
