import { describe, expect, it } from "vitest";
import type {
  CountData,
  CreateData,
  DescribeData,
  EditData,
  Entity,
  ListData,
  Request,
  Response,
  TransitionData,
} from "../protocol";
import { mockTransport } from "./mock";
import { partsEntities } from "./fixtures";

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

  it("filters by exact field value", async () => {
    const t = mockTransport();
    const data = expectOk<ListData>(await t(list({ filter: { fields: { vendor: "Lapp" } } })));
    expect(data.total).toBe(2);
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

  it("rejects an unknown collection", async () => {
    const t = mockTransport();
    const error = expectErr(await t(list({ collection: "gadgets" })));
    expect(error.kind).toBe("BadRequest");
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
    expect(parts?.fields.map((f) => f.key)).toEqual([
      "type",
      "vendor",
      "location",
      "part_number",
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
    expect(expectErr(await t({ op: "Resolve", id: "bogus:PQ7G2MNVX4KH9T" })).kind).toBe(
      "Validation",
    );
    expect(expectErr(await t({ op: "Resolve", id: "22222222222222" })).kind).toBe("NotFound");
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
