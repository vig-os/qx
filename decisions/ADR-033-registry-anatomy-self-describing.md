# ADR-033 — Registry anatomy: self-describing data repo + operator workspace

- Status: Accepted
- Date: 2026-06-10
- Component / area: the deployed-registry data-repo structure + the
  operator-side workspace. Formalizes "what a deployed part-registry is."
- Reviewers: Lars Gerchow (accepted 2026-06-11)
- Related: ADR-012 (ID + schema contract), ADR-013 (data repo on GH
  Pages), ADR-016 (PR-diff policy), ADR-017 (storage adapters), ADR-018
  (Storage port), ADR-021 (config), ADR-030 (locator `file://`|`github:`),
  ADR-034 (registry manifest — sibling)
- Feeds: `decisions/explorations/operations-catalog.md`

## Context

A deployed registry is a separate git **data repo**
(`bootstrap-data-repo.sh` seeds `registry.csv` / `print_log.csv` /
`audit_log.csv` + README/CONTRIBUTING/Pages workflow). But the **schema
contract** (`schema/registry-contract.json`) lives only in the *tool*
repo — the bootstrap does not seed one. Consequences of that gap:

- A registry is **not self-describing**: it implicitly shares whatever
  contract version the tool happens to ship, so a 2-year-old data repo
  can break under a newer tool.
- **No per-deployment fields**: every registry has exactly the tool's
  columns; a customer who needs `lot_number` or `calibration_due` would
  have to fork the tool.
- **No formal spec** for "what a deployed registry contains," and **no
  multi-registry workflow** for an operator who works across several.

## Decision

### 1. One git repo = one registry

The unit of a registry is one git repo. It matches `bootstrap`, the
`file://`|`github:` locator (ADR-030), and gives one clean
authz/audit/contract boundary. (Federation and multi-registry-per-repo
are rejected as premature — see Alternatives in ADR-030's spirit.)

### 2. The data repo is self-describing

The registry carries its **own versioned contract** in the repo. The
tool validates against the registry's *declared* contract version and
supports a `[min, max]` compatibility range; outside it, the tool
refuses (or offers migration) rather than silently mis-reading. The tool
repo keeps a baseline contract only to *seed* new registries.

### 3. Schema = fixed core + registry-declared custom fields

The tool owns a fixed **regulated core** (`id`, `status`, `created_at`,
`transitioned_at{…}`, `kind`, `components[]` — the ADR-012 fields per
ADR-035 (`batch` retired; `minted_at`/`bound_at` are render names),
uniformly validated by
ADR-016/020). A registry's contract may **declare additional domain
fields** (typed, with metadata: label, editable, meaningful-from-status).
All consumers — validators, FE grid, label render, TUI — operate over
`core ∪ declared`. A registry may **add** custom fields; it may not
remove or redefine core fields.

**Custom-field types** (resolved 2026-06-11): a fixed **scalar set** —
`string` (optional `pattern`), `enum` (value list), `integer`, `number`,
`date` (ISO-8601), `bool`, and `attachment` (value =
`{ref: sha256:…, name: original-filename, desc?}`; blob at the derived
path `attachments/<hash>.<ext>`; optionally type-constrained, e.g.
`attachment(md)` — ADR-035 §4) — each with
optional `required` and `meaningful_from: <status>`. Values serialize as
JSON (ADR-035 §4); the type is a validation + rendering rule the
schema-driven validators / grid / label-render dispatch on. Typed
relations are **declared in the descriptor** (`relations[]` with graph
rules — ADR-035 §1a); ad-hoc untyped ref fields stay out.

### 4. Canonical anatomy

```
<registry-repo>/
  collections/              # every entity collection, one JSONL each (ADR-035 §0)
    parts.jsonl             #   the regulated preset: core+kind+components+per-type+properties
    types.jsonl  products.jsonl  vendors.jsonl  locations.jsonl
  attachments/              # content-addressed blobs <sha256>.<ext> (ADR-035 §4)
  audit_log.jsonl           # ADR-022 audit trail — the ONE stream (print events folded in)
  .part-registry/           # tool config namespace (keeps the root clean)
    contract.json           # versioned schema: collection descriptors (ADR-035 §0)
    manifest.toml           # ADR-034 capabilities / policy / features
    roles.toml              # ADR-034 role bindings (advisory / non-GitHub)
  .github/
    CODEOWNERS              # review routing (ADR-034 enforcement)
    workflows/              # `pr check` gate (ADR-016) + Pages
  README.md  CONTRIBUTING.md
```

Each artifact, the contract that governs it, and how it's enforced
(full operator/admin reference, incl. the admin model, lives in
[`docs/registry-anatomy.md`](../docs/registry-anatomy.md)):

| Artifact | Governed by | Enforced / checked by |
|---|---|---|
| `collections/*.jsonl` | `contract.json` descriptors | engine validation (`core ∪ kind ∪ shape` + FK/graph) in `pr check` CI + preflight; writes only via PR (ADR-018/019) |
| `attachments/<sha256>.<ext>` | `attachment` field decls | ref-exists + hash-matches-content (tamper-evident) |
| `audit_log.jsonl` (the one stream; print events folded in) | `AuditEntry`/`Signature` (ADR-022/024) | append-only, no-rewrite; `&Operator` required; snapshot hash; FK to entities |
| `.part-registry/contract.json` | tool meta-schema + `[min,max]` | tool validates on open; SSOT for validators; snapshot test |
| `.part-registry/manifest.toml` | tool meta-schema | tool reads (advisory); seeds CODEOWNERS (ADR-034) |
| `.part-registry/roles.toml` | `manifest.toml` roles | CI cross-check; drives advisory + non-GitHub authz |
| `.github/CODEOWNERS` | derived from `manifest.toml` | **GitHub branch protection — authoritative authz gate** |
| `.github/workflows/pr-check.yml` | ADR-016 check contract | Actions; required-green to merge |

### 5. Operator workspace

`~/.config/part-registry/registries.toml` lists the registries an
operator uses (`name → locator + default identity/profile`), for quick
switching and cross-registry views. Single-registry operations still
take a locator; the workspace is convenience + the home for any
cross-registry UX.

### 6. Bootstrap seeds the structure

Creating a registry seeds the contract, manifest, roles, CODEOWNERS, and
(for `github:`) branch protection — `bootstrap-data-repo` / the GitHub
App (ADR-034 §6).

## Rationale

Self-describing data repos are **auditable and version-stable**: a
registry validates against its own pinned contract, so it neither
silently drifts nor breaks when the tool moves. Core-fixed + custom keeps
the regulated invariants uniform (every registry has the same audited
core) while letting deployments add domain fields without forking the
tool. One-repo-one-registry gives the cleanest authz/audit/contract
boundary; the workspace delivers multi-registry ergonomics without
federation complexity. The `.part-registry/` namespace keeps the data
root (the collections an operator browses) clean.

## Consequences

- **The contract moves into the data repo**; bootstrap seeds it from the
  tool's baseline. The tool gains a **contract-version compatibility
  matrix** (`[min,max]`) and a refuse/migrate path outside it.
- **Validators/codec/FE become schema-driven** over `core ∪ declared`
  fields rather than a hardcoded column list.
- **Storage adapters (ADR-018)** read the declared schema; the port goes
  collection-generic (`get/list` over entities per descriptor — ADR-018
  refinement note); `snapshot_hash` computes over the declared roster +
  the audit stream, so adding a collection changes hash inputs by data,
  not code.
- **CSV exports are generated, never committed** (`Export` op / Pages
  build artifact — ADR-035 §0): a committed derived view beside the
  source of truth is a rival truth. The Pages FE consumes the build
  artifact until the WASM read path lands.
- **`.part-registry/` is the config namespace** for contract + manifest
  + roles (ADR-034 lives here too).
- **`registries.toml`** is a new operator-side artifact (workspace).

## Corrections

> **2026-06-11:** three refinements from the audit-identity/anchoring
> session (ADR-036/037/038): (1) the anatomy gains
> `.part-registry/gate/` (vendored gate binary + sha256 + attestation +
> source + Nix recipe — ADR-038 §1) and the `anchor.yml`/`bundle.yml`
> workflows (ADR-037 §3); (2) the `[min, max]` tool-compat range splits
> into a hard **metamodel parse floor** plus **derived per-op floors**
> (ADR-038 §3) — the original single range overstated the cliff;
> (3) `roles.toml` dissolves into the `personas` collection (ADR-036
> §1) — identity is collection data, not a sidecar file. Original text
> preserved above for audit.

## Open questions / supersession triggers

- **Contract migration mechanics** across versions (the upgrade path) —
  deferred to a migration ADR when the first schema change ships.
- **Relational `ref` fields** — *resolved by ADR-035 §1a (2026-06-11)*:
  typed relations are declared in the collection descriptor
  (`relations[]`, with graph rules + derived backlinks); ad-hoc untyped
  ref fields stay out. *(Scalar types + `required` / `meaningful_from`
  resolved 2026-06-11, §3.)*
- **Storage-format switch** (CSV → SQLite/DuckDB, ADR-018) — recorded in
  the contract/manifest so the tool knows how to open the registry.
- **Workspace scope** — whether `registries.toml` also caches per-repo
  identity/token profiles (ties to ADR-020 credential resolution).

## References

- ADR-012 — Part identification (the core fields)
- ADR-013 — Parts registry data repo (GH Pages)
- ADR-016 — PR-diff policy enforcement
- ADR-018 — Storage as a port (schema-driven adapters)
- ADR-030 — Shells + the `file://`|`github:` locator
- ADR-034 — Registry manifest / capabilities (sibling)
- `tools/bootstrap-data-repo.sh` — current data-repo seeder
- `schema/registry-contract.json`, `registry_contract.py` — current contract
