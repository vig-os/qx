#!/usr/bin/env python3
"""Mint nano-id part IDs and append them to registry.csv. No labels.

Use `label.py` to render the SVGs after minting (or skip it entirely
for software-only use).

    uv run system-design/parts/mint.py --count 50
    uv run system-design/parts/mint.py --count 50 --batch B-2026-05-sdmd

See ADR-012 for the scheme.
"""
from __future__ import annotations

import argparse
import csv
import sys
from datetime import datetime, timezone
from pathlib import Path

import nanoid
from registry_contract import ALPHABET, ID_LENGTH, REGISTRY_FIELDS

PARTS_DIR = Path(__file__).resolve().parent
REGISTRY = PARTS_DIR / "registry.csv"

def mint_id(existing: set[str]) -> str:
    for _ in range(16):
        c = nanoid.generate(ALPHABET, ID_LENGTH)
        if c not in existing:
            return c
    raise RuntimeError("nanoid keeps colliding — registry corrupt or RNG broken")


def load_existing_ids() -> set[str]:
    if not REGISTRY.exists():
        return set()
    with REGISTRY.open() as f:
        return {row["id"] for row in csv.DictReader(f) if row.get("id")}


def append_rows(rows: list[dict]) -> None:
    new_file = not REGISTRY.exists() or REGISTRY.stat().st_size == 0
    with REGISTRY.open("a", newline="") as f:
        w = csv.DictWriter(f, fieldnames=REGISTRY_FIELDS, extrasaction="ignore")
        if new_file:
            w.writeheader()
        for row in rows:
            w.writerow(row)


def mint_batch(count: int, batch: str | None = None) -> tuple[list[str], str, str]:
    """Generate `count` fresh IDs and append unbound rows. Importable.

    Returns (new_ids, batch_label, minted_at_isoformat).
    """
    if count < 1:
        raise ValueError("count must be >= 1")
    now = datetime.now(timezone.utc)
    batch = batch or now.strftime("B-%Y-%m-%d-%H%M")
    minted_at = now.isoformat(timespec="seconds")

    existing = load_existing_ids()
    new_ids: list[str] = []
    for _ in range(count):
        nid = mint_id(existing)
        existing.add(nid)
        new_ids.append(nid)

    append_rows([
        {"id": nid, "status": "unbound", "minted_at": minted_at, "batch": batch}
        for nid in new_ids
    ])
    return new_ids, batch, minted_at


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--count", type=int, required=True, help="how many IDs to mint")
    ap.add_argument("--batch", default=None, help="batch label (default: B-YYYY-MM-DD-HHMM)")
    args = ap.parse_args()

    try:
        new_ids, batch, _ = mint_batch(args.count, args.batch)
    except ValueError as e:
        sys.exit(str(e))

    print(f"minted {len(new_ids)} ids in batch {batch}")
    print(f"  registry: {REGISTRY}")
    for nid in new_ids:
        print(f"    {nid}")
    print(f"\nrender labels:  uv run system-design/parts/label.py --batch {batch} "
          f"--layout horz")


if __name__ == "__main__":
    main()
