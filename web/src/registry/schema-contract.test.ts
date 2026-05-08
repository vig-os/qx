import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { ID_ALPHABET, ID_LENGTH } from "../config";
import { REGISTRY_FIELD_KEYS, STATUSES } from "./schema";
import contract from "@registry-contract";

function readRegistryHeader(): string[] {
  const csv = readFileSync(
    resolve(import.meta.dirname, "../../../registry.csv"),
    "utf-8",
  );
  return csv.split(/\r?\n/, 1)[0].split(",");
}

describe("registry contract", () => {
  it("matches the CSV header order", () => {
    expect(readRegistryHeader()).toEqual(REGISTRY_FIELD_KEYS);
  });

  it("matches the web schema field order", () => {
    expect(REGISTRY_FIELD_KEYS).toEqual(contract.fields.map((field) => field.key));
  });

  it("matches the web status enum", () => {
    expect([...STATUSES]).toEqual(contract.statuses);
  });

  it("matches the canonical ID constants", () => {
    expect(ID_ALPHABET).toBe(contract.id.alphabet);
    expect(ID_LENGTH).toBe(contract.id.canonicalLength);
  });
});
