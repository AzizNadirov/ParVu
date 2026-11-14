#!/bin/bash
# Install ParVu on Debian, Ubuntu and their derivatives.
#
# From a checkout:
#   ./install.sh                  build "parvu" from this checkout and install it
#   ./install.sh --beta           build "parvu-next", which installs next to "parvu"
#   ./install.sh --prebuilt       download the latest released .deb instead of building
#
# Without a checkout:
#   curl -fsSL https://raw.githubusercontent.com/AzizNadirov/ParVu/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/AzizNadirov/ParVu/main/install.sh | bash -s -- --beta
#
# Other options:
#   --ref <branch|tag>   build this branch or tag (default: main, or the checkout as it is)
#   --src <dir>          where to clone the source (default: ~/.cache/parvu/src)
#   --dry-run            print what would be installed and built, change nothing
#
# Building from source installs the build packages with apt, installs Rust
# (rustup, in ~/.cargo) and Node.js (in ~/.cache/parvu/node) only when they are
# missing or too old, and needs about 3 GB of free memory and 10-20 minutes for
# the first build, which compiles DuckDB.
set -euo pipefail

REPO_URL="https://github.com/AzizNadirov/ParVu.git"
REPO_API="https://api.github.com/repos/AzizNadirov/ParVu"
MIN_RUST="1.88"
MIN_NODE="22.12"
NODE_SERIES="22"
CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/parvu"

BETA=0
PREBUILT=0
DRY_RUN=0
REF=""
SRC=""

usage() { sed -n '2,23p' "${BASH_SOURCE[0]:-/dev/null}" 2>/dev/null | sed 's/^# \{0,1\}//'; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --beta) BETA=1 ;;
    --prebuilt) PREBUILT=1 ;;
    --dry-run) DRY_RUN=1 ;;
    --ref) REF="${2:?--ref needs a branch or tag}"; shift ;;
    --src) SRC="${2:?--src needs a directory}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
# Runs a command, or only prints it with --dry-run.
run() {
  if [[ $DRY_RUN -eq 1 ]]; then printf '    [dry run] %s\n' "$*"; else "$@"; fi
}
# Whether version $1 is at least $2 (both dotted numbers).
version_ge() { [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -1)" == "$2" ]]; }

SUDO=""
if [[ $EUID -ne 0 ]]; then
  command -v sudo >/dev/null || die "run as root or install sudo"
  SUDO="sudo"
fi

command -v apt-get >/dev/null && command -v dpkg >/dev/null \
  || die "this installer is for Debian, Ubuntu and their derivatives (apt and dpkg are needed)"

DEB_ARCH=$(dpkg --print-architecture)

apt_install() {
  local missing=()
  for pkg in "$@"; do
    dpkg-query -W -f='${Status}' "$pkg" 2>/dev/null | grep -q "install ok installed" || missing+=("$pkg")
  done
  if [[ ${#missing[@]} -eq 0 ]]; then
    echo "    already installed: $*"
    return
  fi
  say "Installing with apt: ${missing[*]}"
  run $SUDO apt-get update
  run $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y "${missing[@]}"
}

# Installs a .deb file with apt, which also pulls in its runtime libraries.
install_deb() {
  local deb="$1"
  say "Installing $(basename "$deb")"
  # apt needs a path with a slash to treat the argument as a file.
  [[ "$deb" == */* ]] || deb="./$deb"
  run $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y --allow-downgrades "$deb"
}

# ---------------------------------------------------------------------------
# --prebuilt: download the latest release (pre-releases included)
# ---------------------------------------------------------------------------
if [[ $PREBUILT -eq 1 ]]; then
  [[ $BETA -eq 0 ]] || die "--beta builds from source; releases contain only the \"parvu\" package"
  apt_install curl ca-certificates python3
  say "Looking up the latest release"
  releases=$(curl -fsSL "$REPO_API/releases?per_page=20") || die "could not reach GitHub"
  # The newest release that has a deb for this architecture.
  pick_release='
import json, sys
arch = sys.argv[1]
for rel in json.load(sys.stdin):
    if rel.get("draft"):
        continue
    assets = {a["name"]: a["browser_download_url"] for a in rel.get("assets", [])}
    debs = [n for n in assets if n.startswith("parvu_") and n.endswith("_" + arch + ".deb")]
    if debs:
        print(rel["tag_name"], assets[debs[0]], assets.get("SHA256SUMS.txt", "-"))
        break
'
  read -r tag deb_url sums_url < <(python3 -c "$pick_release" "$DEB_ARCH" <<<"$releases") || true
  [[ -n "${deb_url:-}" ]] || die "no released .deb for $DEB_ARCH yet; run without --prebuilt to build from source"
  say "Downloading ParVu $tag"
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  deb="$work/$(basename "$deb_url")"
  curl -fL --progress-bar -o "$deb" "$deb_url"
  if [[ "$sums_url" != "-" ]]; then
    curl -fsSL -o "$work/SHA256SUMS.txt" "$sums_url"
    (cd "$work" && grep " $(basename "$deb")\$" SHA256SUMS.txt | sha256sum -c --quiet -) \
      || die "checksum mismatch for $(basename "$deb")"
    echo "    checksum OK"
  fi
  install_deb "$deb"
  say "Done. Start it with: parvu"
  exit 0
fi

# ---------------------------------------------------------------------------
# Build from source
# ---------------------------------------------------------------------------
say "Build packages"
apt_install build-essential pkg-config curl git ca-certificates file xz-utils \
  libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
  libgtk-3-dev librsvg2-dev

say "Rust (at least $MIN_RUST)"
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
rust_version=$(rustc --version 2>/dev/null | awk '{print $2}' || true)
if [[ -n "$rust_version" ]] && version_ge "$rust_version" "$MIN_RUST"; then
  echo "    rustc $rust_version"
elif command -v rustup >/dev/null; then
  say "Updating Rust with rustup (found ${rust_version:-none})"
  run rustup toolchain install stable --profile minimal
  run rustup default stable
else
  say "Installing Rust with rustup into ~/.cargo (found ${rust_version:-none})"
  if [[ $DRY_RUN -eq 0 ]]; then
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path
    source "$HOME/.cargo/env"
  else
    run "curl https://sh.rustup.rs | sh -s -- -y --profile minimal"
  fi
fi

say "Node.js (at least $MIN_NODE)"
node_version=$(node --version 2>/dev/null | sed 's/^v//' || true)
if [[ -n "$node_version" ]] && version_ge "$node_version" "$MIN_NODE"; then
  echo "    node $node_version"
else
  case "$(uname -m)" in
    x86_64) node_arch=x64 ;;
    aarch64) node_arch=arm64 ;;
    *) die "no Node.js download for $(uname -m); install Node.js $MIN_NODE or newer and run again" ;;
  esac
  node_dir="$CACHE_DIR/node"
  if [[ -x "$node_dir/bin/node" ]] && version_ge "$("$node_dir/bin/node" --version | sed 's/^v//')" "$MIN_NODE"; then
    echo "    using $node_dir"
  else
    say "Downloading Node.js $NODE_SERIES into $node_dir (found ${node_version:-none})"
    if [[ $DRY_RUN -eq 0 ]]; then
      base="https://nodejs.org/dist/latest-v$NODE_SERIES.x"
      sums=$(curl -fsSL "$base/SHASUMS256.txt")
      file=$(awk -v a="linux-$node_arch.tar.xz" '$2 ~ a"$" {print $2}' <<<"$sums" | head -1)
      [[ -n "$file" ]] || die "could not find the Node.js download"
      tmp=$(mktemp -d)
      curl -fL --progress-bar -o "$tmp/$file" "$base/$file"
      (cd "$tmp" && grep " $file\$" <<<"$sums" | sha256sum -c --quiet -) || die "Node.js checksum mismatch"
      rm -rf "$node_dir" && mkdir -p "$node_dir"
      tar -xJf "$tmp/$file" -C "$node_dir" --strip-components=1
      rm -rf "$tmp"
    else
      run "download Node.js $NODE_SERIES to $node_dir"
    fi
  fi
  export PATH="$node_dir/bin:$PATH"
fi

say "Source"
script_dir=""
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
fi
if [[ -z "$SRC" && -z "$REF" && -n "$script_dir" && -f "$script_dir/build.sh" && -d "$script_dir/crates/parvu-app" ]]; then
  # Run from a checkout: build it as it is.
  SRC="$script_dir"
  echo "    using the checkout in $SRC"
else
  SRC="${SRC:-$CACHE_DIR/src}"
  REF="${REF:-main}"
  if [[ -d "$SRC/.git" ]]; then
    echo "    updating $SRC to $REF"
    run git -C "$SRC" fetch --tags --force origin
    run git -C "$SRC" checkout --force --detach "origin/$REF" 2>/dev/null \
      || run git -C "$SRC" checkout --force --detach "$REF"
  else
    echo "    cloning $REF into $SRC"
    run mkdir -p "$(dirname "$SRC")"
    run git clone --branch "$REF" "$REPO_URL" "$SRC"
  fi
fi

say "Building (the first build compiles DuckDB and takes 10-20 minutes)"
build_args=()
[[ $BETA -eq 1 ]] && build_args+=(--beta)
if [[ $DRY_RUN -eq 1 ]]; then
  run "cd $SRC && ./build.sh ${build_args[*]}"
  run "$SUDO apt-get install ./target/release/bundle/deb/<package>.deb"
  exit 0
fi
(cd "$SRC" && ./build.sh "${build_args[@]}")

package=parvu
[[ $BETA -eq 1 ]] && package=parvu-next
deb=$(ls -t "$SRC"/target/release/bundle/deb/"${package}"_*_"$DEB_ARCH".deb 2>/dev/null | head -1)
[[ -n "$deb" ]] || die "the build did not produce a $package .deb in $SRC/target/release/bundle/deb"
install_deb "$deb"
say "Done. Start it with: $package"
