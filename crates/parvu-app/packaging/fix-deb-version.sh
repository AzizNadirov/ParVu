#!/bin/bash
# Repack Tauri's .deb: pre-release versions that sort before the release,
# and xz compression for every package.
#
# Version: Tauri writes the app version as is, so 1.0.0-alpha.1 becomes the
# Debian version "1.0.0-alpha.1": upstream 1.0.0, revision alpha.1, which
# dpkg ranks above 1.0.0, so the stable package would not upgrade the alpha.
# Debian spells pre-releases with a tilde (1.0.0~alpha.1 < 1.0.0), so this
# script rewrites the version in each pre-release package and renames the
# file to match. Release versions without a "-" keep their version.
#
# Compression: Tauri compresses with gzip; xz makes the package about a third
# smaller (37 MB -> 25 MB), so every package is rebuilt with xz.
#
#   fix-deb-version.sh target/release/bundle/deb/*.deb
set -euo pipefail

for deb in "$@"; do
  version=$(dpkg-deb -f "$deb" Version)
  fixed=$version
  [[ "$version" == *-* ]] && fixed=${version/-/\~}
  work=$(mktemp -d)
  dpkg-deb -R "$deb" "$work/pkg"
  sed -i "s/^Version: .*/Version: $fixed/" "$work/pkg/DEBIAN/control"
  out="$(dirname "$deb")/$(basename "$deb" | sed "s/_${version}_/_${fixed}_/")"
  # Build next to the original, then replace it: the name is the same for
  # release versions.
  dpkg-deb --root-owner-group -Zxz -b "$work/pkg" "$work/repacked.deb" >/dev/null
  rm -f "$deb"
  mv "$work/repacked.deb" "$out"
  rm -rf "$work"
  echo "    $(basename "$out") (version $fixed, xz)"
done
