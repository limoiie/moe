#!/usr/bin/env python3
"""Generate the menu bar tray template icon (22x22 PNG) — the Moe mark.

Template images (icon_as_template) only use the alpha channel; macOS renders the color per menu
bar light/dark mode, so the mark adapts to the menu bar appearance for free.

The mark is parsed DIRECTLY from ui/src/moe.svg (M/L/Q/Z path data → polygons, Q curves sampled),
so the raster can never drift from the svg: edit the svg, rerun this script, done. The artwork bbox
(over all vertices) is fitted to the slot, and 6x supersampling + downsampling gives anti-aliasing.
Run: python3 crates/moe-app/icons/gen-tray-icon.py
"""

import math
import pathlib
import re
import struct
import zlib

S = 6  # supersampling factor
W = H = 22  # the radiant-diamond mark is square once cropped
MARGIN = 1.0  # px of breathing room around the mark inside the menu-bar slot

SVG_PATH = pathlib.Path(__file__).resolve().parents[3] / "ui" / "src" / "moe.svg"
TOKEN = re.compile(r"[MLQZmlqz]|-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


def parse_path(d: str) -> list:
    """Path data → polygon vertices (Q curves sampled at four steps; enough at icon scale)."""
    tokens = TOKEN.findall(d)
    pts: list = []
    cur = start = None
    cmd = None
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t.isalpha():
            cmd = t
            i += 1
            continue
        if cmd in "Mm":
            cur = (float(tokens[i]), float(tokens[i + 1]))
            start = cur
            pts.append(cur)
            cmd = "L"  # coordinate pairs after M are implicit line segments
            i += 2
        elif cmd in "Ll":
            cur = (float(tokens[i]), float(tokens[i + 1]))
            pts.append(cur)
            i += 2
        elif cmd in "Qq":
            cx, cy = float(tokens[i]), float(tokens[i + 1])
            ex, ey = float(tokens[i + 2]), float(tokens[i + 3])
            for s in (0.25, 0.5, 0.75, 1.0):
                pts.append(
                    (
                        (1 - s) ** 2 * cur[0] + 2 * (1 - s) * s * cx + s * s * ex,
                        (1 - s) ** 2 * cur[1] + 2 * (1 - s) * s * cy + s * s * ey,
                    )
                )
            cur = (ex, ey)
            i += 4
        elif cmd in "Zz":
            pts.append(start)
            i += 1
        else:  # unreachable with well-formed path data
            i += 1
    return pts


def load_polygons() -> list:
    svg = SVG_PATH.read_text()
    return [parse_path(d) for d in re.findall(r'<path[^>]*\bd="([^"]+)"', svg)]


POLYGONS = load_polygons()
XS = [x for poly in POLYGONS for x, _ in poly]
YS = [y for poly in POLYGONS for _, y in poly]
BX, BY = min(XS), min(YS)
BW, BH = max(XS) - BX, max(YS) - BY
SCALE = min((W - 2 * MARGIN) / BW, (H - 2 * MARGIN) / BH)
OX = (W - BW * SCALE) / 2
OY = (H - BH * SCALE) / 2


def to_design(x: float, y: float) -> tuple:
    """Tray pixel → svg units (inverse of the bbox fit)."""
    return (x - OX) / SCALE + BX, (y - OY) / SCALE + BY


def inside(px: float, py: float, poly: list) -> bool:
    """Ray-casting point-in-polygon."""
    inside_flag = False
    n = len(poly)
    for i in range(n):
        x1, y1 = poly[i]
        x2, y2 = poly[(i + 1) % n]
        if (y1 > py) != (y2 > py):
            if px < x1 + (py - y1) / (y2 - y1) * (x2 - x1):
                inside_flag = not inside_flag
    return inside_flag


def in_mark(x: float, y: float) -> bool:
    return any(inside(x, y, poly) for poly in POLYGONS)


def sample_rows() -> list[bytearray]:
    rows = []
    for y in range(H * S):
        row = bytearray()
        for x in range(W * S):
            cov = 255 if in_mark(*to_design((x + 0.5) / S, (y + 0.5) / S)) else 0
            row += bytes([0, 0, 0, cov])  # template image: color is arbitrary, only alpha matters
        rows.append(row)
    return rows


def downsample(rows: list[bytearray]) -> bytes:
    out = bytearray()
    n = S * S
    for y in range(H):
        for x in range(W):
            acc = [0, 0, 0, 0]
            for dy in range(S):
                row = rows[y * S + dy]
                for dx in range(S):
                    i = x * S * 4 + dx * 4
                    for c in range(4):
                        acc[c] += row[i + c]
            out += bytes(v // n for v in acc)
    return bytes(out)


def chunk(tag: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + tag
        + data
        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    )


def main() -> None:
    if not POLYGONS:
        raise SystemExit(f"no path data found in {SVG_PATH}")
    pixels = downsample(sample_rows())
    raw = bytearray()
    for y in range(H):
        raw.append(0)  # filter: none
        raw += pixels[y * W * 4 : (y + 1) * W * 4]

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")

    path = pathlib.Path(__file__).resolve().parent / "tray.png"
    path.write_bytes(png)
    print(f"parsed {len(POLYGONS)} paths from {SVG_PATH}")
    print(f"wrote {path} ({len(png)} bytes)")


if __name__ == "__main__":
    main()