# ADR-015 — Print event log: non-destructive audit trail of every label print

- Status: Proposed
- Date: 2026-05-08
- Component / area: parts registry — print audit trail (`print_log.csv`,
  `label.py --log`, future FE print pipeline, CI validators)
- Reviewers: _(pending)_

## Context

A printed-but-unbound label is a real artifact in physical space.
ADR-012 holds `status` as the *logical* relationship between an ID and
a real part (`unbound | bound | void`); minting creates the row and
binding flips the status. **Printing is not represented anywhere.**

For QA / regulatory traceability the registry needs to answer "this
sticker exists, when was it printed, by whom, and on what tape?"
without conflating that with the logical bind state. Reprints (a
sticker peels off; a new one is made) are normal; one ID may have
many print events over its lifetime.

The design constraints:

1. **Non-destructive.** A print never overwrites or invalidates an
   earlier print. Append-only.
2. **Multiplicity.** A `status` enum cannot represent "printed N
   times" — event logs are the standard audit pattern.
3. **Sort-stable.** Like `registry.csv` (per ADR-013), the file must
   re-sort to itself byte-for-byte so PR diffs stay reviewable.
4. **One PR can carry mixed diffs.** Validators must handle bind
   appends/edits to `registry.csv` and print appends to
   `print_log.csv` independently in the same PR.

This ADR locks the schema, the FK rule, and the
event-not-status decision. Issue #12 is the implementation tracker.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Add `printed_at` / `print_count` columns to `registry.csv` | One file; no new schema | Loses multiplicity (can only record the latest print); conflates physical artifact with logical association — the exact failure ADR-012 §Decision rejects for `status` | Rejected: doesn't survive reprints |
| Promote `status` to a richer enum (`unbound → printed → bound → void`) | Single state machine, easy to reason about | Same conflation; can't represent "printed twice"; requires migrating every existing row and every `status` consumer | Rejected: violates ADR-012 |
| External event store (DB / log service) | Industrial standard for audit logs | Couples the registry to a hosted service; breaks the file-in-git permanence intent of ADR-012/ADR-013 | Rejected: permanence constraint |
| New append-only `print_log.csv` next to `registry.csv`, FK-validated by CI | Composable with the existing PR pipeline; preserves `status` semantics; multiple events per ID supported natively; same diff/review tooling as `registry.csv` | One more file for the validator to police; CLI/FE both need the wiring | **Chosen** |

## Decision

A new file `print_log.csv` lives next to `registry.csv`. It is
**append-only** (modulo the sort-stability rewrite), **never deletes
or rewrites** a prior event, and references `registry.csv` by
foreign key.

**Schema** (column order is normative — CI checks the header):

```
id,printed_at,printed_by,layout,size_mm,extra,copies,output_mode,batch_label
```

| Column | Type / domain | Notes |
|---|---|---|
| `id` | 12-char canonical ID | FK → `registry.csv.id`, validated by CI |
| `printed_at` | ISO-8601 UTC, second precision (`Z` suffix) | Sort key; multiple events per ID get distinct timestamps |
| `printed_by` | string | CLI: `$USER` or `--operator <name>`; FE: GitHub login resolved via OAuth at submission |
| `layout` | `vert` \| `horz` \| `flag` | Validated against allowed layout ids |
| `size_mm` | float, `%g` formatted | Short-side mm at print time |
| `extra` | JSON object as string | Layout-specific options. `{}` for `vert`/`horz`. `{"cableOd": 6}` for `flag`. Schema not policed beyond JSON-parse-ability |
| `copies` | integer ≥ 1 | One CSV row per print *event*, even if `copies > 1`; do not duplicate rows |
| `output_mode` | string | Print pipeline descriptor: `dk-continuous-auto-cut`, `dk-strip-crop`, `a4-sheet`, … Open vocabulary; CI does not enumerate |
| `batch_label` | string, may be empty | The batch the IDs were minted in, for grouping the audit view |

**FK rule:** every `id` in `print_log.csv` must exist in
`registry.csv`. The CLI emits a stderr warning on local FK miss but
still logs (the registry might be out of sync mid-edit); CI rejects
orphan events at PR time — the validator is the source of truth.

**Sort rule:** `print_log.csv` is sorted by `printed_at` ascending,
then by `id` for ties. Re-sorting must equal the file (same
invariant as ADR-013 for `registry.csv`).

**CLI integration:** `label.py` grows `--log`/`--no-log` (default
on), `--operator` (default `$USER`), `--output-mode` (default
`dk-continuous-auto-cut`). The append happens after a successful
SVG render of all selected IDs.

## Rationale

The "event log alongside, not status promotion" pattern is the
standard for regulated systems (e.g. ISO 9001 §7.5 documented
information, IEC 62304 §5.8 software release records, GxP audit
trails) precisely because it preserves both the current logical
state *and* the full history without conflating them.

Anchoring `print_log.csv` in the same PR-driven, file-in-git pipeline
as `registry.csv` (per ADR-013) means zero new infrastructure, zero
new auth surface, and reviewable diffs for every print batch. The
trade-off — files in git can grow large — is acceptable: at lab
scale (10² – 10³ prints/year) the file stays small enough to load
into the WASM DuckDB session that ADR-013/ADR-014 already commits to.

The single-row-per-event-not-per-copy rule keeps the file compact
when high-copies prints land (e.g. a 50-copy bulk print of a strip
stays one row, not 50). The `copies` column preserves the
information without inflating the diff.

## Consequences

- **Validator work** — CI must now check: header equality, FK to
  `registry.csv`, sort-stability, `layout` ∈ allowed set, `extra`
  parses as JSON, `copies` ≥ 1. Filed as part of the validators
  issue.
- **FE pipeline** — the print tab must queue print events into the
  same submission queue as bind diffs (DRY, per ADR-013); one PR can
  carry both. Implementation lives in `web/`, separate from this
  CLI-side change.
- **Lookup view** — the row detail page should show a chronological
  event list (binds + prints) for an ID once the FE wires this up.
- **Repo growth** — `print_log.csv` accumulates forever (no
  rotation). At 10³ events/year the file is ~100 KB/year; not a
  concern for a decade.
- **Privacy** — `printed_by` is a person's name or login; this is
  the same exposure level as git commit authorship and is consistent
  with the "registry data is non-sensitive" stance in ADR-013.

## Open questions / supersession triggers

- If lab scale grows past ~10⁵ events the CSV-in-git approach gets
  awkward; revisit with a partitioned-by-year file or DuckDB-native
  storage.
- If a regulator requires cryptographic signatures on each event
  (eIDAS-style), this ADR is superseded by a signed-event scheme.
- If the FE ever needs to record events the user didn't actually
  cause (e.g. printer-reported reprints), the schema needs an
  `actor_kind` column distinguishing human / system / printer. Until
  then the `printed_by` field is the actor.

## References

- [ADR-012 — Part identification](ADR-012-part-identification.md) §Decision (status semantics)
- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md) §Decision (PR pipeline, sort-stability invariant)
- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md) §Consequences (queue / submission shape)
- Issue [#12 — Print event log](https://github.com/MorePET/part-registry/issues/12)
- `label.py` (`--log` / `--operator` / `--output-mode` flags, `append_print_events`)
- `print_log.csv` (the data file)
