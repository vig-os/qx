import { describe, expect, it } from "vitest";
import { err, ok } from "../protocol";
import {
  BIND_QUEUE_KEY,
  dequeue,
  enqueue,
  loadQueue,
  saveQueue,
  settle,
  type BindQueueItem,
} from "./queue";

const item = (id: string, fields: Record<string, string> = {}): BindQueueItem => ({
  id,
  fields,
  queued_at: "2026-06-10T00:00:00.000Z",
  error: null,
});

describe("bind queue reducers", () => {
  it("enqueue appends and replaces an existing id, clearing stale errors", () => {
    const failed: BindQueueItem = {
      ...item("AAAA2345678BCD", { vendor: "Old" }),
      error: { kind: "Auth", message: "no operator set" },
    };
    const q1 = enqueue([failed], "BBBB2345678CDE", { type: "pump" }, "2026-06-10T01:00:00.000Z");
    expect(q1.map((i) => i.id)).toEqual(["AAAA2345678BCD", "BBBB2345678CDE"]);
    const q2 = enqueue(q1, "AAAA2345678BCD", { vendor: "New" }, "2026-06-10T02:00:00.000Z");
    expect(q2.map((i) => i.id)).toEqual(["BBBB2345678CDE", "AAAA2345678BCD"]);
    const replaced = q2.find((i) => i.id === "AAAA2345678BCD");
    expect(replaced?.fields).toEqual({ vendor: "New" });
    expect(replaced?.error).toBeNull();
  });

  it("dequeue removes by id and ignores unknown ids", () => {
    const q = [item("AAAA2345678BCD"), item("BBBB2345678CDE")];
    expect(dequeue(q, "AAAA2345678BCD").map((i) => i.id)).toEqual(["BBBB2345678CDE"]);
    expect(dequeue(q, "ZZZZ2345678YXW")).toHaveLength(2);
  });

  it("settle removes the item on a protocol Ok", () => {
    const q = [item("AAAA2345678BCD"), item("BBBB2345678CDE")];
    const next = settle(q, "AAAA2345678BCD", ok({ id: "AAAA2345678BCD" }));
    expect(next.map((i) => i.id)).toEqual(["BBBB2345678CDE"]);
  });

  it("settle keeps a failed item queued with the error recorded verbatim", () => {
    const q = [item("AAAA2345678BCD")];
    const next = settle(
      q,
      "AAAA2345678BCD",
      err("Backend", "browser submission lands with the OAuth + PR wiring (ADR-019/020)"),
    );
    expect(next).toHaveLength(1);
    expect(next[0]?.error).toEqual({
      kind: "Backend",
      message: "browser submission lands with the OAuth + PR wiring (ADR-019/020)",
    });
  });

  it("round-trips through storage and tolerates malformed payloads", () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    };
    const q = [item("AAAA2345678BCD", { vendor: "Omega" })];
    saveQueue(q, storage);
    expect(loadQueue(storage)).toEqual(q);
    store.set(BIND_QUEUE_KEY, "{not json");
    expect(loadQueue(storage)).toEqual([]);
    store.set(BIND_QUEUE_KEY, JSON.stringify([{ nope: true }, item("BBBB2345678CDE")]));
    expect(loadQueue(storage).map((i) => i.id)).toEqual(["BBBB2345678CDE"]);
  });
});
