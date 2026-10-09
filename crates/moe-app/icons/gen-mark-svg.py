#!/usr/bin/env python3
"""Generate the flat brand mark (ui/src/moe.svg) from the refined app-icon master.

`icons/app-icon.png` is the refined render and the geometry's source of truth: this script decodes
it, segments the warm mark pixels (the core circle + the twelve ring satellites), measures each
satellite's ring angle, distance and radial/tangential half-diagonals, snaps the ring to its
30-degree slots, and emits the single-ink svg. The svg carries only M/L/Q/Z path data (the subset
`gen-tray-icon.py` parses), uses `currentColor`, ships transparent, and crops the viewBox to the
artwork bbox. The measurement is an interpretation, not a pixel trace: sizes are the measured ink
extremes (compensated for the render's soft corner fillets) and the ring is snapped clean.
Re-render the master → rerun this script → rerun `gen-tray-icon.py`; the mark cannot drift.
Run: python3 crates/moe-app/icons/gen-mark-svg.py
"""

import math
import pathlib
import struct
import zlib

MASTER = pathlib.Path(__file__).resolve().parent / "app-icon.png"
SVG = pathlib.Path(__file__).resolve().parents[3] / "ui" / "src" / "moe.svg"

STRIDE = 2  # segmentation subsampling: the mark is a large blob, 2x is plenty
MIN_BLOB = 400  # master px² — anything smaller is render noise
R_MIN, RB_MIN, RG_MIN = 150, 90, 25  # warm test; the card, its shadow and the margin are neutral
SAT_COUNT = 12  # the ring's design language: twelve satellites on 30-degree slots
CORNER_CUT = 0.30  # corner fillet: cut length as a fraction of the diamond's edge length
FILET_COMP = 1.15  # measured ink extreme -> corner distance (a fillet pulls the extreme in)
CORE_ARCS = 8  # the circle as quadratic arcs (smooth at the parser's 4-step sampling)
MARK_SPAN = 1000.0  # emitted artwork bbox width, in svg units
Q_STEPS = (0.25, 0.5, 0.75, 1.0)  # the samples gen-tray-icon.py takes from each Q curve


def decode(path: pathlib.Path):
    """Minimal PNG reader (8-bit RGB/RGBA, no interlace) — PIL is not a dependency."""
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path} is not a png")
    pos, idat = 8, bytearray()
    while pos < len(data):
        ln = struct.unpack(">I", data[pos : pos + 4])[0]
        tag = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + ln]
        pos += 12 + ln
        if tag == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or ctype not in (2, 6) or interlace != 0:
                raise SystemExit(f"unsupported png: depth={depth} type={ctype} interlace={interlace}")
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            break
    raw = zlib.decompress(bytes(idat))
    ch = 4 if ctype == 6 else 3
    stride = w * ch
    out = bytearray(w * h * ch)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        filt = raw[p]
        p += 1
        line = bytearray(raw[p : p + stride])
        p += stride
        if filt == 1:
            for i in range(ch, stride):
                line[i] = (line[i] + line[i - ch]) & 0xFF
        elif filt == 2:
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif filt == 3:
            for i in range(stride):
                a = line[i - ch] if i >= ch else 0
                line[i] = (line[i] + ((a + prev[i]) >> 1)) & 0xFF
        elif filt == 4:
            for i in range(stride):
                a = line[i - ch] if i >= ch else 0
                b = prev[i]
                c = prev[i - ch] if i >= ch else 0
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        elif filt != 0:
            raise SystemExit(f"bad filter {filt} at row {y}")
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return w, h, ch, out


def components(w: int, h: int, ch: int, buf: bytearray):
    """Flood-fill the warm mark pixels into components; coords are master px (sampled at STRIDE)."""
    gw, gh = (w - 1) // STRIDE + 1, (h - 1) // STRIDE + 1
    mask = bytearray(gw * gh)
    for gy in range(gh):
        y = gy * STRIDE
        for gx in range(gw):
            i = (y * w + gx * STRIDE) * ch
            if buf[i] >= R_MIN and buf[i] - buf[i + 2] >= RB_MIN and buf[i] - buf[i + 1] >= RG_MIN:
                mask[gy * gw + gx] = 1
    comps = []
    for gy in range(gh):
        for gx in range(gw):
            if mask[gy * gw + gx] != 1:
                continue
            stack, pts = [(gx, gy)], []
            mask[gy * gw + gx] = 2
            while stack:
                x, y = stack.pop()
                pts.append((x * STRIDE, y * STRIDE))
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if 0 <= nx < gw and 0 <= ny < gh and mask[ny * gw + nx] == 1:
                        mask[ny * gw + nx] = 2
                        stack.append((nx, ny))
            if len(pts) * STRIDE * STRIDE >= MIN_BLOB:
                comps.append(pts)
    return sorted(comps, key=len, reverse=True)


def bbox(pts):
    """(cx, cy, half_width, half_height) of a point cloud — center reads better than centroid
    when a render highlight has punched a bite out of the warm mask."""
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2, (max(xs) - min(xs)) / 2, (max(ys) - min(ys)) / 2


def fmt(v: float) -> str:
    s = f"{v:.1f}"
    return s[:-2] if s.endswith(".0") else s


def toward(p, q, length):
    dx, dy = q[0] - p[0], q[1] - p[1]
    d = math.hypot(dx, dy)
    return (p[0] + dx / d * length, p[1] + dy / d * length)


def quad(a, c, b, t):
    return ((1 - t) ** 2 * a[0] + 2 * (1 - t) * t * c[0] + t * t * b[0],
            (1 - t) ** 2 * a[1] + 2 * (1 - t) * t * c[1] + t * t * b[1])


def poly_samples(corners, cut):
    """On-curve points of the filleted polygon (the same vertices gen-tray-icon.py will parse)."""
    n = len(corners)
    a = [toward(corners[i], corners[(i - 1) % n], cut) for i in range(n)]
    b = [toward(corners[i], corners[(i + 1) % n], cut) for i in range(n)]
    pts = []
    for i in range(n):
        pts.append(a[i])
        pts += [quad(a[i], corners[i], b[i], t) for t in Q_STEPS]
    return pts


def poly_path(corners, cut: float) -> str:
    """Filleted polygon: line to the incoming cut point, quadratic through the corner."""
    n = len(corners)
    a = [toward(corners[i], corners[(i - 1) % n], cut) for i in range(n)]
    b = [toward(corners[i], corners[(i + 1) % n], cut) for i in range(n)]
    parts = [f"M{fmt(b[0][0])} {fmt(b[0][1])}"]
    for i in range(1, n):
        parts.append(f"L{fmt(a[i][0])} {fmt(a[i][1])}")
        parts.append(f"Q{fmt(corners[i][0])} {fmt(corners[i][1])} {fmt(b[i][0])} {fmt(b[i][1])}")
    parts.append(f"L{fmt(a[0][0])} {fmt(a[0][1])}")
    parts.append(f"Q{fmt(corners[0][0])} {fmt(corners[0][1])} {fmt(b[0][0])} {fmt(b[0][1])}Z")
    return "".join(parts)


def circle_samples(cx, cy, r):
    k = r / math.cos(math.pi / CORE_ARCS)
    pts = []
    for i in range(CORE_ARCS):
        a = 2 * math.pi * i / CORE_ARCS
        m = 2 * math.pi * (i + 0.5) / CORE_ARCS
        e = 2 * math.pi * (i + 1) / CORE_ARCS
        start = (cx + r * math.cos(a), cy + r * math.sin(a))
        ctl = (cx + k * math.cos(m), cy + k * math.sin(m))
        end = (cx + r * math.cos(e), cy + r * math.sin(e))
        pts.append(start)
        pts += [quad(start, ctl, end, t) for t in Q_STEPS]
    return pts


def circle_path(cx, cy, r):
    k = r / math.cos(math.pi / CORE_ARCS)
    parts = [f"M{fmt(cx + r)} {fmt(cy)}"]
    for i in range(CORE_ARCS):
        m = 2 * math.pi * (i + 0.5) / CORE_ARCS
        e = 2 * math.pi * (i + 1) / CORE_ARCS
        parts.append(
            f"Q{fmt(cx + k * math.cos(m))} {fmt(cy + k * math.sin(m))}"
            f" {fmt(cx + r * math.cos(e))} {fmt(cy + r * math.sin(e))}"
        )
    return "".join(parts) + "Z"


def main() -> None:
    w, h, ch, buf = decode(MASTER)
    comps = components(w, h, ch, buf)
    if len(comps) != SAT_COUNT + 1:
        sizes = ", ".join(str(len(c) * STRIDE * STRIDE) for c in comps)
        raise SystemExit(f"expected {SAT_COUNT + 1} mark components (core + satellites), got {len(comps)}: {sizes}")

    core, sats = comps[0], comps[1:]
    core_cx, core_cy, core_hw, core_hh = bbox(core)
    core_r = (core_hw + core_hh) / 2
    print(f"core: center=({core_cx:.1f}, {core_cy:.1f}) r={core_r:.1f} (bbox {core_hw:.1f}x{core_hh:.1f})")

    measured = []
    for comp in sats:
        bcx, bcy, _, _ = bbox(comp)
        ang = math.degrees(math.atan2(bcy - core_cy, bcx - core_cx))
        dist = math.hypot(bcx - core_cx, bcy - core_cy)
        ux, uy = math.cos(math.radians(ang)), math.sin(math.radians(ang))
        du = [(x - bcx) * ux + (y - bcy) * uy for x, y in comp]
        dt = [-(x - bcx) * uy + (y - bcy) * ux for x, y in comp]
        measured.append({"ang": ang, "dist": dist, "radial": (max(du) - min(du)) / 2,
                         "tangential": (max(dt) - min(dt)) / 2})

    offset = sum(m["ang"] - 360.0 / SAT_COUNT * round(m["ang"] / (360.0 / SAT_COUNT)) for m in measured) / SAT_COUNT
    print(f"ring: phase offset {offset:+.2f} deg, mean distance {sum(m['dist'] for m in measured) / SAT_COUNT:.1f}")
    for m in sorted(measured, key=lambda m: (m["ang"] + 90.0) % 360.0):
        print(f"  {m['ang']:8.2f} deg  d={m['dist']:6.1f}  radial={m['radial']:5.1f}  tangential={m['tangential']:5.1f}")

    # Shapes in master px: the circle core + twelve rounded, corner-outward diamonds on the ring.
    shapes = [("circle", [(core_cx, core_cy, core_r)])]
    for m in sorted(measured, key=lambda m: (m["ang"] + 90.0) % 360.0):
        slot = 360.0 / SAT_COUNT * round(m["ang"] / (360.0 / SAT_COUNT)) + offset
        ux, uy = math.cos(math.radians(slot)), math.sin(math.radians(slot))
        tx, ty = -uy, ux
        hd, ht = m["radial"] * FILET_COMP, m["tangential"] * FILET_COMP
        ccx, ccy = core_cx + m["dist"] * ux, core_cy + m["dist"] * uy
        corners = [(ccx + hd * ux, ccy + hd * uy), (ccx + ht * tx, ccy + ht * ty),
                   (ccx - hd * ux, ccy - hd * uy), (ccx - ht * tx, ccy - ht * ty)]
        edge = math.hypot(corners[1][0] - corners[0][0], corners[1][1] - corners[0][1])
        shapes.append(("diamond", [corners, CORNER_CUT * edge]))

    # Normalize: artwork bbox min -> (0, 0), bbox width -> MARK_SPAN svg units. The bbox is taken
    # over the on-curve samples (the fillet pulls the ink inside the mathematical corners).
    samples = []
    for kind, data in shapes:
        samples += circle_samples(*data[0]) if kind == "circle" else poly_samples(data[0], data[1])
    bx, by, bhw, bhy = bbox(samples)
    scale = MARK_SPAN / (2 * bhw)
    vt = lambda p: ((p[0] - bx + bhw) * scale, (p[1] - by + bhy) * scale)
    height = max(vt(p)[1] for p in samples)

    paths = []
    for kind, data in shapes:
        if kind == "circle":
            paths.append(("core", circle_path(*vt(data[0][:2]), data[0][2] * scale)))
        else:
            paths.append(("satellite", poly_path([vt(p) for p in data[0]], data[1] * scale)))

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {fmt(MARK_SPAN)} {fmt(height)}">',
        "  <!-- Moe mark: a circle core with twelve rounded-diamond satellites on a 30-degree ring —",
        "       the refined radiant mark. Generated from crates/moe-app/icons/app-icon.png by",
        "       crates/moe-app/icons/gen-mark-svg.py: the master render is the geometry's source of",
        "       truth (this svg is measured, not hand-drawn; do not hand-edit). Single ink:",
        "       currentColor = one asset, per-theme and per-scene color. Transparent; the viewBox is",
        "       cropped to the artwork bbox, so the mark fills whatever size it is given. The tray",
        "       raster (gen-tray-icon.py) parses THIS file directly — no duplicated geometry. -->",
        '  <g fill="currentColor">',
        "    <!-- Twelve satellites, 12 o'clock first, clockwise -->",
    ]
    lines += [f'    <path d="{d}"/>' for kind, d in paths if kind == "satellite"]
    lines.append("    <!-- The circle core -->")
    lines += [f'    <path d="{d}"/>' for kind, d in paths if kind == "core"]
    lines += ["  </g>", "</svg>", ""]

    SVG.write_text("\n".join(lines))
    print(f"wrote {SVG} (viewBox 0 0 {fmt(MARK_SPAN)} {fmt(height)})")


if __name__ == "__main__":
    main()