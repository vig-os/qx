// TS mirror of the app-layer command protocol (ADR-030 §1).
//
// The Rust crates/app Request/Response enums are the source of truth.
// This file mirrors their serde shape exactly:
//   - Request is internally tagged with "op".
//   - Response is the {ok, data} | {ok, error} envelope.
//
// Keep this file dependency-free: it is shared by every transport and
// by the UI's data layer.

// ---------------------------------------------------------------------------
// Request

/** The one List/Count/selection filter grammar (ADR-035 §0: one `Filter`). */
export interface Filter {
  status?: string | null;
  kind?: string | null;
  /** Case-insensitive free-text match over id/label/kind/field values. */
  text?: string | null;
  /** Exact match per declared-field key, e.g. {"vendor": "Omega"}. */
  fields?: Record<string, string> | null;
}

export type SortDir = "asc" | "desc";

export interface SortSpec {
  field: string;
  dir: SortDir;
}

export interface Page {
  offset: number;
  limit: number;
}

export type Request =
  | { op: "Resolve"; id: string }
  | {
      op: "List";
      collection: string;
      filter?: Filter | null;
      sort?: SortSpec[] | null;
      page?: Page | null;
    }
  | { op: "Count"; collection: string; filter?: Filter | null; by: string }
  | { op: "Describe"; collection?: string | null }
  | { op: "Create"; collection: string; n: number }
  | { op: "Edit"; collection: string; id: string; fields: Record<string, string> }
  | {
      op: "Transition";
      collection: string;
      id: string;
      to: string;
      fields?: Record<string, string> | null;
    }
  | { op: "Whoami" };

export type Op = Request["op"];

// ---------------------------------------------------------------------------
// Response envelope

export type ErrorKind =
  | "NotFound"
  | "Validation"
  | "Unsupported"
  | "Auth"
  | "Backend"
  | "BadRequest";

export interface ProtocolError {
  kind: ErrorKind;
  message: string;
}

export type Response<T = unknown> =
  | { ok: true; data: T }
  | { ok: false; error: ProtocolError };

// ---------------------------------------------------------------------------
// Entities (ADR-035 micro-core + declared fields + open properties)

export interface Entity {
  id: string;
  collection: string;
  label: string | null;
  /** ISO 8601. Micro-core stamp; "Minted" is parts render metadata. */
  created_at: string;
  /** A token from the collection's lifecycle.statuses — data, not an enum. */
  status: string;
  kind: string | null;
  /** Engine-materialized lifecycle stamps, keyed by status token. */
  transitioned_at: Record<string, string>;
  /** Tier-2 declared fields (descriptor-typed). */
  fields: Record<string, string>;
  /** Tier-3 open escape bag — shape-checked only. */
  properties: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Describe payload — descriptors carry ALL display metadata (ADR-035 §0):
// shells render from these, never from hardcoded strings.

export interface FieldDescriptor {
  key: string;
  type: string;
  label: string;
  editable: boolean;
  /** Status from which this field is meaningful/settable. */
  meaningful_from?: string | null;
}

export interface IdDescriptor {
  scheme: string;
  default: boolean;
}

export interface LifecycleDescriptor {
  statuses: string[];
}

export interface RenderDescriptor {
  /** Keys (micro-core or declared fields) composing an entity's display label. */
  label_fields: string[];
}

export interface CollectionDescriptor {
  name: string;
  id: IdDescriptor;
  lifecycle: LifecycleDescriptor;
  fields: FieldDescriptor[];
  render: RenderDescriptor;
}

export interface DescribeData {
  name: string;
  collections: CollectionDescriptor[];
}

// ---------------------------------------------------------------------------
// Op result payloads

export interface ListData {
  items: Entity[];
  total: number;
}

export interface CountData {
  by: string;
  counts: Record<string, number>;
}

// ---------------------------------------------------------------------------
// Mutation payloads — mutations are PROPOSALS (ADR-019): the registry
// does not change until the proposal lands, so these responses carry a
// proposal ref, never the updated entity. Shapes mirror
// crates/app/src/engine.rs (create/edit/transition).

/** Handle to a submitted proposal (Rust `part_registry_domain::ProposalRef`). */
export interface ProposalRef {
  url: string;
  local_id: string | null;
  adapter: string;
}

/** `Create` result: the minted ids + one RFC 3339 stamp per mint event. */
export interface CreateData {
  minted: string[];
  created_at: string;
  proposal: ProposalRef;
}

/** `Edit` result. */
export interface EditData {
  id: string;
  proposal: ProposalRef;
}

/** `Transition` result. */
export interface TransitionData {
  id: string;
  to: string;
  proposal: ProposalRef;
}
