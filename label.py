#!/usr/bin/env python3
"""Render SVG labels for IDs already in the registry.

A label is two equal-size square blocks: QR + 4/4/4 text.

    vert  — QR on top of text   (size × 2*size, aspect 1:2)
    horz  — QR left of text     (2*size × size, aspect 2:1)
    flag  — horz mirrored around a cable wrap zone

Pick which IDs to render with --id, --batch, or --status (combinable).
Pick geometry with --size <mm> or --tape pt-N.

    uv run system-design/parts/label.py --batch B-2026-05-sdmd --layout horz
    uv run system-design/parts/label.py --id K7M3PQ9RT5VA --layout vert --size 8
    uv run system-design/parts/label.py --status unbound --layout flag --size 11 --cable-od 6

See ADR-012 for the scheme.
"""
from __future__ import annotations

import argparse
import csv
import json
import math
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

import segno

PARTS_DIR = Path(__file__).resolve().parent
REGISTRY = PARTS_DIR / "registry.csv"
PRINT_LOG = PARTS_DIR / "print_log.csv"
LABELS_DIR = PARTS_DIR / "labels"

PRINT_LOG_FIELDS = [
    "id", "printed_at", "printed_by", "layout", "size_mm",
    "extra", "copies", "output_mode", "batch_label",
]

# Tape printable-height presets, in mm of short-side. Two families:
#
#   pt-N  — Brother P-touch (TZe tapes), e.g. PT-D series printers.
#           N = nominal tape width; printable ≈ tape × 0.75.
#   dk-N  — Brother QL DK continuous tapes, e.g. QL-820NWBc.
#           N = nominal tape width; printable ≈ tape × 0.85 (less margin).
#
# DK rolls used in the lab today:
#   DK-22214 (12 mm), DK-22210 (29 mm), DK-22225 (38 mm), DK-22205 (62 mm).
TAPE_SIZES = {
    "pt-9":  6.5,
    "pt-12": 9.0,
    "pt-18": 12.0,
    "pt-24": 18.0,
    "pt-36": 28.0,
    "dk-12": 10.0,
    "dk-29": 25.0,
    "dk-38": 33.0,
    "dk-62": 56.0,
}

DEFAULT_SIZE_MM = 11.0
QR_BORDER_MODULES = 4


def four_four_four(canonical: str) -> tuple[str, str, str]:
    return canonical[0:4], canonical[4:8], canonical[8:12]


# ---------- SVG primitives (mm-native) ----------

def svg_wrap(w_mm: float, h_mm: float, body: str) -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'width="{w_mm:.3f}mm" height="{h_mm:.3f}mm" '
        f'viewBox="0 0 {w_mm:.3f} {h_mm:.3f}">\n'
        f'{body}\n</svg>\n'
    )


def qr_block(canonical: str, x: float, y: float, size: float) -> str:
    matrix = segno.make(canonical, error="m", micro=False).matrix
    n_modules = len(matrix) + 2 * QR_BORDER_MODULES
    module = size / n_modules
    rects = []
    for r, row in enumerate(matrix):
        for c, v in enumerate(row):
            if v:
                rx = x + (c + QR_BORDER_MODULES) * module
                ry = y + (r + QR_BORDER_MODULES) * module
                rects.append(
                    f'<rect x="{rx:.3f}" y="{ry:.3f}" '
                    f'width="{module:.3f}" height="{module:.3f}" fill="#000"/>'
                )
    return "\n".join(rects)


def text_block(canonical: str, x: float, y: float, size: float) -> str:
    rows = four_four_four(canonical)
    inner_h = size * 0.8
    font = inner_h / 3.6  # 3*font + 2*(0.3*font) = 3.6*font
    gap = font * 0.3
    cx = x + size / 2
    y0 = y + (size - inner_h) / 2 + font * 0.85
    return "\n".join(
        f'<text x="{cx:.3f}" y="{y0 + i * (font + gap):.3f}" '
        f'font-family="Courier, monospace" font-size="{font:.3f}" '
        f'text-anchor="middle" fill="#000">{row}</text>'
        for i, row in enumerate(rows)
    )


# ---------- Layouts ----------

def render_vert(canonical: str, size: float) -> str:
    body = qr_block(canonical, 0, 0, size) + "\n" + text_block(canonical, 0, size, size)
    return svg_wrap(size, 2 * size, body)


def render_horz(canonical: str, size: float) -> str:
    body = qr_block(canonical, 0, 0, size) + "\n" + text_block(canonical, size, 0, size)
    return svg_wrap(2 * size, size, body)


def render_flag(canonical: str, size: float, cable_od_mm: float) -> str:
    horz_w = 2 * size
    wrap_w = math.pi * cable_od_mm * 1.1
    W = 2 * horz_w + wrap_w
    left = qr_block(canonical, 0, 0, size) + "\n" + text_block(canonical, size, 0, size)
    rx = horz_w + wrap_w
    right = text_block(canonical, rx, 0, size) + "\n" + qr_block(canonical, rx + size, 0, size)
    wrap = (
        f'<rect x="{horz_w:.3f}" y="0" width="{wrap_w:.3f}" height="{size:.3f}" '
        f'fill="none" stroke="#888" stroke-width="0.1" stroke-dasharray="0.6,0.6"/>\n'
        f'<text x="{horz_w + wrap_w/2:.3f}" y="{size/2 + 0.5:.3f}" '
        f'font-family="Courier, monospace" font-size="1.5" '
        f'text-anchor="middle" fill="#888">wrap d{cable_od_mm:g}</text>'
    )
    return svg_wrap(W, size, "\n".join([left, wrap, right]))


def render(canonical: str, layout: str, size: float, cable_od_mm: float | None) -> str:
    if layout == "vert":
        return render_vert(canonical, size)
    if layout == "horz":
        return render_horz(canonical, size)
    if layout == "flag":
        if cable_od_mm is None:
            sys.exit("--layout flag requires --cable-od <mm>")
        return render_flag(canonical, size, cable_od_mm)
    sys.exit(f"unknown layout: {layout}")


# ---------- Print event log ----------

def _layout_extra(layout: str, cable_od_mm: float | None) -> dict:
    """Layout-specific options that belong in the print_log `extra` column."""
    if layout == "flag" and cable_od_mm is not None:
        return {"cableOd": cable_od_mm}
    return {}


def append_print_events(
    ids: list[str],
    *,
    layout: str,
    size_mm: float,
    extra: dict,
    copies: int,
    output_mode: str,
    operator: str,
    batch_label: str,
    registry_ids: set[str],
) -> None:
    """Append one row per ID to print_log.csv and re-sort by printed_at.

    `copies` is logged as a single row per ID (not duplicated). FK to
    registry.csv is enforced softly: missing IDs warn to stderr but still
    log — the CI validator is the source of truth for orphan events.
    """
    printed_at = datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    extra_str = json.dumps(extra, separators=(",", ":"), sort_keys=True)

    orphans = [i for i in ids if i not in registry_ids]
    if orphans:
        print(
            f"warning: {len(orphans)} id(s) logged but not in registry.csv "
            f"(FK orphans will be flagged by CI): {', '.join(orphans[:3])}"
            f"{'…' if len(orphans) > 3 else ''}",
            file=sys.stderr,
        )

    new_rows = [
        {
            "id": nid,
            "printed_at": printed_at,
            "printed_by": operator,
            "layout": layout,
            "size_mm": f"{size_mm:g}",
            "extra": extra_str,
            "copies": str(copies),
            "output_mode": output_mode,
            "batch_label": batch_label,
        }
        for nid in ids
    ]

    existing_rows: list[dict] = []
    if PRINT_LOG.exists() and PRINT_LOG.stat().st_size > 0:
        with PRINT_LOG.open() as f:
            existing_rows = list(csv.DictReader(f))

    all_rows = existing_rows + new_rows
    all_rows.sort(key=lambda r: r.get("printed_at", ""))

    with PRINT_LOG.open("w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=PRINT_LOG_FIELDS, extrasaction="ignore")
        w.writeheader()
        for row in all_rows:
            w.writerow(row)


# ---------- ID selection ----------

def select_ids(
    rows: list[dict],
    explicit_ids: list[str] | None,
    batch: str | None,
    status: str | None,
) -> list[dict]:
    if not (explicit_ids or batch or status):
        sys.exit("specify at least one of --id, --batch, --status")

    selected = rows
    if explicit_ids:
        wanted = {i.upper().replace("-", "") for i in explicit_ids}
        selected = [r for r in selected if r["id"] in wanted]
        missing = wanted - {r["id"] for r in selected}
        if missing:
            sys.exit(f"unknown ID(s): {', '.join(sorted(missing))}")
    if batch:
        selected = [r for r in selected if r.get("batch") == batch]
    if status:
        selected = [r for r in selected if r.get("status") == status]
    return selected


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--id", action="append", dest="ids",
                    help="explicit ID (12-char). Repeat for multiple.")
    ap.add_argument("--batch", default=None, help="render every ID in this batch")
    ap.add_argument("--status", choices=["unbound", "bound", "void"], default=None,
                    help="render every ID with this status")
    ap.add_argument("--layout", choices=["vert", "horz", "flag"], default="horz")
    ap.add_argument("--size", type=float, default=None,
                    help=f"short-side size in mm (default {DEFAULT_SIZE_MM})")
    ap.add_argument("--tape", choices=list(TAPE_SIZES), default=None,
                    help="Brother P-touch tape preset (shorthand for --size)")
    ap.add_argument("--cable-od", type=float, default=None,
                    help="cable outer diameter in mm (required for --layout flag)")
    ap.add_argument("--out-dir", type=Path, default=None,
                    help="output directory (default: labels/<descriptor>)")
    ap.add_argument("--copies", type=int, default=1,
                    help="copies per ID (recorded in print_log; default 1). "
                    "Does not duplicate rendered SVGs — handle copies at the "
                    "print queue. Only affects the event log row.")
    log_group = ap.add_mutually_exclusive_group()
    log_group.add_argument("--log", dest="log", action="store_true",
                           help="append a row per ID to print_log.csv after "
                           "rendering (default)")
    log_group.add_argument("--no-log", dest="log", action="store_false",
                           help="render only; do not append to print_log.csv")
    ap.set_defaults(log=True)
    ap.add_argument("--operator", default=os.getenv("USER", "unknown"),
                    help="operator name recorded in print_log.printed_by "
                    "(default: $USER, or 'unknown')")
    ap.add_argument("--output-mode", default="dk-continuous-auto-cut",
                    help="print pipeline descriptor recorded in "
                    "print_log.output_mode (default: dk-continuous-auto-cut)")
    args = ap.parse_args()

    if args.copies < 1:
        sys.exit("--copies must be >= 1")

    if args.tape and args.size is not None:
        sys.exit("use either --size or --tape, not both")
    size = TAPE_SIZES[args.tape] if args.tape else (args.size or DEFAULT_SIZE_MM)

    if not REGISTRY.exists():
        sys.exit(f"no registry at {REGISTRY} — mint some IDs first")
    with REGISTRY.open() as f:
        rows = list(csv.DictReader(f))

    selected = select_ids(rows, args.ids, args.batch, args.status)
    if not selected:
        sys.exit("no IDs matched the selection")

    if args.out_dir:
        out_dir = args.out_dir
    else:
        descriptor = args.batch or args.status or "ad-hoc"
        out_dir = LABELS_DIR / f"{descriptor}-{args.layout}-s{size:g}"
    out_dir.mkdir(parents=True, exist_ok=True)

    for row in selected:
        nid = row["id"]
        svg = render(nid, args.layout, size, args.cable_od)
        (out_dir / f"{nid}.svg").write_text(svg)

    if args.log:
        # batch_label: prefer explicit --batch, otherwise fall back to the
        # row's batch field if every selected row shares one batch (common
        # when selecting by --status or --id from a single batch).
        if args.batch:
            batch_label = args.batch
        else:
            batches = {r.get("batch") or "" for r in selected}
            batch_label = batches.pop() if len(batches) == 1 else ""
        append_print_events(
            [r["id"] for r in selected],
            layout=args.layout,
            size_mm=size,
            extra=_layout_extra(args.layout, args.cable_od),
            copies=args.copies,
            output_mode=args.output_mode,
            operator=args.operator,
            batch_label=batch_label,
            registry_ids={r["id"] for r in rows},
        )

    if args.layout == "vert":
        dim = f"{size:.1f} × {2 * size:.1f} mm"
    elif args.layout == "horz":
        dim = f"{2 * size:.1f} × {size:.1f} mm"
    else:
        wrap_w = math.pi * (args.cable_od or 0) * 1.1
        dim = f"{4 * size + wrap_w:.1f} × {size:.1f} mm (wrap {wrap_w:.1f})"
    print(f"rendered {len(selected)} labels  layout={args.layout}  ({dim})")
    print(f"  out: {out_dir}/")
    if args.log:
        print(f"  logged {len(selected)} print event(s) to {PRINT_LOG.name}")


if __name__ == "__main__":
    main()
