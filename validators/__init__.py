"""Shared registry validators.

One Python module, two execution sites — the same rules apply in CI
(`python -m validators registry.csv`) and in the browser (loaded into
Pyodide by the FE). See ADR-013 §"Shared validation".

Pure stdlib. No external dependencies — Pyodide weight stays predictable
and the browser-side import path doesn't have to resolve a wheel tree.
"""
from __future__ import annotations

from .rules import (
    ALPHABET,
    ID_LENGTH,
    ID_REGEX,
    REGISTRY_FIELDS,
    REQUIRED_PER_STATUS,
    STATUS_VALUES,
    Violation,
    parse_csv,
    validate_all,
    validate_diff,
    validate_row,
    validate_sort_stability,
    validate_status_transitions,
    validate_uniqueness,
)

__all__ = [
    "ALPHABET",
    "ID_LENGTH",
    "ID_REGEX",
    "REGISTRY_FIELDS",
    "REQUIRED_PER_STATUS",
    "STATUS_VALUES",
    "Violation",
    "parse_csv",
    "validate_all",
    "validate_diff",
    "validate_row",
    "validate_sort_stability",
    "validate_status_transitions",
    "validate_uniqueness",
]
