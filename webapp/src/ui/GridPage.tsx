import { useState } from "react";
import type { CollectionDescriptor, Filter, SortSpec } from "../protocol";
import { useCount, useList } from "../data/hooks";
import { entityHref } from "../router";
import { CountStrip } from "./CountStrip";
import { DataGrid } from "./DataGrid";

const PAGE_SIZE = 25;

export function GridPage({ descriptor }: { descriptor: CollectionDescriptor }) {
  const [status, setStatus] = useState<string | null>(null);
  const [text, setText] = useState("");
  const [sort, setSort] = useState<SortSpec | null>(null);
  const [offset, setOffset] = useState(0);

  const filter: Filter = { status, kind: null, text: text === "" ? null : text, fields: null };
  const list = useList(descriptor.name, {
    filter,
    ...(sort ? { sort: [sort] } : {}),
    page: { offset, limit: PAGE_SIZE },
  });
  // Counts honor the text filter but never the status filter, so the
  // strip stays meaningful while a status chip is active.
  const count = useCount(descriptor.name, "status", { ...filter, status: null });

  const setStatusFilter = (next: string | null) => {
    setOffset(0);
    setStatus(next);
  };
  const setTextFilter = (next: string) => {
    setOffset(0);
    setText(next);
  };
  const toggleSort = (field: string) => {
    setOffset(0);
    setSort((prev) =>
      prev?.field === field
        ? prev.dir === "asc"
          ? { field, dir: "desc" }
          : null
        : { field, dir: "asc" },
    );
  };

  if (list.isError) {
    return <p className="text-red-700">{(list.error as Error).message}</p>;
  }

  const data = list.data;
  const total = data?.total ?? 0;
  const from = Math.min(offset + 1, total);
  const to = Math.min(offset + PAGE_SIZE, total);

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-4">
        <CountStrip
          statuses={descriptor.lifecycle.statuses}
          counts={count.data?.counts ?? {}}
          active={status}
          onSelect={setStatusFilter}
        />
        <select
          value={status ?? ""}
          onChange={(e) => setStatusFilter(e.target.value === "" ? null : e.target.value)}
          aria-label="status filter"
          className="rounded border border-zinc-300 bg-white px-2 py-1 text-sm"
        >
          <option value="">*</option>
          {descriptor.lifecycle.statuses.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <input
          type="search"
          value={text}
          onChange={(e) => setTextFilter(e.target.value)}
          placeholder="filter…"
          aria-label="text filter"
          className="w-64 rounded border border-zinc-300 bg-white px-2 py-1 text-sm"
        />
      </div>

      {data === undefined ? (
        <p className="text-zinc-500">…</p>
      ) : (
        <>
          <DataGrid
            descriptor={descriptor}
            items={data.items}
            sort={sort}
            onSort={toggleSort}
            onOpen={(e) => {
              window.location.hash = entityHref(e.id);
            }}
          />
          <div className="flex items-center gap-3 text-sm text-zinc-600">
            <button
              type="button"
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              className="rounded border border-zinc-300 px-2 py-1 disabled:opacity-40"
            >
              ‹
            </button>
            <span>
              {from}–{to} / {total}
            </span>
            <button
              type="button"
              disabled={offset + PAGE_SIZE >= total}
              onClick={() => setOffset(offset + PAGE_SIZE)}
              className="rounded border border-zinc-300 px-2 py-1 disabled:opacity-40"
            >
              ›
            </button>
          </div>
        </>
      )}
    </div>
  );
}
