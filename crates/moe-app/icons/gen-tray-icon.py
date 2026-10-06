#!/usr/bin/env python3
"""Generate the menu bar tray template icon (22x22 PNG)。

Template images (icon_as_template) only use the alpha channel; macOS renders the color per menu bar light/dark mode.
Draw a rounded rectangle outline representing the floating panel. 4x supersampling + downsampling for anti-aliasing.
Run: python3 crates/moe-app/icons/gen-tray-icon.py
"""

import math
import pathlib
import struct
import zlib

S = 4  # supersampling factor
W = H = 22
X0, Y0, X1, Y1 = 3.0, 3.0, 19.0, 19.0  # outer frame
RADIUS = 4.5
STROKE = 1.6


def in_stroke(x: float, y: float) -> bool:
    """Whether a point lies on the rounded-rect stroke (signed distance field)."""
    cx = min(max(x, X0 + RADIUS), X1 - RADIUS)
    cy = min(max(y, Y0 + RADIUS), Y1 - RADIUS)
    d = math.hypot(x - cx, y - cy) - RADIUS
    return abs(d) <= STROKE / 2


def sample_rows() -> list[bytearray]:
    rows = []
    for y in range(H * S):
        row = bytearray()
        for x in range(W * S):
            cov = 255 if in_stroke((x + 0.5) / S, (y + 0.5) / S) else 0
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
    print(f"wrote {path} ({len(png)} bytes)")


if __name__ == "__main__":
    main()
