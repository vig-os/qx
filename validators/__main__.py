"""CLI entry point: `python -m validators registry.csv`.

Exit code 0 = clean, 1 = at least one violation. One violation per line
on stderr in `file:line: rule: message` form so editors and CI logs can
jump to the offending row.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .rules import format_violations, validate_path


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        prog="python -m validators",
        description=(
            "Validate a registry.csv against the shared rule set "
            "(ADR-013 §Shared validation). Exits non-zero on any violation."
        ),
    )
    ap.add_argument(
        "csv_path",
        type=Path,
        help="path to the registry CSV to validate (typically registry.csv)",
    )
    ap.add_argument(
        "--base",
        type=Path,
        default=None,
        help=(
            "path to the base (e.g. main-branch) copy of registry.csv. "
            "When given, also enforces diff rules — most notably status "
            "transitions. Skipped if omitted."
        ),
    )
    args = ap.parse_args(argv)

    if not args.csv_path.exists():
        print(f"validators: no such file: {args.csv_path}", file=sys.stderr)
        return 2
    if args.base is not None and not args.base.exists():
        print(f"validators: --base file not found: {args.base}", file=sys.stderr)
        return 2

    violations = validate_path(args.csv_path, args.base)
    if not violations:
        return 0

    print(format_violations(violations, source=str(args.csv_path)), file=sys.stderr)
    print(
        f"validators: {len(violations)} violation(s) in {args.csv_path}",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
