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
  /** Case-insensitive free-text match over id + declared field values. */
  text?: string | null;
  /** Per-field match — case-insensitive substring, keyed by field key. */
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

/**
 * Selection for ops acting on an entity set (Rust `Selection`, serde
 * external tagging): explicit ids or the one shared Filter grammar.
 */
export type Selection = { ids: string[] } | { filter: Filter };

/**
 * Print options (ADR-031). Every key carries a serde default in Rust
 * (layout "horz", size_mm 8, chars "auto", micro false, copies 1,
 * log true), so all are optional on the wire.
 */
export interface PrintOptions {
  /** Geometry preset token: one of PRINT_LAYOUTS. */
  layout?: string;
  size_mm?: number;
  /** Human-ID grouping token: one of PRINT_CHARS. */
  chars?: string;
  micro?: boolean;
  /** Required when layout is "flag". */
  cable_od_mm?: number | null;
  copies?: number;
  /** Append print events to the audit surface (engine default: true). */
  log?: boolean;
}

// Protocol-declared option vocabularies (crates/app: protocol.rs and
// engine.rs print handler) — the UI renders these, never its own list.
export const PRINT_LAYOUTS = ["vert", "horz", "flag"] as const;
export const PRINT_CHARS = ["44", "444", "554", "auto"] as const;

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
  | {
      op: "Create";
      collection: string;
      /** Defaults to 1 in the engine. */
      n?: number | null;
      /** Must be absent/empty: mint-then-bind (ADR-012). */
      fields?: Record<string, string> | null;
    }
  | { op: "Edit"; collection: string; id: string; fields: Record<string, string> }
  | {
      op: "Transition";
      collection: string;
      id: string;
      to: string;
      fields?: Record<string, string> | null;
    }
  | { op: "Print"; collection: string; selection: Selection; options?: PrintOptions | null }
  | { op: "Export"; collection: string; format: string }
  | { op: "PollProposal"; proposal: ProposalRef }
  | { op: "Whoami" };

export type Op = Request["op"];

// ---------------------------------------------------------------------------
// Response envelope

export type ErrorKind =
  | "NotFound"
  | "Ambiguous"
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

// ---------------------------------------------------------------------------
// Print / Export / PollProposal / Whoami payloads — shapes mirror the
// engine's JSON responses (crates/app/src/engine.rs).

/** One rendered label: the entity id + its SVG markup. */
export interface PrintLabel {
  id: string;
  svg: string;
}

/** `Print` result: rendered SVGs + the resolved render parameters. */
export interface PrintData {
  labels: PrintLabel[];
  size_mm: number;
  /** The grouping actually used ("auto" resolves to a concrete one). */
  chars: string;
  warning: string | null;
}

/** `Export` result: a generated flat artifact, never committed. */
export interface ExportData {
  format: string;
  content: string;
  rows: number;
}

/** Status of a submitted proposal (Rust `ProposalStatus`). */
export interface ProposalStatus {
  kind: string;
}

/** `PollProposal` result. */
export interface PollProposalData {
  status: ProposalStatus;
}

/** `Whoami` result: the current operator identity. */
export interface WhoamiData {
  id: string;
  display_name: string;
  source: string;
  verified_at: string | null;
}
