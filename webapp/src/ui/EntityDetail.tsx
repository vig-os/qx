// Descriptor-driven entity detail: micro-core rows render protocol
// identifiers, declared-field rows render descriptor labels, lifecycle
// stamps render their status tokens. No hardcoded field names/labels.

import type { ReactNode } from "react";
import type { CollectionDescriptor, Entity } from "../protocol";
import { entityLabel, entityValue } from "./value";

function Row({ term, children }: { term: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[12rem_1fr] gap-4 border-b border-zinc-100 py-2">
      <dt className="text-zinc-500">{term}</dt>
      <dd>{children}</dd>
    </div>
  );
}

export function EntityDetail({
  entity,
  descriptor,
}: {
  entity: Entity;
  descriptor: CollectionDescriptor;
}) {
  return (
    <article>
      <h2 className="mb-4 text-lg font-semibold">{entityLabel(entity, descriptor)}</h2>
      <dl className="text-sm">
        <Row term="id">
          <span className="font-mono">{entity.id}</span>
        </Row>
        <Row term="status">{entity.status}</Row>
        <Row term="created_at">{entity.created_at}</Row>
        {Object.entries(entity.transitioned_at).map(([status, ts]) => (
          <Row key={status} term={`transitioned_at · ${status}`}>
            {ts}
          </Row>
        ))}
        {descriptor.fields.map((f) => (
          <Row key={f.key} term={f.label}>
            {entityValue(entity, f.key) ?? <span className="text-zinc-400">—</span>}
          </Row>
        ))}
        {Object.entries(entity.properties).map(([key, value]) => (
          <Row key={key} term={key}>
            <span className="font-mono text-xs">{JSON.stringify(value)}</span>
          </Row>
        ))}
      </dl>
    </article>
  );
}
