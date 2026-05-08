# Decision records — part-registry

Markdown ADRs (Architecture Decision Records) for the parts registry.
Each ADR captures one decision — its context, the alternatives that
were considered, the chosen path, and the consequences — so that an
auditor or a new engineer can reconstruct *why* a choice was made and
*what* would invalidate it.

Methodology, status taxonomy, and template are mirrored from the
`MorePET/exopet` decisions framework; see [`METHODOLOGY.md`](METHODOLOGY.md).

## How this folder is organized

- [`METHODOLOGY.md`](METHODOLOGY.md) — playbook for how decisions are
  made and recorded.
- [`LOG.md`](LOG.md) — append-only chronological process trail.
- [`ADR-template.md`](ADR-template.md) — copy this when adding a new ADR.

## Status taxonomy

Six allowed values, defined in full in [`METHODOLOGY.md`](METHODOLOGY.md):

- **Accepted** — in force, governs current work.
- **Proposed** — recommended, awaiting team review.
- **Research** — architectural exploration; not a commitment.
- **Deferred** — postponed with explicit trigger conditions.
- **Rejected** — considered and decided against.
- **Superseded by ADR-NNN** — replaced by a newer ADR.

## Naming convention

`ADR-NNN-short-slug.md`. Numbers continue from `MorePET/exopet`'s
sequence — ADR-001 through ADR-011 are exopet hardware decisions and
stay there; ADR-012 and ADR-013 are the parts-identification scheme
and the registry web app, respectively, and live here. New ADRs in
this repo continue from ADR-014.

## Index

### Accepted

_(none yet — ADR-012 and ADR-013 are Proposed pending team review.)_

### Proposed

- [ADR-012 — Part identification: nano-id + QR labels with mint-then-bind workflow](ADR-012-part-identification.md)
- [ADR-013 — Parts registry web app: GH Pages + WASM DuckDB + PR-driven binds](ADR-013-parts-registry-web-app.md)
