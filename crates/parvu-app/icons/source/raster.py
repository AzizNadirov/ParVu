"""Exact rasteriser for ParVu's small icons (rounded rectangles only).

ImageMagick's built-in SVG renderer draws each rectangle one pixel too
tall, which fills the 1 px gaps of the 16 px drawing. These drawings are
made of whole pixels, so they are rendered here with 8 x 8 supersampling.

    python3 raster.py parvu-16.svg 16 out.png
"""
import re
import struct
import sys
import zlib


def rects(svg):
    out = []
    for m in re.finditer(r"<rect\b([^>]*)/>", svg):
        a = dict(re.findall(r'(\w+)="([^"]*)"', m.group(1)))
        out.append((float(a.get("x", 0)), float(a.get("y", 0)), float(a["width"]), float(a["height"]),
                    float(a.get("rx", 0)), a.get("fill")))
    return out


def inside(px, py, x, y, w, h, r):
    if not (x <= px < x + w and y <= py < y + h):
        return False
    cx = min(max(px, x + r), x + w - r)
    cy = min(max(py, y + r), y + h - r)
    return (px - cx) ** 2 + (py - cy) ** 2 <= r * r


def render(svg, size):
    view = float(re.search(r'viewBox="0 0 ([\d.]+)', svg).group(1))
    scale = view / size
    shapes = []
    group_fill = None
    for token in re.finditer(r'<g fill="([^"]+)"|</g>|<rect\b[^>]*/>', svg):
        t = token.group(0)
        if t.startswith("<g"):
            group_fill = token.group(1)
        elif t == "</g>":
            group_fill = None
        else:
            x, y, w, h, r, fill = rects(t)[0]
            shapes.append((x, y, w, h, r, fill or group_fill))
    n = 8
    rows = []
    for j in range(size):
        row = bytearray([0])
        for i in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sj in range(n):
                for si in range(n):
                    px = (i + (si + 0.5) / n) * scale
                    py = (j + (sj + 0.5) / n) * scale
                    colour = None
                    for x, y, w, h, r, fill in shapes:
                        if inside(px, py, x, y, w, h, r):
                            colour = fill
                    if colour:
                        c = int(colour[1:], 16)
                        acc[0] += c >> 16
                        acc[1] += (c >> 8) & 255
                        acc[2] += c & 255
                        acc[3] += 1
            k = acc[3]
            if k:
                row += bytes([round(acc[0] / k), round(acc[1] / k), round(acc[2] / k), round(255 * k / (n * n))])
            else:
                row += bytes(4)
        rows.append(bytes(row))
    return rows


def write_png(path, size, rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    raw = zlib.compress(b"".join(rows), 9)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)) + chunk(b"IDAT", raw) + chunk(b"IEND", b"")
    open(path, "wb").write(png)


if __name__ == "__main__":
    src, size, out = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    write_png(out, size, render(open(src).read(), size))
