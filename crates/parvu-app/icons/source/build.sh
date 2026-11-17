#!/bin/bash
# Regenerate ParVu's icon files from the SVGs in this folder (needs ImageMagick).
#   parvu.svg     the master (512 x 512), used from 32 px up
#   parvu-24.svg  hand-tuned for 24 px (and scaled to 20 px)
#   parvu-16.svg  hand-tuned for 16 px
# Writes the Tauri set in ../, ui/public/logo.png and docs/images/parvu-logo.png.
set -euo pipefail
cd "$(dirname "$0")"
out=..
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

master() { # size file
  convert -background none -density 384 parvu.svg -resize "$1x$1" "PNG32:$2"
}
master 32 "$tmp/32.png"; master 40 "$tmp/40.png"; master 48 "$tmp/48.png"; master 64 "$tmp/64.png"
master 128 "$tmp/128.png"; master 256 "$tmp/256.png"; master 512 "$tmp/512.png"
# The small drawings are whole pixels: rendered exactly (see raster.py).
python3 raster.py parvu-16.svg 16 "$tmp/16.png"
python3 raster.py parvu-24.svg 24 "$tmp/24.png"
python3 raster.py parvu-24.svg 20 "$tmp/20.png"

cp "$tmp/32.png" "$out/32x32.png"
cp "$tmp/64.png" "$out/64x64.png"
cp "$tmp/128.png" "$out/128x128.png"
cp "$tmp/256.png" "$out/128x128@2x.png"
cp "$tmp/512.png" "$out/icon.png"
# Windows: 20 and 40 px are what 125 % and 250 % scaling ask for.
# Each size stored as PNG (Windows Vista and later), as Tauri's own icons are.
python3 - "$tmp" "$out/icon.ico" <<'PY'
import struct, sys
tmp, out = sys.argv[1], sys.argv[2]
sizes = [16, 20, 24, 32, 40, 48, 64, 256]
data = [open(f"{tmp}/{n}.png", "rb").read() for n in sizes]
head = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries = b""
for n, d in zip(sizes, data):
    entries += struct.pack("<BBBBHHII", n % 256, n % 256, 0, 0, 1, 32, len(d), offset)
    offset += len(d)
open(out, "wb").write(head + entries + b"".join(data))
PY
cp "$tmp/128.png" ../../../../ui/public/logo.png
cp "$tmp/512.png" ../../../../docs/images/parvu-logo.png
echo "icons written"
