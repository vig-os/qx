#!/usr/bin/env python3
"""Compose multi-up label sheets for fixed-format paper.

Today: Brother QL DK-1201 die-cut (29 × 90 mm physical, 25 × 80 mm
printable). Long axis is horizontal (the printer feed direction).

Usage:

    uv run tools/sheet.py --batch B-2026-05-08 --paper dk-1201 \\
        --rows 3 --cols 5 --layout horz --size 8 \\
        --out-dir sheets/

Output: one SVG + PNG + PDF per sheet, named
`<batch>-<paper>-<rows>x<cols>-sheet-<N>.{svg,png,pdf}`.

Each rendered ID is also appended as a print event to `print_log.csv`
(reuses `label.append_print_events`) — same audit trail as `label.py`.
Pass `--no-log` to skip.

This is a CLI counterpart to the planned web-app DK-1201 die-cut
output mode (sub-task of issue #11). When the web app's output-mode
plugin lands the two paths produce equivalent sheets.
"""
from __future__ import annotations

import argparse
import csv
import math
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT))

from label import (  # noqa: E402
    PRINT_LOG_FIELDS,
    REGISTRY,
    _layout_extra,
    append_print_events,
    render,
)

# Paper formats: physical and printable area (mm).
# `axis` = which paper dim is horizontal in the SVG. For DK rolls fed
# by the QL series, the long axis is the feed direction, which we keep
# horizontal so the grid reads col-by-col in the natural way.
PAPERS = {
    "dk-1201": {
        "physical_w": 90,
        "physical_h": 29,
        "printable_w": 80,
        "printable_h": 25,
        "margin_x": 5,  # (physical_w - printable_w) / 2
        "margin_y": 2,  # (physical_h - printable_h) / 2
        "label_id": "DK-1201 (29 × 90 mm die-cut, 25 × 80 mm printable)",
    },
}

RSVG = shutil.which("rsvg-convert")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--batch", required=True, help="batch label in registry.csv to source IDs from")
    ap.add_argument("--paper", default="dk-1201", choices=list(PAPERS))
    ap.add_argument("--rows", type=int, required=True,
                    help="number of rows along the short paper dim "
                    "(stacked vertically when long axis is horizontal)")
    ap.add_argument("--cols", type=int, required=True,
                    help="number of cols along the long paper dim "
                    "(side-by-side along feed direction)")
    ap.add_argument("--layout", default="horz", choices=["vert", "horz", "flag"])
    ap.add_argument("--size", type=float, default=8.0,
                    help="label short-side size in mm (default 8)")
    ap.add_argument("--cable-od", type=float, default=None,
                    help="required for --layout flag")
    ap.add_argument("--micro", action="store_true",
                    help="encode as Micro QR M4 (52%% area of Standard QR)")
    ap.add_argument("--out-dir", type=Path, required=True,
                    help="output directory for the per-sheet SVG/PNG/PDF")
    ap.add_argument("--operator", default=os.getenv("USER", "unknown"))
    ap.add_argument("--no-log", dest="log", action="store_false",
                    help="skip the print_log.csv append")
    ap.add_argument("--no-png", dest="png", action="store_false",
                    help="skip PNG generation")
    ap.add_argument("--no-pdf", dest="pdf", action="store_false",
                    help="skip PDF generation")
    ap.add_argument("--bind-template", type=Path, default=None,
                    help="also write a fill-in CSV template for the printed IDs "
                    "(columns: id, human_id, type, description, vendor, "
                    "part_number, location, notes). Edit in a spreadsheet, "
                    "ingest later via a bind tool.")
    ap.set_defaults(log=True, png=True, pdf=True)
    args = ap.parse_args()

    paper = PAPERS[args.paper]

    if args.layout == "flag" and args.cable_od is None:
        sys.exit("--layout flag requires --cable-od <mm>")

    # Source rows from registry batch.
    if not REGISTRY.exists():
        sys.exit(f"no registry at {REGISTRY}")
    with REGISTRY.open() as f:
        all_rows = list(csv.DictReader(f))
    rows = [r for r in all_rows if r.get("batch") == args.batch]
    if not rows:
        sys.exit(f"no rows in batch {args.batch}")

    # Sort by id for deterministic sheet order (same input → same output).
    rows.sort(key=lambda r: r["id"])

    per_sheet = args.rows * args.cols
    n_sheets = math.ceil(len(rows) / per_sheet)

    # Verify the rendered label fits the cell.
    label_w, label_h = _label_dims(args.layout, args.size, args.cable_od)
    cell_w_long = paper["printable_w"] / args.cols   # along x (long axis)
    cell_h_short = paper["printable_h"] / args.rows  # along y (short axis)
    if label_w > cell_w_long + 1e-6 or label_h > cell_h_short + 1e-6:
        sys.exit(
            f"label {label_w:g}×{label_h:g} mm doesn't fit cell "
            f"{cell_w_long:g}×{cell_h_short:g} mm "
            f"(printable {paper['printable_w']}×{paper['printable_h']}, "
            f"grid {args.cols}×{args.rows}). "
            f"Reduce --size, change --layout, or change --rows/--cols."
        )

    args.out_dir.mkdir(parents=True, exist_ok=True)

    base_name = f"{args.batch}-{args.paper}-{args.rows}x{args.cols}-sheet"
    output_mode = (
        f"{args.paper}-{args.rows}x{args.cols}"
        + ("-micro" if args.micro else "")
    )

    printed_ids: list[str] = []
    written: list[Path] = []

    for sheet_idx in range(n_sheets):
        chunk = rows[sheet_idx * per_sheet:(sheet_idx + 1) * per_sheet]
        sheet_path = args.out_dir / f"{base_name}-{sheet_idx + 1:02d}.svg"
        _write_sheet(
            sheet_path,
            chunk_ids=[r["id"] for r in chunk],
            paper=paper,
            rows_n=args.rows,
            cols_n=args.cols,
            layout=args.layout,
            size=args.size,
            cable_od=args.cable_od,
            micro=args.micro,
            cell_w_long=cell_w_long,
            cell_h_short=cell_h_short,
            label_w=label_w,
            label_h=label_h,
        )
        written.append(sheet_path)
        printed_ids.extend(r["id"] for r in chunk)

        if args.png:
            _convert(sheet_path, sheet_path.with_suffix(".png"), fmt="png")
        if args.pdf:
            _convert(sheet_path, sheet_path.with_suffix(".pdf"), fmt="pdf")

    # Log a single print event per ID (one row, copies=1, output_mode names the layout).
    if args.log and printed_ids:
        append_print_events(
            printed_ids,
            layout=args.layout,
            size_mm=args.size,
            extra=_layout_extra(args.layout, args.cable_od),
            copies=1,
            output_mode=output_mode,
            operator=args.operator,
            batch_label=args.batch,
            registry_ids={r["id"] for r in all_rows},
        )

    # Optional fill-in template — same row order as the printed sheets so
    # filling left-to-right top-to-bottom matches the physical layout.
    if args.bind_template:
        _write_bind_template(args.bind_template, printed_ids)
        print(f"  template: {args.bind_template} ({len(printed_ids)} rows)")

    # Summary.
    print(f"composed {n_sheets} sheet(s) ({len(printed_ids)} labels) for batch {args.batch}")
    print(f"  paper:  {paper['label_id']}")
    print(f"  grid:   {args.rows} rows × {args.cols} cols → cell {cell_w_long:g}×{cell_h_short:g} mm")
    print(f"  label:  {args.layout} size={args.size}mm "
          f"({label_w:g}×{label_h:g} mm){' [Micro QR]' if args.micro else ''}")
    print(f"  out:    {args.out_dir}/")
    for p in written:
        sibs = [p.name]
        if args.png and p.with_suffix(".png").exists():
            sibs.append(p.with_suffix(".png").name)
        if args.pdf and p.with_suffix(".pdf").exists():
            sibs.append(p.with_suffix(".pdf").name)
        print(f"    {', '.join(sibs)}")
    if args.log:
        print(f"  logged {len(printed_ids)} print event(s) (output_mode={output_mode})")
    else:
        print("  --no-log: print_log.csv not touched")


def _label_dims(layout: str, size: float, cable_od: float | None) -> tuple[float, float]:
    if layout == "horz":
        return 2 * size, size
    if layout == "vert":
        return size, 2 * size
    if layout == "flag":
        wrap = math.pi * (cable_od or 0) * 1.1
        return 4 * size + wrap, size
    raise ValueError(f"unknown layout {layout}")


_INNER_SVG_RE = re.compile(r"<svg[^>]*>(.*)</svg>", re.DOTALL)


def _strip_svg_wrapper(svg: str) -> str:
    """Return the inner content of an <svg>…</svg> document.

    label.render() emits a self-contained SVG; we want only the QR/text
    primitives so we can re-position them inside a sheet via translate.
    """
    m = _INNER_SVG_RE.search(svg)
    if not m:
        raise ValueError("expected <svg>…</svg> document from label.render")
    return m.group(1).strip()


def _write_sheet(
    path: Path,
    *,
    chunk_ids: list[str],
    paper: dict,
    rows_n: int,
    cols_n: int,
    layout: str,
    size: float,
    cable_od: float | None,
    micro: bool,
    cell_w_long: float,
    cell_h_short: float,
    label_w: float,
    label_h: float,
) -> None:
    physical_w = paper["physical_w"]
    physical_h = paper["physical_h"]
    margin_x = paper["margin_x"]
    margin_y = paper["margin_y"]

    parts: list[str] = []
    parts.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'width="{physical_w}mm" height="{physical_h}mm" '
        f'viewBox="0 0 {physical_w} {physical_h}">'
    )
    # Cut-line border (the physical label outline) — gray, dashed,
    # printer-driver-friendly.
    parts.append(
        f'<rect x="0.05" y="0.05" width="{physical_w - 0.1}" height="{physical_h - 0.1}" '
        f'fill="none" stroke="#bbb" stroke-width="0.1" stroke-dasharray="0.5,0.5"/>'
    )
    # Printable-area outline — lighter, for orientation reference.
    parts.append(
        f'<rect x="{margin_x}" y="{margin_y}" '
        f'width="{paper["printable_w"]}" height="{paper["printable_h"]}" '
        f'fill="none" stroke="#eee" stroke-width="0.05"/>'
    )

    for i, canonical in enumerate(chunk_ids):
        col = i % cols_n
        row = i // cols_n
        # Cell origin in the sheet (printable-area-relative, then offset by margin).
        cell_x = margin_x + col * cell_w_long
        cell_y = margin_y + row * cell_h_short
        # Center the label within the cell.
        offset_x = cell_x + (cell_w_long - label_w) / 2
        offset_y = cell_y + (cell_h_short - label_h) / 2
        label_svg = render(canonical, layout, size, cable_od, micro=micro)
        inner = _strip_svg_wrapper(label_svg)
        parts.append(
            f'<g transform="translate({offset_x:.3f},{offset_y:.3f})">'
            f'{inner}'
            f'</g>'
        )

    parts.append("</svg>\n")
    path.write_text("\n".join(parts))


BIND_TEMPLATE_FIELDS = [
    "id", "human_id",
    "type", "description", "vendor", "part_number", "location", "notes",
]


def _write_bind_template(path: Path, ids: list[str]) -> None:
    """Write a fill-in CSV with one row per printed ID.

    The `id` column is the canonical 12-char string (matches what gets
    scanned and what binds reference). The `human_id` column is the
    4-4-4 dashed form for visually cross-referencing with the printed
    sticker — operators read that off the physical part.

    The remaining columns mirror the editable fields of `registry.csv`,
    pre-blank, ready to be filled in a spreadsheet. Sort order is by
    `id` so the file diffs cleanly when re-imported.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    sorted_ids = sorted(set(ids))
    with path.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=BIND_TEMPLATE_FIELDS)
        w.writeheader()
        for nid in sorted_ids:
            w.writerow({
                "id": nid,
                "human_id": f"{nid[:4]}-{nid[4:8]}-{nid[8:12]}",
                "type": "",
                "description": "",
                "vendor": "",
                "part_number": "",
                "location": "",
                "notes": "",
            })


def _convert(svg: Path, out: Path, *, fmt: str) -> None:
    if RSVG is None:
        sys.stderr.write(
            "warning: rsvg-convert not on PATH; skipping "
            f"{out.name} (install librsvg via brew)\n"
        )
        return
    flags = ["-d", "300", "-p", "300", "-b", "white"] if fmt == "png" else ["-f", "pdf"]
    subprocess.run([RSVG, *flags, str(svg), "-o", str(out)], check=True)


# Forward-compat: import this module to access the same constants
# label.py uses, so future tooling (e.g. a `tools/sheet_a4.py`) can
# share the print_log writer.
__all__ = [
    "PAPERS",
    "PRINT_LOG_FIELDS",
    "main",
]


if __name__ == "__main__":
    main()
