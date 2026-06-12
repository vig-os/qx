// Preflight local-issue checks (#176/#168) — exercises the FE-local
// detection (unknown id, duplicates, assembly component validation).
// The WASM policy decision is try/caught inside runPreflight and
// defaults to "allow" when unavailable, so these run in jsdom.

import { describe, it, expect } from "vitest";
import { runPreflight, type QueueItem } from "./preflight";
import type { RegistryRow } from "./schema";

function reg(rows: Array<Partial<RegistryRow> & { id: string }>): Map<string, RegistryRow> {
  const m = new Map<string, RegistryRow>();
  for (const r of rows) m.set(r.id, { status: "unbound", ...r } as RegistryRow);
  return m;
}

function bind(id: string, fields: Record<string, string> = {}): QueueItem {
  return { id, kind: "bind", fields };
}

const kinds = (r: ReturnType<typeof runPreflight>) => r.localIssues.map((i) => i.kind);

describe("runPreflight — id checks", () => {
  it("flags a bind whose id is not in the registry", () => {
    const r = runPreflight([bind("AAAAAAAAAAAAAA")], reg([]));
    expect(kinds(r)).toContain("unknown_id");
  });

  it("does not flag a bind whose id IS in the registry (incl. session mints)", () => {
    const r = runPreflight([bind("AAAAAAAAAAAAAA")], reg([{ id: "AAAAAAAAAAAAAA" }]));
    expect(kinds(r)).not.toContain("unknown_id");
  });

  it("flags a duplicate id in the queue", () => {
    const registry = reg([{ id: "AAAAAAAAAAAAAA" }]);
    const r = runPreflight([bind("AAAAAAAAAAAAAA"), bind("AAAAAAAAAAAAAA")], registry);
    expect(kinds(r)).toContain("duplicate_in_queue");
  });
});

describe("runPreflight — assembly component checks (#176 merge mint+bind)", () => {
  const PARENT = "PARENTAAAAAAAA";
  const C1 = "3456ABCDEFGHJK";
  const C2 = "56789ABCDEFGHJ";

  it("passes when all components exist and are non-void", () => {
    const registry = reg([{ id: PARENT }, { id: C1, status: "bound" }, { id: C2, status: "bound" }]);
    const r = runPreflight([bind(PARENT, { components: `${C1};${C2}` })], registry);
    expect(kinds(r)).not.toContain("void_component");
    expect(kinds(r)).not.toContain("unknown_component");
  });

  it("treats a component that's a same-session mint as known", () => {
    // PARENT and C1 are both freshly minted this session (in the map);
    // combining them must not flag unknown_component.
    const registry = reg([{ id: PARENT }, { id: C1 }]);
    const r = runPreflight([bind(PARENT, { components: C1 })], registry);
    expect(kinds(r)).not.toContain("unknown_component");
  });

  it("flags a voided component", () => {
    const registry = reg([{ id: PARENT }, { id: C1, status: "void" }]);
    const r = runPreflight([bind(PARENT, { components: C1 })], registry);
    expect(kinds(r)).toContain("void_component");
  });

  it("flags an unknown component", () => {
    const registry = reg([{ id: PARENT }]);
    const r = runPreflight([bind(PARENT, { components: "ZZZZZZZZZZZZZZ" })], registry);
    expect(kinds(r)).toContain("unknown_component");
  });

  it("flags a self-referential component", () => {
    const registry = reg([{ id: PARENT }]);
    const r = runPreflight([bind(PARENT, { components: PARENT })], registry);
    expect(kinds(r)).toContain("self_component");
  });

  it("ignores an empty components field", () => {
    const registry = reg([{ id: PARENT }]);
    const r = runPreflight([bind(PARENT, { components: "" })], registry);
    expect(kinds(r)).not.toContain("unknown_component");
    expect(kinds(r)).not.toContain("void_component");
  });
});
