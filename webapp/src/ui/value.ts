import type { CollectionDescriptor, Entity } from "../protocol";

// The entity micro-core (ADR-035 §0): keys every entity has regardless
// of collection. These are protocol identifiers — rendered as-is, they
// are not display strings, which is why they may appear here while
// declared-field names/labels must come from the descriptor.
export const MICRO_CORE_KEYS = ["id", "status", "created_at"] as const;

/** Read a renderable value off an entity: micro-core key or declared field. */
export function entityValue(e: Entity, key: string): string | null {
  switch (key) {
    case "id":
      return e.id;
    case "label":
      return e.label;
    case "status":
      return e.status;
    case "kind":
      return e.kind;
    case "created_at":
      return e.created_at;
    default:
      return e.fields[key] ?? null;
  }
}

/** Display label for an entity, generated from render.label_fields. */
export function entityLabel(e: Entity, descriptor: CollectionDescriptor): string {
  const parts = descriptor.render.label_fields
    .map((key) => entityValue(e, key))
    .filter((v): v is string => v != null && v !== "");
  return parts.length > 0 ? parts.join(" — ") : e.id;
}
