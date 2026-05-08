#!/usr/bin/env python3
"""Bind an unbound nano-id to a physical part (fill registry metadata).

Accepts either the full 12-char canonical ID or the 8-char human prefix
(with or without dash). On prefix collision, prints all matches and refuses
to bind without disambiguation.

    uv run system-design/parts/bind.py K7M3-PQ9R \\
        --type "PT100 1/3 DIN class B, 4-wire" \\
        --vendor "TC Direct" --part-number "402-141" \\
        --location "sdmd_v2 / cooling-loop / supply-T"
"""
from __future__ import annotations

import argparse
import csv
import os
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

PARTS_DIR = Path(__file__).resolve().parent
REGISTRY = PARTS_DIR / "registry.csv"

REGISTRY_FIELDS = [
    "id", "status", "minted_at", "batch", "bound_at",
    "type", "description", "vendor", "part_number", "location", "notes",
]

ID_LENGTH = 12
HUMAN_LENGTH = 8


def normalize(query: str) -> str:
    """Strip dashes/whitespace, uppercase. `K7M3-PQ9R` → `K7M3PQ9R`."""
    return query.replace("-", "").replace(" ", "").strip().upper()


def find_matches(query: str, rows: list[dict]) -> list[dict]:
    q = normalize(query)
    if len(q) == ID_LENGTH:
        return [r for r in rows if r["id"] == q]
    if len(q) >= HUMAN_LENGTH:
        return [r for r in rows if r["id"].startswith(q)]
    sys.exit(f"query too short ({len(q)} chars); need >= {HUMAN_LENGTH}")


def write_atomic(rows: list[dict]) -> None:
    fd, tmp_path = tempfile.mkstemp(prefix=".registry.", suffix=".csv", dir=PARTS_DIR)
    try:
        with os.fdopen(fd, "w", newline="") as f:
            w = csv.DictWriter(f, fieldnames=REGISTRY_FIELDS, extrasaction="ignore")
            w.writeheader()
            for row in rows:
                w.writerow(row)
        os.replace(tmp_path, REGISTRY)
    except Exception:
        Path(tmp_path).unlink(missing_ok=True)
        raise


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("id", help="full 12-char canonical or 8-char human prefix")
    ap.add_argument("--type", help="part type, e.g. 'PT100 1/3 DIN class B, 4-wire'")
    ap.add_argument("--description", help="free-text description")
    ap.add_argument("--vendor")
    ap.add_argument("--part-number", dest="part_number")
    ap.add_argument("--location", help="where the part lives, e.g. 'sdmd_v2 / cooling-loop'")
    ap.add_argument("--notes")
    ap.add_argument(
        "--rebind", action="store_true",
        help="allow rewriting metadata on an already-bound ID",
    )
    ap.add_argument(
        "--void", action="store_true",
        help="mark this ID as void (sticker spoiled / lost) instead of binding",
    )
    args = ap.parse_args()

    if not REGISTRY.exists():
        sys.exit(f"no registry at {REGISTRY} — mint some IDs first")

    with REGISTRY.open() as f:
        rows = list(csv.DictReader(f))

    matches = find_matches(args.id, rows)
    if not matches:
        sys.exit(f"no match for '{args.id}'")
    if len(matches) > 1:
        print(f"ambiguous prefix '{args.id}' — {len(matches)} matches:", file=sys.stderr)
        for m in matches:
            label = m.get("type") or m.get("description") or "(unbound)"
            loc = m.get("location") or "-"
            print(f"  {m['id']}  status={m['status']}  {label}  @ {loc}", file=sys.stderr)
        sys.exit(2)

    target = matches[0]
    nid = target["id"]
    now = datetime.now(timezone.utc).isoformat(timespec="seconds")

    if args.void:
        target["status"] = "void"
        target["notes"] = (target.get("notes") or "") + f" [voided {now}]"
        write_atomic(rows)
        print(f"voided {nid}")
        return

    if target["status"] == "bound" and not args.rebind:
        sys.exit(
            f"{nid} is already bound to '{target.get('type') or target.get('description')}'. "
            f"Pass --rebind to overwrite."
        )
    if target["status"] == "void":
        sys.exit(f"{nid} is voided; cannot bind. Mint a new ID.")

    updates = {
        "status": "bound",
        "bound_at": now,
        "type": args.type or target.get("type", ""),
        "description": args.description or target.get("description", ""),
        "vendor": args.vendor or target.get("vendor", ""),
        "part_number": args.part_number or target.get("part_number", ""),
        "location": args.location or target.get("location", ""),
        "notes": args.notes or target.get("notes", ""),
    }
    target.update(updates)
    write_atomic(rows)

    print(f"bound {nid}")
    for k in ("type", "description", "vendor", "part_number", "location", "notes"):
        if target.get(k):
            print(f"  {k:14s} {target[k]}")


if __name__ == "__main__":
    main()
