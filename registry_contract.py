"""Shared machine-readable registry contract loader.

This is the runtime SSOT for field order, statuses, and canonical ID
rules. Python tooling and the web app both consume the same JSON file
under `schema/` so schema edits don't require hand-synchronizing magic
lists across the repo.
"""
from __future__ import annotations

import json
from functools import lru_cache
from pathlib import Path
from typing import Any

_CONTRACT_PATH = Path(__file__).resolve().parent / "schema" / "registry-contract.json"


@lru_cache(maxsize=1)
def load_registry_contract() -> dict[str, Any]:
    return json.loads(_CONTRACT_PATH.read_text(encoding="utf-8"))


CONTRACT = load_registry_contract()
REGISTRY_FIELDS: list[str] = [field["key"] for field in CONTRACT["fields"]]
ALPHABET: str = CONTRACT["id"]["alphabet"]
ID_LENGTH: int = int(CONTRACT["id"]["canonicalLength"])
LEGACY_ID_LENGTH: int = int(CONTRACT["id"].get("legacyCanonicalLength", 0))
HUMAN_LENGTH: int = int(CONTRACT["id"]["prefixLength"])
STATUS_VALUES: tuple[str, ...] = tuple(CONTRACT["statuses"])
