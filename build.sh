#!/bin/bash
# Build ParVu and its installer.
#
#   ./build.sh          package "parvu" (.deb on Linux, setup.exe on Windows)
#   ./build.sh --beta   package "parvu-next", which installs next to "parvu"
#
# Linux build dependencies (Debian/Ubuntu):
#   sudo apt install build-essential libwebkit2gtk-4.1-dev libsoup-3.0-dev \
#        libjavascriptcoregtk-4.1-dev libgtk-3-dev librsvg2-dev
# plus Rust (rustup) and Node.js 20+.
#
# The installer links DuckDB statically, compiled from C++ on the first build.
# Each compiler process needs up to 3 GB of RAM, so the job count is derived
# from the memory available right now (override with CARGO_BUILD_JOBS).
set -euo pipefail
cd "$(dirname "$0")"

if [[ -z "${CARGO_BUILD_JOBS:-}" ]]; then
  avail_mb=$(awk '/MemAvailable/ {print int($2 / 1024)}' /proc/meminfo 2>/dev/null || echo 0)
  if [[ "$avail_mb" -gt 0 ]]; then
    jobs=$(( (avail_mb - 1500) / 3000 ))
  else
    jobs=2
  fi
  (( jobs < 1 )) && jobs=1
  (( jobs > 4 )) && jobs=4
  export CARGO_BUILD_JOBS=$jobs
fi
echo "==> Using $CARGO_BUILD_JOBS parallel build jobs"

BETA=0
[[ "${1:-}" == "--beta" ]] && BETA=1

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

ARGS=(build --features bundled-duckdb)
[[ -n "$BUNDLES" ]] && ARGS+=(--bundles "$BUNDLES")
[[ $BETA -eq 1 ]] && ARGS+=(--config tauri.next.conf.json)

echo "==> Building the app (${BUNDLES:-no installer})"
(cd crates/parvu-app && npx --prefix ../../ui tauri "${ARGS[@]}")

if [[ "$BUNDLES" == "deb" ]]; then
  echo "==> Writing pre-release versions the Debian way (1.0.0~alpha.1)"
  crates/parvu-app/packaging/fix-deb-version.sh target/release/bundle/deb/*.deb
fi

echo
echo "Done. Installers are in target/release/bundle/"
ls -1 target/release/bundle/*/* 2>/dev/null || true
