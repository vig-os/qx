// FE preflight per ADR-016 + issue #23. Runs the same Rust
// validators + policy engine that CI runs, compiled to WASM, against
// the proposed Diff so the operator sees allow/warn/blocked status
// before submitting a PR.
//
// Authority remains CI's — this module is *advisory*. See ADR-016
// §"FE preflight = advisory, immediate; CI = final, merge-blocking".

import {
  classifyDiffSync,
  policyDecisionSync,
  type Action,
  type AuthDecision,
} from "../wasm/loader";
import type { RegistryRow } from "./schema";
import { REGISTRY_FIELD_KEYS } from "./schema";

/** Single edit produced by the FE — a `QueuedBind`, a Lookup-edit, etc. */
export interface QueueItem {
  id: string;
  kind: "bind" | "edit" | "void";
  /** Fields the operator set / changed. Sparse — only the columns
   * that actually changed. */
  fields: Record<string, string>;
}

/**
 * Build a Rust-shaped `Diff` from a queue of operator-edits + the
 * current registry baseline. Mirrors `crates/domain::Diff`.
 *
 * - `bind` items become `edits` flipping `status:unbound→bound` plus
 *   the metadata fields set by the operator.
 * - `edit` items become `edits` over the current row's values.
 * - `void` items become `edits` that flip status to `void`.
 */
export function buildDiffFromQueue(
  items: QueueItem[],
  registry: ReadonlyMap<string, RegistryRow>,
): {
  adds: Array<{ id: string; fields: Record<string, string> }>;
  deletes: Array<{ id: string; fields: Record<string, string> }>;
  edits: Array<{
    id: string;
    before: Record<string, string>;
    after: Record<string, string>;
    changed_keys: string[];
  }>;
  header_changes: Array<{ file: string; before: string[]; after: string[] }>;
} {
  const edits = items
    .map((item) => {
      const row = registry.get(item.id);
      if (!row) return null; // unknown id — surfaced as a violation elsewhere
      const before: Record<string, string> = {};
      const after: Record<string, string> = {};
      for (const key of REGISTRY_FIELD_KEYS) {
        before[key as string] = (row as unknown as Record<string, string>)[
          key as string
        ] ?? "";
      }
      // Start `after` from `before`, then layer the operator's changes
      // and the kind-specific status flip.
      for (const k of Object.keys(before)) after[k] = before[k];
      for (const [k, v] of Object.entries(item.fields)) after[k] = v;
      if (item.kind === "bind") {
        after.status = "bound";
        if (!after.bound_at) {
          after.bound_at = new Date().toISOString();
        }
      } else if (item.kind === "void") {
        after.status = "void";
      }
      const changed_keys = Object.keys(after).filter((k) => after[k] !== before[k]);
      return {
        id: item.id,
        before,
        after,
        changed_keys,
      };
    })
    .filter((e): e is NonNullable<typeof e> => e !== null);

  return {
    adds: [],
    deletes: [],
    edits,
    header_changes: [],
  };
}

/** Anonymous operator stand-in until #5 wires real GitHub OAuth.
 *
 * The shape matches `crates/domain::Operator` exactly:
 *   - `IdentitySource` is internally tagged with `kind` (snake-case).
 *   - `OperatorId` is a newtype tuple struct that serde flattens to
 *     a bare string.
 *   - `verified_at` is `Option<Timestamp>`; `Timestamp` round-trips
 *     RFC-3339 strings, so `null` is correct for "unverified". */
export function anonymousOperator(): {
  id: string;
  display_name: string;
  source: { kind: "env_user" };
  verified_at: string | null;
  claims: Record<string, string>;
  pubkey: string | null;
} {
  return {
    id: "fe-anonymous",
    display_name: "FE (no OAuth — see #5)",
    source: { kind: "env_user" },
    verified_at: null,
    claims: {},
    pubkey: null,
  };
}

/**
 * Issue we can spot before the WASM engine ever sees the diff. The
 * Rust validators run on a registry+diff pair; FE-only issues such
 * as "this id isn't in the loaded registry" are caught here.
 */
export interface LocalIssue {
  kind: "unknown_id" | "duplicate_in_queue";
  message: string;
  id: string;
}

/** Full preflight: classify + policy via WASM, plus a few FE-local
 * sanity checks the WASM doesn't perform (e.g. unknown id).
 *
 * Note: `validateDiff` (the Rust whole-registry validator) is not
 * called here. It validates a `Part`-typed registry whose
 * `Option<...>` fields don't round-trip cleanly from the FE's plain-
 * string `RegistryRow` shape. The semantic-policy gates (status
 * transitions, destructive elevation, bulk threshold, header
 * changes) all live in `policyDecision`, which is what we wire up.
 * CI runs `validateDiff` against the real on-disk CSV regardless. */
export interface PreflightResult {
  actions: Action[];
  decision: AuthDecision;
  localIssues: LocalIssue[];
}

export function runPreflight(
  queue: QueueItem[],
  registry: ReadonlyMap<string, RegistryRow>,
): PreflightResult {
  const localIssues: LocalIssue[] = [];
  const seen = new Set<string>();
  for (const item of queue) {
    if (seen.has(item.id)) {
      localIssues.push({
        kind: "duplicate_in_queue",
        id: item.id,
        message: `ID ${item.id} appears more than once in the queue`,
      });
    }
    seen.add(item.id);
    if (!registry.has(item.id)) {
      localIssues.push({
        kind: "unknown_id",
        id: item.id,
        message: `ID ${item.id} is not in the loaded registry`,
      });
    }
  }

  const diff = buildDiffFromQueue(queue, registry);
  const actions = classifyDiffSync(diff);
  const decision = policyDecisionSync({
    diff,
    operator: anonymousOperator(),
  });
  return { actions, decision, localIssues };
}
