# Registry anatomy — files, contracts, and enforcement

The operator/admin reference for **what a deployed part-registry contains
and how each piece is kept honest**. The *decisions* behind this live in
[ADR-033](../decisions/ADR-033-registry-anatomy-self-describing.md)
(anatomy + self-describing contract) and
[ADR-034](../decisions/ADR-034-registry-manifest-capabilities.md)
(manifest + host-enforced authz); this page is the living reference they
point at.

A deployed registry is **one git repo = one registry**. The data root
holds the CSVs a human or spreadsheet touches; tool config lives under
`.part-registry/`; the host's enforcement lives under `.github/` + repo
settings.

> **Format note (ADR-035):** every entity lives in a **collection** —
> one JSONL file per collection under `collections/` (parts, types,
> products, vendors, locations — `batch` is retired per ADR-035 §0;
> grouping is the mint event), each entity a `{stable id +
> mutable label}` record referenced **by id** (renames never break
> references). The contract declares the collection descriptors; the
> `parts` collection is the regulated preset. CSV exports are
> **generated, never committed** (`Export` op / Pages build artifact);
> print events live **in the audit stream** (one stream — `print_log`
> is retired); large content is content-addressed under `attachments/`.
> In the tables below read `collections/parts.jsonl` for `registry.csv`
> and `audit_log.jsonl` for both logs; ADR-035 is authoritative for the
> record model.

```
<registry-repo>/
  collections/              # every entity collection, one JSONL each (ADR-035 §0)
    parts.jsonl             #   the regulated preset: core+kind+components+per-type+properties
    types.jsonl  products.jsonl  vendors.jsonl  locations.jsonl
  attachments/              # content-addressed blobs <sha256>.<ext> (ADR-035 §4)
  audit_log.jsonl           # the ONE stream — audit trail incl. print events (ADR-022)
  .part-registry/
    contract.json           # versioned schema: collection descriptors (ADR-035 §0)
    manifest.toml           # capabilities / policy / feature flags
    roles.toml              # role bindings (advisory / non-GitHub)
  .github/
    CODEOWNERS              # review routing — generated from the manifest
    workflows/
      pr-check.yml          # `pr check --diff` gate (ADR-016)
      pages.yml             # web view deploy (ADR-013)
  README.md  CONTRIBUTING.md  .gitignore
```

## A. Data files (repo root)

| File | What it does | Governed by | Enforced / checked by | Administered by |
|---|---|---|---|---|
| `collections/parts.jsonl` | Parts collection — one entity per line (`id, status, created_at, transitioned_at{…}, kind, components[]` + per-kind fields + `properties`) | `contract.json` (field set, order, statuses, ID rules) | `validate-registry` (header-schema · required-field · id-format · status-enum · status-field required/forbidden · id-uniqueness · sort-stability) in `pr check` CI + FE/CLI preflight. **No direct writes** — read+audit-append only (ADR-018); mutations via PR (ADR-019) | Operators propose via `mint`/`bind`/`void` → PR; GitHub gates merge |
| `print_log.csv` | Append-only print audit (ADR-015) — `id, printed_at, layout, size, copies, operator, output_mode, extra` | print-event shape in `contract.json`; **FK** → `registry.csv` ids | FK-integrity (orphans flagged in CI) · sort-by-timestamp · append-only | Written by `render-label`/print; rides the same PR (or appended on `file://` commit) |
| `audit_log.csv` | Append-only audit trail (ADR-022) — one `AuditEntry` per mutation: `operator(source, verified_at), action, ts, chain_hash, signature` | `AuditEntry` schema (ADR-022/018); signature/chain (ADR-024) | append-only + **no rewrite of prior rows** · every entry carries a verified `&Operator` (ADR-020) · `snapshot_hash` reproducibility (ADR-024) · `verify-signature` | Written automatically by the tool on every mutation; immutable |

## B. Config namespace — `.part-registry/`

| File | What it does | Governed by | Enforced / checked by | Administered by |
|---|---|---|---|---|
| `contract.json` | **Schema/data contract** (versioned): `schema_version`, field set (core + custom, each `label/editable/meaningful-from/type`), ID rules, status enum | tool's contract **meta-schema** + supported `[min,max]` version range | tool validates the contract on open (known version? well-formed?); it's the **SSOT** every `validate-*` reads; schema-snapshot test catches breaking changes | Seeded by bootstrap from the tool baseline; changes via PR on a **CODEOWNERS-protected path** |
| `manifest.toml` | **Capabilities/policy** (ADR-034): registry id/metadata, **enabled-ops allow-list**, **feature flags** (layouts/output-modes/scan), **advisory role→capability map** | tool's manifest meta-schema | tool reads it to decide which ops/features a shell exposes + the advisory `AuthDecision`; CI validates well-formedness; **source for the CODEOWNERS seed** | Seeded by bootstrap; changes via PR, CODEOWNERS-protected |
| `roles.toml` | **Role bindings** (ADR-034): `operator-id` / IdP-team → roles | roles declared in `manifest.toml` | CI cross-checks roles exist in the manifest; drives advisory authz + the **non-GitHub/`file://`** path | PR-reviewed (auditable), CODEOWNERS-protected |

## C. Host enforcement — `.github/` + repo settings (the teeth)

| Thing | What it does | Governed by | Enforced / checked by | Administered by |
|---|---|---|---|---|
| `.github/CODEOWNERS` | Which paths require which reviewers (`registry.csv` deletes, `contract.json`, `manifest.toml` → `@org/qms-approvers`) | GitHub CODEOWNERS format; **derived from `manifest.toml`** | **GitHub branch protection** (required code-owner review) — the *authoritative* authz gate; consistency check vs the manifest (drift) | Generated/regenerated from the manifest by bootstrap / GitHub App |
| `.github/workflows/pr-check.yml` | Runs `pr check --diff` (ADR-016): validate-registry + validate-diff + classify + policy-decision; posts the check | the tool's check contract (exit codes) | GitHub Actions; **branch protection requires it green to merge** | Seeded by bootstrap; pins/fetches the `pr` binary |
| `.github/workflows/pages.yml` | Deploys the web view (ADR-013) | — | Actions | bootstrap |
| **Branch protection** *(repo setting, not a file)* | Require PRs (no direct `main`), require the check + required reviews | GitHub | GitHub itself | **Set by bootstrap / GitHub App via API** — config that can drift; must be self-audited (see below) |

## D. Docs / housekeeping
`README.md` (what this registry is) · `CONTRIBUTING.md` (PR-driven mutation model, "no direct main") · `.gitignore` (ignore generated label SVGs). Seeded by bootstrap; convention-governed.

## E. Operator-side (local, NOT in the data repo)
`~/.config/part-registry/registries.toml` — workspace (name → locator + identity, ADR-033 §5) · `~/.config/part-registry/github-token.json` — cached token (ADR-020). Local to each operator.

## The contracts, and where the teeth are

| Contract | What it governs | Where it's checked |
|---|---|---|
| **Schema/data** (`contract.json`) | what a record *is* | tool validators (CI + preflight) |
| **Manifest** (`manifest.toml` + `roles.toml`) | what the registry *exposes/allows* | tool reads (advisory); seeds CODEOWNERS |
| **Port traits** (code, not in data repo) | capability interfaces | compile-time + ADR-027 conformance |
| **Host enforcement** (CODEOWNERS + branch protection + check) | the actual merge gate | GitHub |

**Three enforcement layers:** ① tool validators classify + advise (CI `pr check` + FE/CLI preflight) → ② GitHub enforces (branch protection + CODEOWNERS + required check) → ③ on `file://` there is no ②, so it is **local-trust + advisory only** (ADR-034's stated asymmetry).

**Cross-file consistency checks:** `contract.json` ↔ `registry.csv` (validators) · `registry.csv` ↔ `print_log`/`audit_log` (FK) · `manifest` roles ↔ `roles.toml` ↔ `CODEOWNERS` (drift) · `contract.schema_version` ↔ tool `[min,max]` (compat) · `snapshot_hash` (reproducibility).

## Admin model

- **bootstrap / GitHub App** seeds every file **and applies branch protection + generates CODEOWNERS**. Without that step the host has no teeth — a registry isn't "correctly deployed."
- **Operators** mutate only via PR (`mint`/`bind`/`void`).
- **Approvers** (CODEOWNERS) gate sensitive paths; **schema/manifest changes** are themselves CODEOWNERS-protected PRs.

## Known gap — protection drift

Branch protection is a repo **setting**, not a committed file, so it can be changed out-of-band and drift silently. Mitigation (tracked as an ADR-034 open item): a `pr check` / GitHub-App **self-audit** that verifies a registry's branch protection + CODEOWNERS still match its manifest, and flags drift.
