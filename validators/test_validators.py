"""Pytest suite for the shared registry validators.

Each test pairs a fixture CSV with the rule that should (or shouldn't)
fire on it. Adding a new rule should mean adding a new fixture + test,
not modifying existing tests.
"""
from __future__ import annotations

from pathlib import Path

import pytest

from validators import (
    REGISTRY_FIELDS,
    Violation,
    parse_csv,
    validate_all,
    validate_diff,
    validate_row,
    validate_sort_stability,
    validate_status_transitions,
    validate_uniqueness,
)
from validators.__main__ import main as cli_main

FIXTURES = Path(__file__).parent / "fixtures"


def _load(name: str) -> str:
    return (FIXTURES / name).read_text(encoding="utf-8")


def _rules(violations: list[Violation]) -> set[str]:
    return {v.rule for v in violations}


# --- file-local: clean fixtures ----------------------------------------

def test_valid_empty_passes():
    assert validate_all(_load("valid_empty.csv")) == []


def test_valid_mixed_passes():
    assert validate_all(_load("valid_mixed.csv")) == []


# --- per-row: schema ----------------------------------------------------

def test_bad_id_format_fires_id_format():
    violations = validate_all(_load("bad_id_format.csv"))
    assert "id-format" in _rules(violations)


def test_bad_id_length_fires_id_format():
    violations = validate_all(_load("bad_id_length.csv"))
    assert "id-format" in _rules(violations)


def test_bad_status_enum_fires_status_enum():
    violations = validate_all(_load("bad_status_enum.csv"))
    assert "status-enum" in _rules(violations)


def test_bound_without_bound_at_fires():
    violations = validate_all(_load("bad_bound_missing_bound_at.csv"))
    assert "status-field-required" in _rules(violations)


def test_unbound_with_type_fires():
    violations = validate_all(_load("bad_unbound_with_type.csv"))
    assert "status-field-forbidden" in _rules(violations)


def test_required_field_missing():
    # Manually construct: missing batch.
    text = (
        "id,status,minted_at,batch,bound_at,type,description,vendor,"
        "part_number,location,notes\n"
        "2A3B4C5D6E7F,unbound,2026-05-01T10:00:00+00:00,,,,,,,,\n"
    )
    violations = validate_all(text)
    assert "required-field" in _rules(violations)


# --- header -------------------------------------------------------------

def test_bad_header_fires_and_short_circuits():
    violations = validate_all(_load("bad_header.csv"))
    rules = _rules(violations)
    assert rules == {"header-schema"}, (
        "header mismatch must short-circuit per-row checks to avoid "
        "drowning the operator in cascading violations"
    )


# --- cross-row: uniqueness, sort ----------------------------------------

def test_bad_unsorted_fires_sort_stability():
    violations = validate_all(_load("bad_unsorted.csv"))
    assert "sort-stability" in _rules(violations)


def test_bad_duplicate_fires_uniqueness():
    violations = validate_all(_load("bad_duplicate.csv"))
    assert "id-uniqueness" in _rules(violations)


def test_uniqueness_unit():
    rows = [
        {"id": "2A3B4C5D6E7F"},
        {"id": "2A3B4C5D6E7F"},
    ]
    assert _rules(validate_uniqueness(rows)) == {"id-uniqueness"}


def test_sort_stability_unit():
    rows = [{"id": "Z23456789ABC"}, {"id": "2A3B4C5D6E7F"}]
    assert _rules(validate_sort_stability(rows)) == {"sort-stability"}


def test_validate_row_unit():
    row = {
        "id": "2A3B4C5D6E7F",
        "status": "bound",
        "minted_at": "2026-05-01T10:00:00+00:00",
        "batch": "B-2026-05-test",
        "bound_at": "2026-05-02T11:00:00+00:00",
        "type": "PT100",
        "description": "",
        "vendor": "",
        "part_number": "",
        "location": "",
        "notes": "",
    }
    assert validate_row(row, line=2) == []


# --- diff vs base -------------------------------------------------------

def test_diff_void_to_bound_disallowed():
    base = _load("base_for_diff.csv")
    head = _load("head_bad_void_to_bound.csv")
    violations = validate_diff(base, head)
    rules = _rules(violations)
    assert "status-transition" in rules


def test_diff_bound_to_unbound_disallowed():
    base = _load("base_for_diff.csv")
    head = _load("head_bad_bound_to_unbound.csv")
    violations = validate_diff(base, head)
    assert "status-transition" in _rules(violations)


def test_diff_good_progression_passes():
    base = _load("base_for_diff.csv")
    head = _load("head_good_bind_progression.csv")
    assert validate_diff(base, head) == []


def test_diff_new_row_as_void_disallowed():
    base = _load("base_for_diff.csv")
    head = _load("head_bad_new_void.csv")
    violations = validate_diff(base, head)
    assert "status-new-row" in _rules(violations)


def test_status_transitions_unit_allowed():
    base = [{"id": "2A3B4C5D6E7F", "status": "unbound"}]
    head = [{"id": "2A3B4C5D6E7F", "status": "bound"}]
    assert validate_status_transitions(base, head) == []


def test_status_transitions_unit_rebind_allowed():
    # bound -> bound is the rebind path — must not be flagged.
    base = [{"id": "2A3B4C5D6E7F", "status": "bound"}]
    head = [{"id": "2A3B4C5D6E7F", "status": "bound"}]
    assert validate_status_transitions(base, head) == []


def test_status_transitions_unit_void_to_bound_blocked():
    base = [{"id": "2A3B4C5D6E7F", "status": "void"}]
    head = [{"id": "2A3B4C5D6E7F", "status": "bound"}]
    assert _rules(validate_status_transitions(base, head)) == {"status-transition"}


# --- parse_csv shape ----------------------------------------------------

def test_parse_csv_header_only():
    header, rows = parse_csv("id,status\n")
    assert header == ["id", "status"]
    assert rows == []


def test_parse_csv_pads_short_rows():
    header, rows = parse_csv("id,status,minted_at\n2A3B4C5D6E7F,bound\n")
    assert rows == [{"id": "2A3B4C5D6E7F", "status": "bound", "minted_at": ""}]


# --- canonical schema sanity check --------------------------------------

def test_registry_fields_match_mint():
    # If REGISTRY_FIELDS in mint.py / bind.py is updated, this test should
    # be updated in lockstep — it's the contract that keeps writers and
    # validators aligned.
    assert REGISTRY_FIELDS == [
        "id", "status", "minted_at", "batch", "bound_at",
        "type", "description", "vendor", "part_number", "location", "notes",
    ]


# --- CLI ----------------------------------------------------------------

def test_cli_returns_zero_on_clean(capsys, tmp_path):
    csv_path = tmp_path / "registry.csv"
    csv_path.write_text(_load("valid_mixed.csv"))
    rc = cli_main([str(csv_path)])
    assert rc == 0
    captured = capsys.readouterr()
    assert captured.err == ""


def test_cli_returns_one_on_violation(capsys, tmp_path):
    csv_path = tmp_path / "registry.csv"
    csv_path.write_text(_load("bad_unsorted.csv"))
    rc = cli_main([str(csv_path)])
    assert rc == 1
    captured = capsys.readouterr()
    assert "sort-stability" in captured.err


def test_cli_with_base_flag(capsys, tmp_path):
    base_path = tmp_path / "base.csv"
    head_path = tmp_path / "head.csv"
    base_path.write_text(_load("base_for_diff.csv"))
    head_path.write_text(_load("head_bad_void_to_bound.csv"))
    rc = cli_main([str(head_path), "--base", str(base_path)])
    assert rc == 1
    captured = capsys.readouterr()
    assert "status-transition" in captured.err


def test_cli_returns_two_on_missing_file(capsys, tmp_path):
    rc = cli_main([str(tmp_path / "nope.csv")])
    assert rc == 2


def test_cli_against_real_registry(capsys):
    # The committed registry.csv must always validate clean — if this
    # fails locally, you've got a registry rule violation in main.
    repo_root = Path(__file__).resolve().parent.parent
    rc = cli_main([str(repo_root / "registry.csv")])
    assert rc == 0


# --- integration: every fixture exercises at least one rule -------------

@pytest.mark.parametrize(
    "fixture,expected_rule",
    [
        ("bad_id_format.csv", "id-format"),
        ("bad_id_length.csv", "id-format"),
        ("bad_status_enum.csv", "status-enum"),
        ("bad_bound_missing_bound_at.csv", "status-field-required"),
        ("bad_unbound_with_type.csv", "status-field-forbidden"),
        ("bad_unsorted.csv", "sort-stability"),
        ("bad_duplicate.csv", "id-uniqueness"),
        ("bad_header.csv", "header-schema"),
    ],
)
def test_each_bad_fixture_fires_its_rule(fixture, expected_rule):
    violations = validate_all(_load(fixture))
    assert expected_rule in _rules(violations), (
        f"{fixture} should fire {expected_rule}; got {_rules(violations)}"
    )
