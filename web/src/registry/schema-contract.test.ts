import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { ID_ALPHABET, ID_LENGTH } from "../config";
import { REGISTRY_FIELD_KEYS, STATUSES } from "./schema";
import contract from "@registry-contract";

function readRegistryHeader(): string[] {
  // Per #35: the live registry.csv lives in the data repo
  // (`exo-pet/exopet-registry[-sandbox]`), not this code repo. We
  // assert the schema contract against a committed header fixture so
  // the test stays deterministic and the data repo stays the single
  // source of truth for runtime row contents.
  const csv = readFileSync(
    resolve(import.meta.dirname, "../../test-fixtures/registry-header.csv"),
    "utf-8",
  );
  return csv.split(/\r?\n/, 1)[0].split(",");
}

describe("registry contract", () => {
  it("matches the CSV header order", () => {
    expect(readRegistryHeader()).toEqual(REGISTRY_FIELD_KEYS);
  });

  it("matches the web schema field order", () => {
    expect(REGISTRY_FIELD_KEYS).toEqual(contract.fields.map((field: { key: string }) => field.key));
  });

  it("matches the web status enum", () => {
    expect([...STATUSES]).toEqual(contract.statuses);
  });

  it("matches the canonical ID constants", () => {
    expect(ID_ALPHABET).toBe(contract.id.alphabet);
    expect(ID_LENGTH).toBe(contract.id.canonicalLength);
  });

  it("has schema_version 1", () => {
    expect(contract.schema_version).toBe(1);
  });

  it("every field has a type", () => {
    for (const field of contract.fields) {
      expect(field.type).toBeDefined();
      expect(["string", "dropdown", "yes-no", "date", "number", "json"]).toContain(field.type);
    }
  });
});
