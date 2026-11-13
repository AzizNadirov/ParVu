#!/bin/bash
# Make pre-release .deb versions sort before the release.
#
# Tauri writes the app version as is, so 1.0.0-alpha.1 becomes the Debian
# version "1.0.0-alpha.1": upstream 1.0.0, revision alpha.1, which dpkg ranks
# above 1.0.0, so the stable package would not upgrade the alpha. Debian
# spells pre-releases with a tilde (1.0.0~alpha.1 < 1.0.0), so this script
# rewrites the version in each given package and renames the file to match.
# Release versions without a "-" are left alone.
#
#   fix-deb-version.sh target/release/bundle/deb/*.deb
set -euo pipefail

for deb in "$@"; do
  version=$(dpkg-deb -f "$deb" Version)
  [[ "$version" == *-* ]] || continue
  fixed=${version/-/\~}
  work=$(mktemp -d)
  dpkg-deb -R "$deb" "$work/pkg"
  sed -i "s/^Version: .*/Version: $fixed/" "$work/pkg/DEBIAN/control"
  out="$(dirname "$deb")/$(basename "$deb" | sed "s/_${version}_/_${fixed}_/")"
  dpkg-deb --root-owner-group -Zxz -b "$work/pkg" "$out" >/dev/null
  rm -rf "$work"
  [[ "$out" != "$deb" ]] && rm -f "$deb"
  echo "    $(basename "$out") (version $fixed)"
done
