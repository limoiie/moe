#!/usr/bin/env python3
"""Generate the app icon source (1024x1024 PNG): dark rounded panel + list rows.

For `cargo tauri icon` (generates .icns and all sizes); 4x supersampling + downsampling for anti-aliasing.
The drawing follows the tray icon's visual language (floating panel outline), adapted to a filled app-icon version.
Run: python3 crates/moe-app/icons/gen-app-icon.py
"""

import math
import pathlib
import struct
import zlib

S = 4  # supersampling factor
W = H = 1024

# Rounded-square base (macOS app icon convention: ~22% corner radius + a little padding)
BG_X0, BG_Y0, BG_X1, BG_Y1 = 64.0, 64.0, 960.0, 960.0
BG_RADIUS = 200.0

# Panel outline (same language as the tray icon, scaled up)
P_X0, P_Y0, P_X1, P_Y1 = 236.0, 268.0, 788.0, 756.0
P_RADIUS = 56.0
P_STROKE = 26.0

# List rows (three rows; the first is the "selected" state, brighter and bolder)
ROWS = [
    (306.0, 372.0, True),
    (444.0, 486.0, False),
    (558.0, 600.0, False),
]
ROW_X0, ROW_X1 = 342.0, 682.0


def rounded_rect_sd(x: float, y: float, x0: float, y0: float, x1: float, y1: float, r: float) -> float:
    """Signed distance field of a rounded rectangle (<0 inside)."""
    cx = min(max(x, x0 + r), x1 - r)
    cy = min(max(y, y0 + r), y1 - r)
    return math.hypot(x - cx, y - cy) - r


def background_color(x: float, y: float) -> tuple[int, int, int, int]:
    """Vertical gradient: #2a2a32 at the top to #141418 at the bottom; alpha anti-aliasing is handled by the caller."""
    t = (y - BG_Y0) / (BG_Y1 - BG_Y0)
    t = min(max(t, 0.0), 1.0)
    top = (42, 42, 52)
    bottom = (18, 18, 24)
    rgb = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
    return (rgb[0], rgb[1], rgb[2], 255)


def sample_pixel(x: float, y: float) -> tuple[int, int, int, int]:
    # Background rounded square
    d_bg = rounded_rect_sd(x, y, BG_X0, BG_Y0, BG_X1, BG_Y1, BG_RADIUS)
    if d_bg > 1.0:
        return (0, 0, 0, 0)
    r, g, b, _ = background_color(x, y)

    # Panel outline (white translucent stroke)
    d_panel = rounded_rect_sd(x, y, P_X0, P_Y0, P_X1, P_Y1, P_RADIUS)
    if abs(d_panel) <= P_STROKE / 2:
        return (245, 245, 248, 235)

    # List rows: first row bright white (selected), the rest gray
    for y0, y1, active in ROWS:
        d_row = rounded_rect_sd(x, y, ROW_X0, y0, ROW_X1, y1, (y1 - y0) / 2)
        if d_row <= 0:
            return (250, 250, 252, 240) if active else (150, 150, 160, 190)

    return (r, g, b, 255)


def render_rows() -> list[bytearray]:
    rows = []
    for y in range(H * S):
        row = bytearray()
        for x in range(W * S):
            px = sample_pixel((x + 0.5) / S, (y + 0.5) / S)
            row += bytes(px)
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
    pixels = downsample(render_rows())
    raw = bytearray()
    for y in range(H):
        raw.append(0)  # filter: none
        raw += pixels[y * W * 4 : (y + 1) * W * 4]

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")

    path = pathlib.Path(__file__).resolve().parent / "app-icon.png"
    path.write_bytes(png)
    print(f"wrote {path} ({len(png)} bytes)")


if __name__ == "__main__":
    main()
