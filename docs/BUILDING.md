# Building ParVu

## Requirements

- [Rust](https://rustup.rs/) (stable)
- Node.js 20 or newer
- **Linux (Debian/Ubuntu):**
  ```bash
  sudo apt install build-essential libwebkit2gtk-4.1-dev libsoup-3.0-dev \
       libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev
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
| Windows | `target/release/bundle/nsis/parvu_<version>_x64-setup.exe` |

Windows builds only the NSIS installer, because MSI does not accept pre-release versions such as `1.0.0-alpha.1`.

## Development

Run the app with live UI reloading:

```bash
npm --prefix ui ci
cd crates/parvu-app
npx --prefix ../../ui tauri dev
```

Tests and lint:

```bash
cargo test -p parvu-core -p parvu-dsl -p parvu-config
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix ui run typecheck
npm --prefix ui test
```

## DuckDB

DuckDB is compiled from C++ source and linked statically, so the app is a single binary. The first build takes several minutes. Each compiler process can use 2–3 GB of RAM, so `.cargo/config.toml` limits cargo to 4 parallel jobs.

For quick checks you can link the official prebuilt DuckDB library instead, which skips that compile:

```bash
cargo test -p parvu-core --no-default-features
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
```

CI does this for tests and clippy. Installers are always built with DuckDB compiled in.

## CI

`.github/workflows/ci.yml`:

- **Every push to `main` or `rust-*` branches, and pull requests:** UI checks, clippy and Rust tests, with the prebuilt DuckDB (a few minutes).
- **`main`, `v*` tags and manual runs** (`gh workflow run CI --ref <branch>`): also builds the Linux and Windows installers, and on Linux installs the `.deb` and starts the app with an empty home directory. The installers are uploaded as run artifacts.

## Packaging files

| File | Purpose |
|---|---|
| `crates/parvu-app/tauri.conf.json` | App name, version, window security, file associations |
| `crates/parvu-app/tauri.linux.conf.json` | Linux package name, desktop entry, install scripts |
| `crates/parvu-app/tauri.next.conf.json` | Overrides for the `parvu-next` beta |
| `crates/parvu-app/packaging/` | Desktop entries, MIME type for Parquet, `postinst`/`postrm` |
| `crates/parvu-app/icons/` | App icons |

The version is set in `Cargo.toml` (`workspace.package.version`), `crates/parvu-app/tauri.conf.json` and `ui/package.json`.
