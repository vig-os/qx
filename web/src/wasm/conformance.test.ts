// FE arm of the ADR-039 §4 conformance triplet (native / wasm / FE).
//
// Drives the SHARED conformance corpus (`conformance/cases.json` +
// `conformance/contract.json`) through the SHIPPED wasm bundle — the exact
// `crates/wasm` artifact the FE loads in production — and asserts the same
// verdicts the native `qx check` gate produces. This closes
// native⇄wasm⇄FE parity: the SSOT validator is the one Rust engine, and
// every arm agrees case-for-case.
//
// The wasm module is preloaded by `src/wasm/test-setup.ts`.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { validateRecordSync } from "./loader";

const HERE = dirname(fileURLToPath(import.meta.url));
const CONFORMANCE = resolve(HERE, "../../../conformance");

interface Expect {
  path: string;
  severity: "error" | "warn";
  contains: string;
}
interface Case {
  name: string;
  collection: string;
  status?: string;
  record: Record<string, unknown>;
  known_ids?: Record<string, string[]>;
  expect: Expect[];
}

const contractBytes = new Uint8Array(
  readFileSync(resolve(CONFORMANCE, "contract.json")),
);
const corpus = JSON.parse(
  readFileSync(resolve(CONFORMANCE, "cases.json"), "utf8"),
) as { cases: Case[] };

const enc = new TextEncoder();

describe("conformance corpus through the shipped wasm (ADR-039 §4 FE arm)", () => {
  it("has cases", () => {
    expect(corpus.cases.length).toBeGreaterThan(0);
  });

  for (const c of corpus.cases) {
    it(`${c.name}`, () => {
      const issues = validateRecordSync(
        contractBytes,
        enc.encode(JSON.stringify(c.record)),
        c.collection,
        c.status,
        enc.encode(JSON.stringify(c.known_ids ?? {})),
      );

      // Each expectation must match exactly one issue; nothing left over —
      // the SAME matching the native runner enforces.
      const remaining = [...issues];
      const unmatched: string[] = [];
      for (const e of c.expect) {
        const idx = remaining.findIndex(
          (i) =>
            i.path === e.path &&
            i.severity === e.severity &&
            i.message.includes(e.contains),
        );
        if (idx === -1) {
          unmatched.push(`${e.path} / ${e.severity} / contains "${e.contains}"`);
        } else {
          remaining.splice(idx, 1);
        }
      }
      expect(
        { unmatched, unexpected: remaining },
        `case ${c.name}`,
      ).toEqual({ unmatched: [], unexpected: [] });
    });
  }
});
