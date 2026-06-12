// Descriptor-driven data grid: every column is generated from the
// collection descriptor (micro-core keys + Describe field metadata).
// No field names or labels are hardcoded here.

import type { CollectionDescriptor, Entity, SortSpec } from "../protocol";
import { entityValue, MICRO_CORE_KEYS } from "./value";

export interface Column {
  key: string;
  header: string;
}

export function gridColumns(descriptor: CollectionDescriptor): Column[] {
  return [
    // Micro-core keys render their protocol identifier as the header.
    ...MICRO_CORE_KEYS.filter((k) => k !== "created_at").map((key) => ({ key, header: key })),
    // Declared fields carry their display label in the descriptor.
    ...descriptor.fields.map((f) => ({ key: f.key, header: f.label })),
  ];
}

export function DataGrid({
  descriptor,
  items,
  sort,
  onSort,
  onOpen,
}: {
  descriptor: CollectionDescriptor;
  items: Entity[];
  sort?: SortSpec | null;
  onSort?: (key: string) => void;
  onOpen?: (entity: Entity) => void;
}) {
  const columns = gridColumns(descriptor);
  return (
    <table className="w-full border-collapse text-sm">
      <thead>
        <tr className="border-b border-zinc-300 text-left">
          {columns.map((col) => (
            <th key={col.key} className="px-3 py-2 font-medium text-zinc-600">
              <button
                type="button"
                onClick={() => onSort?.(col.key)}
                className="inline-flex items-center gap-1 hover:text-zinc-900"
              >
                {col.header}
                {sort?.field === col.key && (
                  <span aria-hidden>{sort.dir === "asc" ? "▲" : "▼"}</span>
                )}
              </button>
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {items.map((e) => (
          <tr
            key={e.id}
            onClick={() => onOpen?.(e)}
            className="cursor-pointer border-b border-zinc-100 hover:bg-zinc-100"
          >
            {columns.map((col) => (
              <td key={col.key} className="px-3 py-2">
                {col.key === "id" ? (
                  <span className="font-mono text-xs">{e.id}</span>
                ) : (
                  (entityValue(e, col.key) ?? "")
                )}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
