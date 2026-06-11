import { describe, expect, it } from "vitest";
import type {
  CountData,
  CreateData,
  DescribeData,
  EditData,
  Entity,
  ExportData,
  ListData,
  PollProposalData,
  PrintData,
  Request,
  Response,
  TransitionData,
  WhoamiData,
} from "../protocol";
import { mockTransport } from "./mock";
import { partsDescribe, partsEntities, type Fixtures } from "./fixtures";

function expectOk<T>(res: Response): T {
  if (!res.ok) throw new Error(`expected ok, got ${res.error.kind}: ${res.error.message}`);
  return res.data as T;
}

function expectErr(res: Response): { kind: string; message: string } {
  if (res.ok) throw new Error(`expected error, got ok: ${JSON.stringify(res.data)}`);
  return res.error;
}

const list = (req: Partial<Extract<Request, { op: "List" }>>): Request => ({
  op: "List",
  collection: "parts",
  ...req,
});

describe("mockTransport List", () => {
  it("returns all entities with total when unfiltered", async () => {
    const t = mockTransport();
    const data = expectOk<ListData>(await t(list({})));
    expect(data.items).toHaveLength(partsEntities.length);
    expect(data.total).toBe(partsEntities.length);
  });

  it("filters by status", async () => {
    const t = mockTransport();
    const data = expectOk<ListData>(await t(list({ filter: { status: "bound" } })));
    expect(data.total).toBe(4);
    expect(data.items.every((e) => e.status === "bound")).toBe(true);
  });

  it("filters by free text, case-insensitively, across field values", async () => {
    const t = mockTransport();
    const data = expectOk<ListData>(await t(list({ filter: { text: "omega" } })));
    expect(data.total).toBe(2);
    expect(data.items.map((e) => e.fields["vendor"])).toEqual(["Omega", "Omega"]);
  });

  it("filters per-field as a case-insensitive substring (engine apply_filter)", async () => {
    const t = mockTransport();
    const exact = expectOk<ListData>(await t(list({ filter: { fields: { vendor: "Lapp" } } })));
    expect(exact.total).toBe(2);
    const substring = expectOk<ListData>(await t(list({ filter: { fields: { vendor: "lap" } } })));
    expect(substring.total).toBe(2);
    const none = expectOk<ListData>(await t(list({ filter: { fields: { vendor: "lappland" } } })));
    expect(none.total).toBe(0);
  });

  it("free-text haystack is id + field values only — not label/kind", async () => {
    const labeled: Fixtures = {
      describe: partsDescribe,
      entities: [
        {
          ...partsEntities[0]!,
          id: "AAAA2345678BCD",
          label: "needle-in-label",
          kind: "needle-in-kind",
          fields: {},
        },
      ],
    };
    const t = mockTransport(labeled);
    expect(expectOk<ListData>(await t(list({ filter: { text: "needle" } }))).total).toBe(0);
    expect(expectOk<ListData>(await t(list({ filter: { text: "aaaa2345" } }))).total).toBe(1);
  });

  it("defaults to id-ascending order when no sort is given (engine list)", async () => {
    const t = mockTransport();
    const data = expectOk<ListData>(await t(list({})));
    const ids = data.items.map((e) => e.id);
    expect(ids).toEqual([...ids].sort());
  });

  it("sorts by a declared field in both directions", async () => {
    const t = mockTransport();
    const bound = { status: "bound" };
    const asc = expectOk<ListData>(
      await t(list({ filter: bound, sort: [{ field: "vendor", dir: "asc" }] })),
    );
    expect(asc.items.map((e) => e.fields["vendor"])).toEqual(["KNF", "Lapp", "Omega", "Omega"]);
    const desc = expectOk<ListData>(
      await t(list({ filter: bound, sort: [{ field: "vendor", dir: "desc" }] })),
    );
    expect(desc.items.map((e) => e.fields["vendor"])).toEqual(["Omega", "Omega", "Lapp", "KNF"]);
  });

  it("pages with offset/limit while total reflects the filtered set", async () => {
    const t = mockTransport();
    const sort = [{ field: "id", dir: "asc" as const }];
    const all = expectOk<ListData>(await t(list({ sort })));
    const page = expectOk<ListData>(await t(list({ sort, page: { offset: 2, limit: 3 } })));
    expect(page.total).toBe(all.total);
    expect(page.items).toHaveLength(3);
    expect(page.items.map((e) => e.id)).toEqual(all.items.slice(2, 5).map((e) => e.id));
  });

  it("answers Unsupported for an undeclared collection (engine known_collection)", async () => {
    const t = mockTransport();
    const error = expectErr(await t(list({ collection: "gadgets" })));
    expect(error.kind).toBe("Unsupported");
  });
});

describe("mockTransport Count", () => {
  it("groups by status", async () => {
    const t = mockTransport();
    const data = expectOk<CountData>(await t({ op: "Count", collection: "parts", by: "status" }));
    expect(data.by).toBe("status");
    expect(data.counts).toEqual({ bound: 4, unbound: 3, void: 1 });
  });

  it("applies the filter before counting", async () => {
    const t = mockTransport();
    const data = expectOk<CountData>(
      await t({ op: "Count", collection: "parts", filter: { text: "lapp" }, by: "status" }),
    );
    expect(data.counts).toEqual({ bound: 1, void: 1 });
  });
});

describe("mockTransport Describe", () => {
  it("returns the registry name and collection descriptors", async () => {
    const t = mockTransport();
    const data = expectOk<DescribeData>(await t({ op: "Describe" }));
    expect(data.name).toBe("mock-registry");
    const parts = data.collections.find((c) => c.name === "parts");
    expect(parts).toBeDefined();
    expect(parts?.lifecycle.statuses).toEqual(["unbound", "bound", "void"]);
    // Roster + order mirror the engine's parts preset (preset.rs).
    expect(parts?.fields.map((f) => f.key)).toEqual([
      "type",
      "description",
      "vendor",
      "part_number",
      "location",
      "notes",
    ]);
  });

  it("narrows to one collection and NotFounds unknown ones", async () => {
    const t = mockTransport();
    const data = expectOk<DescribeData>(await t({ op: "Describe", collection: "parts" }));
    expect(data.collections).toHaveLength(1);
    const error = expectErr(await t({ op: "Describe", collection: "gadgets" }));
    expect(error.kind).toBe("NotFound");
  });
});

describe("mockTransport Resolve", () => {
  it("resolves a bare id and a scheme-qualified id", async () => {
    const t = mockTransport();
    const bare = expectOk<Entity>(await t({ op: "Resolve", id: "PQ7G2MNVX4KH9T" }));
    expect(bare.fields["type"]).toBe("t-sensor");
    const qualified = expectOk<Entity>(await t({ op: "Resolve", id: "nano14:PQ7G2MNVX4KH9T" }));
    expect(qualified.id).toBe(bare.id);
  });

  it("rejects unknown schemes and NotFounds unknown ids", async () => {
    const t = mockTransport();
    const scheme = expectErr(await t({ op: "Resolve", id: "bogus:PQ7G2MNVX4KH9T" }));
    expect(scheme.kind).toBe("Validation");
    expect(scheme.message).toContain("nano14");
    expect(expectErr(await t({ op: "Resolve", id: "22222222222222" })).kind).toBe("NotFound");
  });

  it("normalizes the query: dashes/spaces stripped, case-folded (engine normalize_id)", async () => {
    const t = mockTransport();
    const e = expectOk<Entity>(await t({ op: "Resolve", id: " pq7g-2mnv x4kh-9t " }));
    expect(e.id).toBe("PQ7G2MNVX4KH9T");
  });

  it("accepts a >=8-char human prefix with a unique match", async () => {
    const t = mockTransport();
    const e = expectOk<Entity>(await t({ op: "Resolve", id: "PQ7G2MNV" }));
    expect(e.id).toBe("PQ7G2MNVX4KH9T");
    const notFound = expectErr(await t({ op: "Resolve", id: "ZZZZZZZZ" }));
    expect(notFound.kind).toBe("NotFound");
  });

  it("answers Ambiguous when a prefix matches several ids", async () => {
    const twin = (id: string): Entity => ({ ...partsEntities[0]!, id });
    const t = mockTransport({
      describe: partsDescribe,
      entities: [twin("23456789ABCDEF"), twin("23456789GHJKMN")],
    });
    const error = expectErr(await t({ op: "Resolve", id: "23456789" }));
    expect(error.kind).toBe("Ambiguous");
    expect(error.message).toContain("23456789ABCDEF");
    expect(error.message).toContain("23456789GHJKMN");
  });

  it("answers BadRequest for a query under the 8-char prefix floor", async () => {
    const t = mockTransport();
    const error = expectErr(await t({ op: "Resolve", id: "PQ7G2" }));
    expect(error.kind).toBe("BadRequest");
    expect(error.message).toContain(">= 8");
  });
});

// Mutation responses carry a ProposalRef, never the updated entity
// (ADR-019: mutations are proposals). The mock still applies the
// change to its in-memory store immediately, so effects are observed
// via List/Resolve.
describe("mockTransport mutations", () => {
  it("Create returns minted ids + one created_at stamp + a proposal ref", async () => {
    const t = mockTransport();
    const created = expectOk<CreateData>(await t({ op: "Create", collection: "parts", n: 3 }));
    expect(created.minted).toHaveLength(3);
    expect(created.created_at).toBeDefined();
    expect(created.proposal.adapter).toBe("mock");
    expect(created.proposal.url).toBe(`mock://proposal/${created.proposal.local_id}`);
    // Mock applies immediately: minted ids are listable as unbound.
    const after = expectOk<ListData>(await t(list({})));
    expect(after.total).toBe(partsEntities.length + 3);
    for (const id of created.minted) {
      const e = expectOk<Entity>(await t({ op: "Resolve", id }));
      expect(e.status).toBe("unbound");
      expect(e.created_at).toBe(created.created_at);
    }
  });

  it("Transition returns {id, to, proposal}; store reflects the change", async () => {
    const t = mockTransport();
    const data = expectOk<TransitionData>(
      await t({
        op: "Transition",
        collection: "parts",
        id: "T8U3VWXY9Z2ABC",
        to: "bound",
        fields: { vendor: "Omega" },
      }),
    );
    expect(data.id).toBe("T8U3VWXY9Z2ABC");
    expect(data.to).toBe("bound");
    expect(data.proposal.adapter).toBe("mock");
    const e = expectOk<Entity>(await t({ op: "Resolve", id: "T8U3VWXY9Z2ABC" }));
    expect(e.status).toBe("bound");
    expect(e.transitioned_at["bound"]).toBeDefined();
    expect(e.fields["vendor"]).toBe("Omega");
  });

  it("Transition validates the target status and field keys", async () => {
    const t = mockTransport();
    expect(
      expectErr(
        await t({ op: "Transition", collection: "parts", id: "T8U3VWXY9Z2ABC", to: "broken" }),
      ).kind,
    ).toBe("Validation");
    expect(
      expectErr(
        await t({
          op: "Transition",
          collection: "parts",
          id: "T8U3VWXY9Z2ABC",
          to: "bound",
          fields: { wingspan: "3m" },
        }),
      ).kind,
    ).toBe("Validation");
  });

  it("Edit returns {id, proposal}, merges fields, rejects unknown keys", async () => {
    const t = mockTransport();
    const data = expectOk<EditData>(
      await t({
        op: "Edit",
        collection: "parts",
        id: "PQ7G2MNVX4KH9T",
        fields: { notes: "recalibrated" },
      }),
    );
    expect(data.id).toBe("PQ7G2MNVX4KH9T");
    expect(data.proposal.adapter).toBe("mock");
    const e = expectOk<Entity>(await t({ op: "Resolve", id: "PQ7G2MNVX4KH9T" }));
    expect(e.fields["notes"]).toBe("recalibrated");
    expect(e.fields["vendor"]).toBe("Omega");
    expect(
      expectErr(
        await t({ op: "Edit", collection: "parts", id: "PQ7G2MNVX4KH9T", fields: { nope: "x" } }),
      ).kind,
    ).toBe("Validation");
  });

  it("isolates state between transport instances", async () => {
    const a = mockTransport();
    expectOk<CreateData>(await a({ op: "Create", collection: "parts", n: 5 }));
    const b = mockTransport();
    const data = expectOk<ListData>(await b(list({})));
    expect(data.total).toBe(partsEntities.length);
  });
});

describe("mockTransport engine-fidelity rules", () => {
  it("Create defaults n to 1 and BadRequests n < 1", async () => {
    const t = mockTransport();
    const one = expectOk<CreateData>(await t({ op: "Create", collection: "parts" }));
    expect(one.minted).toHaveLength(1);
    const error = expectErr(await t({ op: "Create", collection: "parts", n: 0 }));
    expect(error.kind).toBe("BadRequest");
    expect(error.message).toContain(">= 1");
  });

  it("Create rejects a fields payload — mint-then-bind", async () => {
    const t = mockTransport();
    const error = expectErr(
      await t({ op: "Create", collection: "parts", n: 1, fields: { vendor: "Omega" } }),
    );
    expect(error.kind).toBe("Validation");
    expect(error.message).toContain("mint-then-bind");
  });

  it("Edit requires at least one field", async () => {
    const t = mockTransport();
    const error = expectErr(
      await t({ op: "Edit", collection: "parts", id: "PQ7G2MNVX4KH9T", fields: {} }),
    );
    expect(error.kind).toBe("BadRequest");
  });

  it("Edit applies to bound parts only, directing to Transition otherwise", async () => {
    const t = mockTransport();
    const unbound = expectErr(
      await t({ op: "Edit", collection: "parts", id: "T8U3VWXY9Z2ABC", fields: { notes: "x" } }),
    );
    expect(unbound.kind).toBe("Validation");
    expect(unbound.message).toContain("Transition");
    const voided = expectErr(
      await t({ op: "Edit", collection: "parts", id: "V5W8XYZ2B3CDEF", fields: { notes: "x" } }),
    );
    expect(voided.kind).toBe("Validation");
  });

  it("Edit's unknown-field error lists the descriptor's editable roster", async () => {
    const t = mockTransport();
    const error = expectErr(
      await t({ op: "Edit", collection: "parts", id: "PQ7G2MNVX4KH9T", fields: { batch: "B1" } }),
    );
    expect(error.kind).toBe("Validation");
    expect(error.message).toContain(
      "type, description, vendor, part_number, location, notes",
    );
  });

  it("Edit and Transition resolve their target like Resolve (prefix accepted)", async () => {
    const t = mockTransport();
    const edited = expectOk<EditData>(
      await t({ op: "Edit", collection: "parts", id: "PQ7G2MNV", fields: { notes: "via prefix" } }),
    );
    expect(edited.id).toBe("PQ7G2MNVX4KH9T");
    const bound = expectOk<TransitionData>(
      await t({ op: "Transition", collection: "parts", id: "T8U3-VWXY-9Z2A-BC", to: "bound" }),
    );
    expect(bound.id).toBe("T8U3VWXY9Z2ABC");
  });

  it("Transition to bound rejects already-bound parts, directing to Edit", async () => {
    const t = mockTransport();
    const error = expectErr(
      await t({ op: "Transition", collection: "parts", id: "PQ7G2MNVX4KH9T", to: "bound" }),
    );
    expect(error.kind).toBe("Validation");
    expect(error.message).toContain("Edit");
  });

  it("Transition rejects void -> bound: mint a new ID", async () => {
    const t = mockTransport();
    const error = expectErr(
      await t({ op: "Transition", collection: "parts", id: "V5W8XYZ2B3CDEF", to: "bound" }),
    );
    expect(error.kind).toBe("Validation");
    expect(error.message).toContain("Mint a new ID");
  });

  it("Transition to void stamps the request's notes with the void timestamp", async () => {
    const t = mockTransport();
    const data = expectOk<TransitionData>(
      await t({
        op: "Transition",
        collection: "parts",
        id: "PQ7G2MNVX4KH9T",
        to: "void",
        fields: { notes: "drowned" },
      }),
    );
    expect(data.to).toBe("void");
    const e = expectOk<Entity>(await t({ op: "Resolve", id: "PQ7G2MNVX4KH9T" }));
    expect(e.status).toBe("void");
    expect(e.fields["notes"]).toMatch(/^drowned \[voided .+\]$/);
  });

  it("non-Describe ops answer Unsupported for an undeclared collection", async () => {
    const t = mockTransport();
    const reqs: Request[] = [
      { op: "Count", collection: "gadgets", by: "status" },
      { op: "Create", collection: "gadgets", n: 1 },
      { op: "Edit", collection: "gadgets", id: "PQ7G2MNVX4KH9T", fields: { notes: "x" } },
      { op: "Transition", collection: "gadgets", id: "PQ7G2MNVX4KH9T", to: "bound" },
      { op: "Print", collection: "gadgets", selection: { ids: ["PQ7G2MNVX4KH9T"] } },
      { op: "Export", collection: "gadgets", format: "csv" },
    ];
    for (const req of reqs) {
      expect(expectErr(await t(req)).kind).toBe("Unsupported");
    }
    // Describe alone keeps NotFound for an unknown collection.
    expect(expectErr(await t({ op: "Describe", collection: "gadgets" })).kind).toBe("NotFound");
  });
});

describe("mockTransport Print", () => {
  it("renders one placeholder SVG per selected id", async () => {
    const t = mockTransport();
    const data = expectOk<PrintData>(
      await t({
        op: "Print",
        collection: "parts",
        selection: { ids: ["PQ7G2MNVX4KH9T", "W3JD8RST2UVKXM"] },
        options: { layout: "horz", size_mm: 8, chars: "44" },
      }),
    );
    expect(data.labels.map((l) => l.id)).toEqual(["PQ7G2MNVX4KH9T", "W3JD8RST2UVKXM"]);
    for (const label of data.labels) {
      expect(label.svg).toContain("<svg");
      expect(label.svg).toContain(label.id);
    }
    expect(data.size_mm).toBe(8);
    expect(data.chars).toBe("44");
  });

  it("selects via the shared filter grammar and defaults its options", async () => {
    const t = mockTransport();
    const data = expectOk<PrintData>(
      await t({
        op: "Print",
        collection: "parts",
        selection: { filter: { status: "unbound" } },
      }),
    );
    expect(data.labels).toHaveLength(3);
    expect(data.size_mm).toBe(8);
    expect(data.chars).not.toBe("auto");
  });

  it("resolves auto chars to a concrete grouping", async () => {
    const t = mockTransport();
    const data = expectOk<PrintData>(
      await t({
        op: "Print",
        collection: "parts",
        selection: { ids: ["PQ7G2MNVX4KH9T"] },
        options: { chars: "auto", size_mm: 12 },
      }),
    );
    expect(["44", "444", "554"]).toContain(data.chars);
  });

  it("validates copies, layout, flag's cable_od_mm, and chars", async () => {
    const t = mockTransport();
    const sel = { ids: ["PQ7G2MNVX4KH9T"] };
    const copies = expectErr(
      await t({ op: "Print", collection: "parts", selection: sel, options: { copies: 0 } }),
    );
    expect(copies.kind).toBe("BadRequest");
    const layout = expectErr(
      await t({ op: "Print", collection: "parts", selection: sel, options: { layout: "spiral" } }),
    );
    expect(layout.kind).toBe("Validation");
    const flag = expectErr(
      await t({ op: "Print", collection: "parts", selection: sel, options: { layout: "flag" } }),
    );
    expect(flag.kind).toBe("Validation");
    expect(flag.message).toContain("cable_od_mm");
    const chars = expectErr(
      await t({ op: "Print", collection: "parts", selection: sel, options: { chars: "999" } }),
    );
    expect(chars.kind).toBe("Validation");
  });

  it("propagates resolve failures from the ids selection and NotFounds an empty match", async () => {
    const t = mockTransport();
    const bad = expectErr(
      await t({ op: "Print", collection: "parts", selection: { ids: ["PQ7"] } }),
    );
    expect(bad.kind).toBe("BadRequest");
    const empty = expectErr(
      await t({
        op: "Print",
        collection: "parts",
        selection: { filter: { text: "no-such-needle" } },
      }),
    );
    expect(empty.kind).toBe("NotFound");
    expect(empty.message).toContain("selection matched no entities");
  });
});

describe("mockTransport Export", () => {
  it("builds a real CSV from the store with the engine's column roster", async () => {
    const t = mockTransport();
    const data = expectOk<ExportData>(await t({ op: "Export", collection: "parts", format: "csv" }));
    expect(data.format).toBe("csv");
    expect(data.rows).toBe(partsEntities.length);
    const lines = data.content.trimEnd().split("\n");
    expect(lines[0]).toBe(
      "id,status,created_at,type,description,vendor,part_number,location,notes",
    );
    expect(lines).toHaveLength(partsEntities.length + 1);
    expect(lines.some((l) => l.startsWith("PQ7G2MNVX4KH9T,bound,"))).toBe(true);
  });

  it("escapes commas and quotes per CSV rules", async () => {
    const t = mockTransport();
    const data = expectOk<ExportData>(await t({ op: "Export", collection: "parts", format: "csv" }));
    // The KNF fixture description carries a comma and inner quotes.
    expect(data.content).toContain('"Diaphragm pump, 5.5 l/min ""lab grade"""');
  });

  it("answers Unsupported for non-csv formats", async () => {
    const t = mockTransport();
    const error = expectErr(await t({ op: "Export", collection: "parts", format: "xlsx" }));
    expect(error.kind).toBe("Unsupported");
    expect(error.message).toContain("csv");
  });
});

describe("mockTransport PollProposal and Whoami", () => {
  it("PollProposal reports the mock proposal as open", async () => {
    const t = mockTransport();
    const data = expectOk<PollProposalData>(
      await t({
        op: "PollProposal",
        proposal: { url: "mock://proposal/1", local_id: "1", adapter: "mock" },
      }),
    );
    expect(data.status.kind).toBe("open");
  });

  it("Whoami answers the engine's operator shape with a fake operator", async () => {
    const t = mockTransport();
    const data = expectOk<WhoamiData>(await t({ op: "Whoami" }));
    expect(data).toEqual({
      id: "mock:operator",
      display_name: "Mock Operator",
      source: "OfflineClaim",
      verified_at: null,
    });
  });
});
