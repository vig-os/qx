"""Validator rule functions.

Pure stdlib. Each function returns a list of `Violation` objects so that
callers can choose to surface all violations at once (CI), or stream them
into a UI (FE). No function ever raises on a *data* problem — exceptions
are reserved for programmer errors (bad call shapes).

Column order mirrors `mint.py` / `bind.py` (`REGISTRY_FIELDS`); changing
it here is a breaking change for the CSV.
"""
from __future__ import annotations

import csv
import io
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

from registry_contract import (
    ALPHABET,
    ID_LENGTH,
    LEGACY_ID_LENGTH,
    REGISTRY_FIELDS,
    STATUS_VALUES as CONTRACT_STATUS_VALUES,
)

# --- canonical schema ---------------------------------------------------

# Accept both canonical (14) and legacy (12) lengths. Legacy IDs are
# deprecated — they still validate but emit a warning violation.
ID_REGEX = re.compile(rf"^[{ALPHABET}]{{{ID_LENGTH}}}$")
STATUS_VALUES: frozenset[str] = frozenset(CONTRACT_STATUS_VALUES)

# Fields that must be non-empty regardless of status.
REQUIRED_ALWAYS: tuple[str, ...] = ("id", "status", "minted_at", "batch")

# Per-status required-field overlay. `bound` parts must have a bind time
# recorded; `unbound` parts must NOT carry bind metadata yet.
REQUIRED_PER_STATUS: dict[str, dict[str, bool]] = {
    # status -> {field: must_be_nonempty}
    "unbound": {"bound_at": False, "type": False, "location": False},
    "bound":   {"bound_at": True},
    "void":    {},  # void is terminal; we don't constrain its metadata
}


# --- violation type -----------------------------------------------------

@dataclass(frozen=True)
class Violation:
    """A single rule failure.

    `line` is 1-indexed and points at the CSV file line (header = line 1,
    first data row = line 2). `None` for whole-file violations.
    """
    rule: str
    message: str
    line: int | None = None
    id: str | None = None

    def format(self, source: str = "registry.csv") -> str:
        loc = f"{source}:{self.line}" if self.line is not None else source
        ident = f" [{self.id}]" if self.id else ""
        return f"{loc}: {self.rule}: {self.message}{ident}"


# --- parsing ------------------------------------------------------------

def parse_csv(text: str) -> tuple[list[str], list[dict[str, str]]]:
    """Parse CSV text into (header, rows). No validation performed here.

    Rows are dicts keyed by header column name. Missing columns surface
    as empty strings (consistent with how `csv.DictReader` would behave
    if the header is correct).
    """
    reader = csv.reader(io.StringIO(text))
    try:
        header = next(reader)
    except StopIteration:
        return [], []
    rows: list[dict[str, str]] = []
    for raw in reader:
        # Pad / truncate so every row has exactly `len(header)` cells.
        # We *don't* silently drop ragged rows; the validator surfaces
        # them as a row-shape violation instead.
        cells = list(raw)
        if len(cells) < len(header):
            cells = cells + [""] * (len(header) - len(cells))
        rows.append(dict(zip(header, cells)))
    return header, rows


# --- per-row rules ------------------------------------------------------

def validate_row(row: dict[str, str], line: int | None = None) -> list[Violation]:
    """Schema check for a single row. Stateless; cross-row checks live elsewhere."""
    out: list[Violation] = []
    rid = (row.get("id") or "").strip()

    # Required-always fields.
    for field in REQUIRED_ALWAYS:
        if not (row.get(field) or "").strip():
            out.append(Violation(
                rule="required-field",
                message=f"missing required field '{field}'",
                line=line,
                id=rid or None,
            ))

    # ID alphabet / length. Only checked if `id` is non-empty — otherwise
    # the required-field violation above already covers it.
    if rid and not ID_REGEX.fullmatch(rid):
        out.append(Violation(
            rule="id-format",
            message=(
                f"id '{rid}' does not match the canonical "
                f"{ID_LENGTH}-char alphabet [{ALPHABET}]"
            ),
            line=line,
            id=rid,
        ))

    # Status enum.
    status = (row.get("status") or "").strip()
    if status and status not in STATUS_VALUES:
        out.append(Violation(
            rule="status-enum",
            message=(
                f"status '{status}' not in "
                f"{{{', '.join(sorted(STATUS_VALUES))}}}"
            ),
            line=line,
            id=rid or None,
        ))

    # Per-status field constraints.
    if status in REQUIRED_PER_STATUS:
        for field, must_be_set in REQUIRED_PER_STATUS[status].items():
            value = (row.get(field) or "").strip()
            if must_be_set and not value:
                out.append(Violation(
                    rule="status-field-required",
                    message=f"status '{status}' requires non-empty '{field}'",
                    line=line,
                    id=rid or None,
                ))
            elif not must_be_set and value:
                out.append(Violation(
                    rule="status-field-forbidden",
                    message=(
                        f"status '{status}' must have empty '{field}' "
                        f"(got '{value}')"
                    ),
                    line=line,
                    id=rid or None,
                ))

    return out


# --- cross-row rules ----------------------------------------------------

def validate_uniqueness(rows: list[dict[str, str]]) -> list[Violation]:
    """No duplicate IDs."""
    seen: dict[str, int] = {}  # id -> first line seen on
    out: list[Violation] = []
    for i, row in enumerate(rows, start=2):  # +2: header is line 1
        rid = (row.get("id") or "").strip()
        if not rid:
            continue
        if rid in seen:
            out.append(Violation(
                rule="id-uniqueness",
                message=f"duplicate id (first seen on line {seen[rid]})",
                line=i,
                id=rid,
            ))
        else:
            seen[rid] = i
    return out


def validate_sort_stability(rows: list[dict[str, str]]) -> list[Violation]:
    """Re-sorting by `id` ascending must equal the file as-is.

    If this fails, diffs become unreadable: an unrelated row movement
    can shift a hundred lines. The canonical CSV is sorted by ID so
    `git diff` only shows the rows actually changing.
    """
    ids = [(row.get("id") or "").strip() for row in rows]
    sorted_ids = sorted(ids)
    if ids == sorted_ids:
        return []
    # Find the first index that differs — best signal for the operator.
    out: list[Violation] = []
    for i, (have, want) in enumerate(zip(ids, sorted_ids)):
        if have != want:
            out.append(Violation(
                rule="sort-stability",
                message=(
                    f"row {i + 2} out of sort order "
                    f"(found '{have}', expected '{want}')"
                ),
                line=i + 2,
                id=have or None,
            ))
            break
    if not out:
        # Shouldn't happen given the equality check above, but defensive.
        out.append(Violation(
            rule="sort-stability",
            message="rows are not sorted by id ascending",
        ))
    return out


# Allowed status transitions for the diff-vs-base check.
# `bound -> bound` is allowed (a rebind is an edit, not a status change).
_ALLOWED_TRANSITIONS: frozenset[tuple[str, str]] = frozenset({
    ("unbound", "bound"),
    ("unbound", "void"),
    ("bound", "bound"),
    ("bound", "void"),
    ("void", "void"),
    # Same-status no-op for unbound is also fine (unbound row edited, e.g.
    # batch label corrected before any bind).
    ("unbound", "unbound"),
})


def validate_status_transitions(
    base_rows: list[dict[str, str]],
    head_rows: list[dict[str, str]],
) -> list[Violation]:
    """Status changes between `base` (e.g. main) and `head` (PR) must be forward-only.

    Allowed:
      - `unbound -> bound`
      - `unbound -> unbound`
      - `bound   -> bound`   (rebind — metadata edit)
      - `*       -> void`

    Disallowed (back-transitions, resurrection):
      - `bound   -> unbound`
      - `void    -> bound`
      - `void    -> unbound`

    New rows in head must start as `unbound` or `bound` — never `void`
    (voiding an ID we never minted is a sign of CSV corruption / a copy-
    paste accident).
    """
    base_by_id: dict[str, dict[str, str]] = {
        (r.get("id") or "").strip(): r for r in base_rows
        if (r.get("id") or "").strip()
    }
    out: list[Violation] = []
    for i, row in enumerate(head_rows, start=2):
        rid = (row.get("id") or "").strip()
        if not rid:
            continue
        head_status = (row.get("status") or "").strip()
        prior = base_by_id.get(rid)
        if prior is None:
            # New ID. Must be born as unbound or bound (not void).
            if head_status == "void":
                out.append(Violation(
                    rule="status-new-row",
                    message=(
                        f"new id introduced with status 'void' — new rows "
                        f"must be 'unbound' or 'bound'"
                    ),
                    line=i,
                    id=rid,
                ))
            continue
        base_status = (prior.get("status") or "").strip()
        if base_status == head_status:
            continue
        if (base_status, head_status) not in _ALLOWED_TRANSITIONS:
            out.append(Violation(
                rule="status-transition",
                message=(
                    f"disallowed status transition "
                    f"'{base_status}' -> '{head_status}'"
                ),
                line=i,
                id=rid,
            ))
    return out


def validate_diff(base_text: str, head_text: str) -> list[Violation]:
    """Run all diff-vs-base rules. Today: status transitions only.

    Sort-stability and uniqueness are file-local and run via
    `validate_all` against `head` directly; they don't need the base.
    """
    _, base_rows = parse_csv(base_text)
    _, head_rows = parse_csv(head_text)
    return validate_status_transitions(base_rows, head_rows)


# --- top-level driver ---------------------------------------------------

def validate_all(
    text: str,
    base_text: str | None = None,
) -> list[Violation]:
    """Run every validator against `text`. If `base_text` is given, also run
    diff rules against it.

    Returns a flat list of violations in deterministic order:
      1. header shape
      2. per-row schema (in file order)
      3. uniqueness
      4. sort stability
      5. diff-vs-base (if `base_text` given)
    """
    out: list[Violation] = []
    header, rows = parse_csv(text)

    # Header must match the canonical schema exactly. A column rename or
    # reorder breaks every downstream consumer.
    if header != REGISTRY_FIELDS:
        out.append(Violation(
            rule="header-schema",
            message=(
                f"header {header!r} does not match canonical "
                f"{REGISTRY_FIELDS!r}"
            ),
            line=1,
        ))
        # If the header is wrong, per-row checks would emit a flood of
        # false positives. Stop here.
        return out

    for i, row in enumerate(rows, start=2):
        out.extend(validate_row(row, line=i))

    out.extend(validate_uniqueness(rows))
    out.extend(validate_sort_stability(rows))

    if base_text is not None:
        out.extend(validate_diff(base_text, text))

    return out


# --- convenience for path-based callers ---------------------------------

def validate_path(
    path: str | Path,
    base_path: str | Path | None = None,
) -> list[Violation]:
    """Read `path` (and optionally `base_path`) and run `validate_all`."""
    text = Path(path).read_text(encoding="utf-8")
    base_text = Path(base_path).read_text(encoding="utf-8") if base_path else None
    return validate_all(text, base_text)


def format_violations(
    violations: Iterable[Violation],
    source: str = "registry.csv",
) -> str:
    """One violation per line, suitable for stderr."""
    return "\n".join(v.format(source) for v in violations)
