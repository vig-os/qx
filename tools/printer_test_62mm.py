#!/usr/bin/env python3
"""Generate 62 mm continuous-tape test sheets for Brother QL raster/cut testing.

Brother QL DK-22205 (62 mm continuous tape, 696 dots wide at the printer's
native resolution = 11.226 px/mm ≈ 285.1 dpi). The sheets are authored in
the printer's pixel grid so every QR module lands on an integer dot — no
sub-pixel antialiasing, no rasteriser drift.

Three sheets, all using Micro QR M4 (14-char alphanumeric fits at error M)
with 4-px modules and a 2-module quiet zone (spec):

    test_4x1_horz   696 ×  84 px   62.00 ×  7.48 mm   4 horz (168×84 px)
    test_8x1_vert   696 × 168 px   62.00 × 14.97 mm   8 vert (84×168 px)
    test_8x2_vert   696 × 336 px   62.00 × 29.93 mm   8 vert × 2 rows

Per-cell math (Micro QR M4 = 17×17 modules, border = 2 modules):
    cell short side  = (17 + 2·2) · 4 px = 84 px = 7.482 mm
    cell long side   = 2 · 84 px         = 168 px = 14.964 mm

Eight 84-px cells = 672 px; the remaining 24 px sit as a 12-px margin
on each side of the tape (= 1.07 mm), which the Brother driver will
shave naturally as the unprintable left/right edge. Tape canvas
is 696 px wide either way.

SVG output has width/height in mm (physical size for the driver) and a
viewBox in px (pixel-perfect internal grid). PNG output is forced to
exactly 696 px wide so the rasterised image maps 1:1 to the printer's
dots.

IDs are deterministic-random — throwaway calibration prints, not
registry entries. Nothing is logged. Same seed → byte-identical sheets.

    uv run tools/printer_test_62mm.py
    uv run tools/printer_test_62mm.py --out-dir /tmp/printer-test
"""
from __future__ import annotations

import argparse
import random
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from label import render  # noqa: E402

ALPHABET = "23456789ABCDEFGHJKMNPQRSTUVWXYZ"
ID_LENGTH = 14
FMT = "4/4"

# Brother QL native dots for DK-22205 (62 mm continuous).
TAPE_WIDTH_PX = 696
TAPE_WIDTH_MM = 62.0
PX_PER_MM = TAPE_WIDTH_PX / TAPE_WIDTH_MM   # 11.225806…

# Pixel-perfect Micro QR M4: 4 px modules + 2-module quiet zone (spec).
# 21 total modules × 4 px = 84 px per QR cell.
QR_MODULE_PX = 4
QR_BORDER_MOD = 2

# Label cell sizes in printer pixels. "size" mirrors the short-side
# argument used elsewhere in the codebase — render(layout, size) builds
# a `size × 2·size` (vert) or `2·size × size` (horz) label.
MICRO_M4_MATRIX = 17  # Micro QR M4 is always 17×17
LABEL_SIZE_PX = (MICRO_M4_MATRIX + 2 * QR_BORDER_MOD) * QR_MODULE_PX  # = 84
LABEL_LONG_PX = 2 * LABEL_SIZE_PX                                     # = 168

_INNER_SVG_RE = re.compile(r"<svg[^>]*>(.*)</svg>", re.DOTALL)
RSVG = shutil.which("rsvg-convert")


def gen_ids(n: int, *, seed: int) -> list[str]:
    rng = random.Random(seed)
    return ["".join(rng.choices(ALPHABET, k=ID_LENGTH)) for _ in range(n)]


def strip_svg_wrapper(svg: str) -> str:
    m = _INNER_SVG_RE.search(svg)
    if not m:
        raise ValueError("expected <svg>…</svg> from label.render")
    return m.group(1).strip()


def compose_sheet(
    *,
    canvas_w_px: int,
    canvas_h_px: int,
    cols: int,
    rows: int,
    layout: str,
    label_size_px: int,
    ids: list[str],
    show_cell_lines: bool,
    micro: bool,
    border: int,
) -> str:
    """Compose a sheet in the printer's pixel grid.

    All internal coordinates are in printer pixels (1 unit = 1 dot at
    the printer's native resolution). The SVG also carries width/height
    in mm so PDF preview and physically-sized print drivers get the
    correct physical dimensions.
    """
    if layout == "horz":
        label_w_px, label_h_px = 2 * label_size_px, label_size_px
    elif layout == "vert":
        label_w_px, label_h_px = label_size_px, 2 * label_size_px
    else:
        raise ValueError(f"unknown layout {layout}")

    cell_w_px = label_w_px
    cell_h_px = label_h_px

    canvas_w_mm = canvas_w_px / PX_PER_MM
    canvas_h_mm = canvas_h_px / PX_PER_MM

    # Center the cell grid horizontally and vertically within the tape.
    margin_x = (canvas_w_px - cols * cell_w_px) // 2
    margin_y = (canvas_h_px - rows * cell_h_px) // 2

    parts: list[str] = [
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'width="{canvas_w_mm:.3f}mm" height="{canvas_h_mm:.3f}mm" '
        f'viewBox="0 0 {canvas_w_px} {canvas_h_px}">'
    ]

    # Faint outer border — useful to see whether the raster reaches the
    # physical edge of the tape on the printed output. Inset by 0.5 px
    # so the 1-px stroke centers on the canvas edge.
    parts.append(
        f'<rect x="0.5" y="0.5" '
        f'width="{canvas_w_px - 1}" height="{canvas_h_px - 1}" '
        f'fill="none" stroke="#bbb" stroke-width="1" stroke-dasharray="4,4"/>'
    )

    # Optional inter-cell guide lines (manual-cut references).
    if show_cell_lines:
        for c in range(1, cols):
            x = margin_x + c * cell_w_px
            parts.append(
                f'<line x1="{x}" y1="{margin_y}" '
                f'x2="{x}" y2="{margin_y + rows * cell_h_px}" '
                f'stroke="#ddd" stroke-width="0.5" stroke-dasharray="3,3"/>'
            )
        for r in range(1, rows):
            y = margin_y + r * cell_h_px
            parts.append(
                f'<line x1="{margin_x}" y1="{y}" '
                f'x2="{margin_x + cols * cell_w_px}" y2="{y}" '
                f'stroke="#ddd" stroke-width="0.5" stroke-dasharray="3,3"/>'
            )

    for i, canonical in enumerate(ids[:cols * rows]):
        col = i % cols
        row = i // cols
        ox = margin_x + col * cell_w_px
        oy = margin_y + row * cell_h_px
        # render() returns an SVG whose internal viewBox numbers equal
        # the `size` passed in. Calling it with size in px → inner coords
        # land on the same px grid as the outer sheet.
        inner = strip_svg_wrapper(
            render(canonical, layout, label_size_px, fmt=FMT, micro=micro, border=border)
        )
        parts.append(
            f'<g transform="translate({ox},{oy})">{inner}</g>'
        )

    parts.append("</svg>\n")
    return "\n".join(parts)


def convert(
    svg: Path, out: Path, *, fmt: str,
    png_width_px: int | None = None, png_height_px: int | None = None,
) -> None:
    if RSVG is None:
        sys.stderr.write(
            f"warning: rsvg-convert not on PATH; skipping {out.name} "
            "(brew install librsvg)\n"
        )
        return
    if fmt == "png":
        # Force exact pixel dimensions. We pass both -w and -h so that
        # rsvg-convert can't round the height via the mm-aspect path
        # (the SVG's mm width/height are %.3f-rounded and that 1-µm
        # rounding error otherwise nudges some heights up by one px).
        # With viewBox = output px dims, this is a 1:1 mapping → QR
        # modules land on integer dots, no antialiasing.
        flags = ["-w", str(png_width_px), "-h", str(png_height_px), "-b", "white"]
    else:
        # PDF respects the SVG's intrinsic mm width/height, so the
        # physical print size stays correct.
        flags = ["-f", "pdf"]
    subprocess.run([RSVG, *flags, str(svg), "-o", str(out)], check=True)


CASES = [
    # name, cols, rows, layout
    ("test_4x1_horz", 4, 1, "horz"),
    ("test_8x1_vert", 8, 1, "vert"),
    ("test_8x2_vert", 8, 2, "vert"),
]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--out-dir", type=Path,
        default=REPO_ROOT / "sheets" / "printer_test_62mm",
        help="output directory (default: sheets/printer_test_62mm/)",
    )
    ap.add_argument("--seed", type=int, default=20260511,
                    help="RNG seed for the test IDs (default 20260511)")
    ap.add_argument("--no-cell-lines", dest="cell_lines", action="store_false",
                    help="omit the inter-cell dashed guide lines")
    ap.add_argument("--no-micro", dest="micro", action="store_false",
                    help="use Standard QR V1 instead of Micro QR M4 "
                    "(default: Micro — bigger modules at this size)")
    ap.add_argument("--qr-border", type=int, default=QR_BORDER_MOD,
                    help=f"QR quiet-zone in modules (default {QR_BORDER_MOD} "
                    "= Micro QR M4 spec; lower grows the visible matrix at "
                    "the cost of scanner tolerance and breaks the integer-px "
                    "module count assumed by the canvas math)")
    ap.add_argument("--no-png", dest="png", action="store_false")
    ap.add_argument("--no-pdf", dest="pdf", action="store_false")
    ap.set_defaults(cell_lines=True, micro=True, png=True, pdf=True)
    args = ap.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)

    total = sum(cols * rows for _, cols, rows, *_ in CASES)
    ids_pool = gen_ids(total, seed=args.seed)

    qr_kind = "Micro QR M4" if args.micro else "Standard QR V1"
    print(
        f"composing {len(CASES)} test sheets "
        f"(canvas {TAPE_WIDTH_PX} px = {TAPE_WIDTH_MM} mm wide, "
        f"{qr_kind} module={QR_MODULE_PX}px border={args.qr_border}mod, "
        f"{total} unique IDs seed={args.seed})"
    )
    idx = 0
    for name, cols, rows, layout in CASES:
        n = cols * rows
        ids = ids_pool[idx:idx + n]
        idx += n

        if layout == "horz":
            canvas_h_px = rows * LABEL_SIZE_PX
        else:
            canvas_h_px = rows * LABEL_LONG_PX

        svg = compose_sheet(
            canvas_w_px=TAPE_WIDTH_PX,
            canvas_h_px=canvas_h_px,
            cols=cols, rows=rows,
            layout=layout,
            label_size_px=LABEL_SIZE_PX,
            ids=ids,
            show_cell_lines=args.cell_lines,
            micro=args.micro,
            border=args.qr_border,
        )
        svg_path = args.out_dir / f"{name}.svg"
        svg_path.write_text(svg)
        if args.png:
            convert(svg_path, svg_path.with_suffix(".png"),
                    fmt="png",
                    png_width_px=TAPE_WIDTH_PX,
                    png_height_px=canvas_h_px)
        if args.pdf:
            convert(svg_path, svg_path.with_suffix(".pdf"), fmt="pdf")

        canvas_h_mm = canvas_h_px / PX_PER_MM
        print(
            f"  {name}  {cols}×{rows} {layout:4s}  "
            f"{TAPE_WIDTH_PX}×{canvas_h_px} px  "
            f"({TAPE_WIDTH_MM:g}×{canvas_h_mm:.2f} mm)  "
            f"({n} labels)"
        )

    print(f"out: {args.out_dir}/")


if __name__ == "__main__":
    main()
