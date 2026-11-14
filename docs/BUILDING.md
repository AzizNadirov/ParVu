# Building ParVu

## Requirements

- [Rust](https://rustup.rs/) 1.88 or newer (stable)
- Node.js 22.12 or newer
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
| Windows | `target/release/bundle/nsis/ParVu_<version>_x64-setup.exe` |

Windows builds only the NSIS installer, because MSI does not accept pre-release versions such as `1.0.0-alpha.1`.

On Linux the pre-release version is written into the deb as `1.0.0~alpha.1`. With a tilde dpkg ranks it below `1.0.0`, so the stable release upgrades it; Tauri's own `1.0.0-alpha.1` would rank above it.

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
- **`v*` tags** (`release.yml`): the same tests and installers, published as a GitHub release. See [RELEASES.md](../RELEASES.md#building-a-release).

## Packaging files

| File | Purpose |
|---|---|
| `crates/parvu-app/tauri.conf.json` | App name, version, window security, file associations |
| `crates/parvu-app/tauri.linux.conf.json` | Linux package name, desktop entry, install scripts |
| `crates/parvu-app/tauri.next.conf.json` | Overrides for the `parvu-next` beta |
| `crates/parvu-app/packaging/` | Desktop entries, MIME type for Parquet, `postinst`/`postrm` |
| `crates/parvu-app/icons/` | App icons |

The version is set in `Cargo.toml` (`workspace.package.version`), `crates/parvu-app/tauri.conf.json` and `ui/package.json`.
