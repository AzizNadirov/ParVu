#!/bin/bash
# Build the Rust/Tauri version of ParVu and its installer.
#
#   ./build-rust.sh          beta package "parvu-next" (installs next to the Python app)
#   ./build-rust.sh --final  package "parvu" that replaces the Python app
#
# Linux build dependencies (Debian/Ubuntu):
#   sudo apt install build-essential libwebkit2gtk-4.1-dev libsoup-3.0-dev \
#        libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev
# plus Rust (rustup) and Node.js 20+.
#
# DuckDB is compiled from source on the first build. .cargo/config.toml caps
# cargo at 4 parallel jobs so this fits in 16 GB of RAM.
set -euo pipefail
cd "$(dirname "$0")"

FINAL=0
[[ "${1:-}" == "--final" ]] && FINAL=1

echo "==> Installing UI dependencies"
npm --prefix ui ci

echo "==> Checking and testing the UI"
npm --prefix ui run typecheck
npm --prefix ui test

echo "==> Testing the Rust crates"
cargo test -p parvu-core -p parvu-dsl -p parvu-config

case "$(uname -s)" in
  Linux*) BUNDLES="deb" ;;
  MINGW*|MSYS*|CYGWIN*) BUNDLES="nsis" ;;
  *) BUNDLES="" ;;
esac

ARGS=(build)
[[ -n "$BUNDLES" ]] && ARGS+=(--bundles "$BUNDLES")
[[ $FINAL -eq 0 ]] && ARGS+=(--config tauri.next.conf.json)

echo "==> Building the app (${BUNDLES:-no installer})"
(cd crates/parvu-app && npx --prefix ../../ui tauri "${ARGS[@]}")

echo
echo "Done. Installers are in target/release/bundle/"
ls -1 target/release/bundle/*/* 2>/dev/null || true
