#!/usr/bin/env python3
"""Bench the ADR obligations shortlist so nothing falls out of the ADRs silently.

Reads `decisions/obligations.toml` (the structured "what falls out of the ADRs"
feeder, per ADR-030 §8 / ADR-029 dimension 4) and checks reality against it:

  - schema: every row has the fields its `status` requires;
  - satisfied rows: `satisfied_by` path(s)/glob(s) actually resolve;
  - pending rows: carry a `tracking` pointer (work isn't lost, just open);
  - exempt rows: carry `exempt_until` + `exempt_reason`, and the date hasn't passed;
  - coverage: every in-force ADR (decisions/ADR-NNN-*.md, minus [meta].excluded)
    has >=1 row — so a new ADR can't land without declaring its obligations;
  - orphans: no row points at an ADR (or excluded entry) that doesn't exist.

Exit codes (mirror ADR-029): 0 ok · 1 missing/unsatisfied · 2 orphan · 3 expired
exemption. Precedence when several apply: 3 > 2 > 1. `pending` rows are reported
but never fail (foundation work in flight is legitimate).

Usage:
  obligations_check.py [--json PATH]   # also write feeder-JSON (PATH or - for stdout)
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
OBLIGATIONS = REPO / "decisions" / "obligations.toml"
DECISIONS = REPO / "decisions"

VALID_KIND = {"crate", "gate", "test", "artifact", "doc", "ci", "issue"}
VALID_STATUS = {"satisfied", "pending", "exempt"}
ADR_FILE_RE = re.compile(r"^(ADR-\d+)-.*\.md$")


def adr_ids_on_disk() -> set[str]:
    ids = set()
    for p in DECISIONS.glob("ADR-*.md"):
        m = ADR_FILE_RE.match(p.name)
        if m:
            ids.add(m.group(1))
    return ids


def resolves(pattern: str) -> bool:
    # Treat as a path first (covers literal paths with no glob chars), then glob.
    if (REPO / pattern).exists():
        return True
    return bool(list(REPO.glob(pattern)))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", metavar="PATH", help="write feeder-JSON (PATH or - for stdout)")
    args = ap.parse_args()

    try:
        data = tomllib.loads(OBLIGATIONS.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"FATAL: cannot read {OBLIGATIONS.relative_to(REPO)}: {e}", file=sys.stderr)
        return 1

    on_disk = adr_ids_on_disk()
    meta = data.get("meta", {})
    excluded = {e["adr"]: e.get("reason", "") for e in meta.get("excluded", []) if "adr" in e}

    missing: list[str] = []   # exit 1
    orphan: list[str] = []    # exit 2
    expired: list[str] = []   # exit 3
    pending: list[str] = []   # soft (reported, no fail)
    feeder: list[dict] = []

    today = datetime.date.today().isoformat()  # ISO strings sort lexicographically
    seen_ids: set[str] = set()
    covered_adrs: set[str] = set()

    for row in data.get("obligation", []):
        rid = row.get("id", "<no-id>")
        if rid in seen_ids:
            missing.append(f"{rid}: duplicate id")
        seen_ids.add(rid)

        adr = row.get("adr")
        if not adr:
            missing.append(f"{rid}: missing 'adr'")
        elif adr not in on_disk:
            orphan.append(f"{rid}: references {adr} but no decisions/{adr}-*.md exists")
        else:
            covered_adrs.add(adr)

        if row.get("kind") not in VALID_KIND:
            missing.append(f"{rid}: kind '{row.get('kind')}' not in {sorted(VALID_KIND)}")
        if not row.get("statement"):
            missing.append(f"{rid}: missing 'statement'")

        status = row.get("status")
        satisfied = False
        citation = None
        if status == "satisfied":
            sb = row.get("satisfied_by")
            paths = [sb] if isinstance(sb, str) else (sb or [])
            if not paths:
                missing.append(f"{rid}: status=satisfied requires 'satisfied_by'")
            else:
                unresolved = [p for p in paths if not resolves(p)]
                if unresolved:
                    missing.append(f"{rid}: satisfied_by does not resolve: {unresolved}")
                else:
                    satisfied = True
            citation = sb
        elif status == "pending":
            if not row.get("tracking"):
                missing.append(f"{rid}: status=pending requires 'tracking'")
            else:
                pending.append(f"{rid}: {row.get('tracking')}")
            citation = row.get("tracking")
        elif status == "exempt":
            until = row.get("exempt_until")
            if not until or not row.get("exempt_reason"):
                missing.append(f"{rid}: status=exempt requires 'exempt_until' + 'exempt_reason'")
            elif today > until:
                expired.append(f"{rid}: exemption expired {until} ({row.get('exempt_reason')})")
            citation = f"exempt until {until}"
        else:
            missing.append(f"{rid}: status '{status}' not in {sorted(VALID_STATUS)}")

        feeder.append({
            "dimension": "adr-obligation",
            "obligation": rid,
            "satisfied": satisfied,
            "citation": citation,
            "exempt_until": row.get("exempt_until"),
        })

    # Coverage teeth: every in-force ADR must be represented.
    for adr in sorted(on_disk):
        if adr in excluded or adr in covered_adrs:
            continue
        missing.append(f"{adr}: no obligation row references this ADR — something fell out (add a row or exclude it in [meta])")

    # Orphan excluded entries.
    for adr in sorted(excluded):
        if adr not in on_disk:
            orphan.append(f"[meta].excluded {adr}: no such ADR file")

    # ---- report (diagnostics to stderr; stdout is reserved for --json) ----
    def say(s: str) -> None:
        print(s, file=sys.stderr)

    total = len(data.get("obligation", []))
    sat = sum(1 for f in feeder if f["satisfied"])
    say(f"ADR obligations: {total} rows · {sat} satisfied · {len(pending)} pending "
        f"· {len(on_disk) - len(excluded)} in-force ADRs covered")
    for label, items in (("EXPIRED EXEMPTION", expired), ("ORPHAN", orphan), ("UNSATISFIED", missing)):
        for it in items:
            say(f"  ✗ {label}: {it}")
    if pending:
        say("  pending (tracked, not a failure):")
        for it in pending:
            say(f"    · {it}")

    if args.json:
        out = json.dumps(feeder, indent=2)
        if args.json == "-":
            print(out)
        else:
            Path(args.json).write_text(out + "\n", encoding="utf-8")

    if expired:
        return 3
    if orphan:
        return 2
    if missing:
        return 1
    say("OK — nothing fell out of the ADRs.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
