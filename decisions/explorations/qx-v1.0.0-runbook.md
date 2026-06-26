# qx v1.0.0 — Runbook

**Status:** living plan (created 2026-06-26). Not an ADR — a sequencing
document that ties the open ADRs, obligations, and issues into one
ordered path to a `v1.0.0` tag. Supersedes the ad-hoc, mostly-closed
milestones (Foundation / Rust Hardening / Web App Phase 2 / …) as the
*forward* plan; those remain the history.

> Naming note: the project is now **qx** (was part-registry / "HQ").
> Dated ADRs/explorations keep the old names — historical record.

---

## 1. What `v1.0.0` means (definition of done)

`v1.0.0` is the first tag where **the generic engine is in production
with parts + companies as the proving instance, regulated-grade, on the
new front end, in a repo that passes its own gate and can bootstrap a
fresh "company HQ."** Concretely, all of:

1. **Engine is generic, not parts-special.** The contract engine
   (ADR-039) is the SSOT: one Rust core drives FE form-gen, preflight,
   record validation, and the `qx check` gate. The tiered data model +
   collections metamodel (ADR-035) is real: typed ids, declared
   relations, one audit stream, JSONL primary. **`batch` is retired.**
2. **Two presets live in production:** `parts` and `companies`, as
   *declared collections*, not code.
3. **Governance is regulated-grade.** Self-describing registry (ADR-033)
   + manifest/capabilities (ADR-034) + audit-trail integrity & anchor
   ledger (ADR-037) + the PR-diff gate (ADR-016). A deployed registry
   enforces append-only audit and host-witnessed authorship.
4. **The new front end (`webapp/`) is at feature parity** with today's
   `web/` for the parts workflow, running over `qx-app::dispatch` via the
   wasm / `qx serve` transports — one artifact, three targets
   (browser / Tauri desktop / mobile). `web/` is retired.
5. **The repo is self-validating and deployable.** Reproducible signed
   releases (ADR-024), distribution integrity (ADR-025), vendored gate +
   host-independent CI (ADR-038), SOUP (ADR-028). `qx init` scaffolds a
   repo whose CI runs the *released* gate by hash; the bootstrap brings
   up a "company HQ" spine = its IQ/OQ baseline.
6. **Dogfood proof:** this repo carries its own `documents` + `personas`
   + `trainings` collections (the eQMS spine) as *records* — zero engine
   change — proving the fabric is a QMS, not a parts tool (#208 spine
   subset; the full external preset family polish is **1.x**).

**Out of scope for 1.0 (explicitly 1.x / deferred):** e-signature rungs
E2–E4 (WebAuthn/PAdES/QES), the scheduler (periodic review / expiry),
Typst→PDF, alternative storage adapters (Dolt #193, SQLite), GitHub-App
hosting, LLM label extraction (#183). All trigger-gated, none on the
critical path.

---

## 2. Current state snapshot (2026-06-26)

| Layer | State |
|---|---|
| **CLI / `qx` binary** | Done-ish. 14 subcommands work (`mint/bind/label/print/list/resolve/describe/count/export/whoami/check/init` + gated `serve/mcp/tui`). |
| **Command protocol** (`qx-app::dispatch`) | Real. `Request` = `Resolve, List, Count, Describe, Create, Edit, Transition, Print, Export, PollProposal, Whoami`. **Missing: a mint-events / audit-query op.** |
| **Contract engine (ADR-039)** | Parser + structural rules exist (`crates/contract`); canonical scalar-set rewrite, meta-schema, conformance corpus, `pattern` regex + `$ref` object-schema = **pending** (#204, tasks #17–20). |
| **Data model (ADR-035)** | Largest pending cluster (~15 obligations): typed ids, declared relations, entity store, attachments, lifecycle timestamps, **batch retirement**. JSONL adapter exists but not primary. |
| **Governance (ADR-016/033/034/037)** | Gate code-side exists; `manifest.toml`/`roles.toml`, personas, append-only diff rule, `qx verify --anchors`, canary = **pending**. |
| **Front end** | `web/` (vanilla TS, ADR-014) = feature-rich + e2e-covered but **slated for retirement**. `webapp/` (React+Tailwind over the protocol, ADR-030 §3) = read-capable scaffold (Grid/Detail/Count/Print pages, mock/http/wasm transports), **no write path**. |
| **Release / dossier (ADR-024/025/028/038)** | install.sh + runner image + anchor/bundle templates shipped; reproducible signed releases, vendored gate, distribution integrity = **pending**. |

---

## 3. Milestones (gaps 1–5 → ordered epics)

Six epics. Letters denote sequence/dependency, not priority. Each lists
its **exit criteria**, the **ADRs/obligations** it discharges, and the
**issues** it absorbs or needs created.

### M-A — Contract engine keystone (ADR-039) · *gap 1*
The single most load-bearing piece; everything downstream reads it.
- **Exit:** canonical `contract.json` with the fixed scalar set; meta-schema; `validate_record` enforces every §2 facet incl. **`pattern` regex** and **`$ref` object-schema**; effective-dated (commit-resolved) versioning proven by `qx check --base`; native ⇄ wasm ⇄ FE-vitest conformance corpus passes (parity gate).
- **Discharges:** `contract-canonical-form`, `contract-ssot-validation`, `effective-dated-versioning`.
- **Issues:** #204 (spike → engine), #192 (container routing into Diff), #191 (de-string-ify `Diff::classify`), #194 (lift `stage_to_proposal` into wasm — shared with M-D).
- **Blocks:** M-B, M-D (form-gen/preflight), M-C (gate semantics).

### M-B — Tiered data model & collections metamodel (ADR-035) · *gap 2*
- **Exit:** global typed ids `(scheme,value)`; declared relations + derived backlinks (the `components`/assembly graph with referential + acyclicity integrity); one entity store over `collections/<name>.jsonl` (JSONL **primary**, CSV export-only); content-addressed attachments; lifecycle timestamps (`created_at`/`transitioned_at[…]`); **`batch` retired from core** (validators `REQUIRED_ALWAYS`, mint/label CLI, print events, web pickers; legacy `B-*` migrated); the **`list-mint-events`** derived op exists.
- **Discharges:** the ADR-035 obligation cluster incl. **`batch-deprecated`**, `typed-ids`, `declared-relations`, `entity-store`, `jsonl-storage`, `attachments-content-addressed`, `lifecycle-timestamps`, `collections-metamodel`, `export-never-committed`.
- **Issues:** #190 (ADR-027 tier-3 parity re-scope), #193 (Dolt parity partner — *1.x but designed here*).
- **New issue to create:** **"Retire `batch` from core (ADR-035 §0)"** — see the port punch-list §4; and **"Add `list-mint-events` op to the command protocol."**

### M-C — Governance, authz & audit integrity (ADR-016/033/034/037) · *gap 3*
- **Exit:** registry is self-describing (`.qx/contract.json` + `manifest.toml` + `roles.toml`); `qx check` consumes the manifest; CODEOWNERS ⊆ `personas`; append-only audit diff rule enforced; `qx verify --anchors` offline; merge-sync witness; canary pipeline; Part 11 / ALCOA / GAMP 5 controls.
- **Discharges:** `registry-self-describing`, `core-plus-custom-schema`, `operator-workspace`, `registry-manifest`, `capability-grain`, `host-enforced-authz`, `pr-diff-policy-gate`, `audit-append-only-gate-rule`, `merge-sync-witness`, `pr-verify-offline`, `personas-collection`, `pr-is-truth-write-path`.
- **Issues:** #195 (Part 11 / ALCOA / GAMP 5 controls), #199 (signed id payloads — HMAC tail), #189 follow-ups as relevant.

### M-D — Front-end port `web/` → `webapp/` (ADR-030 §3) · *gap 4*
The strangler-fig cutover. Full feature mapping in **§4**. Depends on
M-A (wasm validate/form-gen) and the protocol ops, incl. M-B's
`list-mint-events` and the wasm write path (#194).
- **Exit:** every "carry/reshape" row in §4 lands on `webapp/` over the protocol; e2e parity (port the `web/tests/e2e/*` suite); the live PR submit flow runs through the wasm/serve transport (FE stops doing the GitHub REST dance directly); `web/` deleted; Tauri desktop ships `webapp/dist`.
- **Discharges:** `spoke-feature-parity` (the ADR-030 FEATURE-MATRIX gate), `auth-credential-resolver` (#020).
- **Issues:** #194 (write path into wasm), #163 (submit via SW token enclave), #164–167 (auth modal a11y/UX — re-land in webapp, not web), #181/#180/#182 (mint-from-label extraction — re-land), #211 (QR M3-L — codec, feeds print).

### M-E — Release engineering & validation dossier (ADR-024/025/028/038) · *gap 5 (deferred bucket, the shippable subset)*
- **Exit:** reproducible signed releases; distribution integrity (install.sh pins by hash); vendored gate + musl static build; host-independent CI as derivations; SOUP inventory current; `qx init`'d repos run the *released* gate by hash; bootstrap = IQ/OQ baseline.
- **Discharges:** `reproducible-signed-releases`, `distribution-integrity`, `gate-vendored`, `upgrade-succession`, `per-op-floors`, `crypto-reopen-triggers-watched`.

### M-F — Dogfood the eQMS spine (#208 subset) · *the 1.0 proof, post M-B/M-C*
- **Exit:** this repo's own contract gains `documents` + `personas` + `trainings` collections; QM-001 / QSP-001/002 / SOP-PART-001 exist as **records** (zero engine change); the gate governs them.
- **Issues:** #208 (the H2 preset family — *1.0 takes the spine subset; full external preset polish is 1.x*).

---

## 4. Port punch-list — `web/` → `webapp/` (with `batch` retired)

Verdict legend: **CARRY** (re-land ~as-is) · **RESHAPE** (re-land, but
the model changes — usually `batch` → mint-event) · **DROP→X** (remove,
replaced by X). "Engine op needed first" lists the `dispatch` op(s) the
webapp feature consumes; ✓ = already in the protocol, **bold** = pending.

| `web/` feature | Verdict | `webapp/` target | Engine op needed first |
|---|---|---|---|
| Lookup grid, fuzzy search, deep-link `/<ID>` | CARRY | `GridPage` (exists) + search | `List` ✓, `Resolve` ✓ (search client-side) |
| Status / column filters, sortable headers | CARRY | `GridPage` filters | `List{filter,sort}` ✓ |
| Detail card + inline edit | CARRY | `DetailPage`/`EntityDetail` (exist) | `Resolve` ✓, `Edit` ✓ |
| **Mint a "batch" → print plan / export** | **RESHAPE** | new `MintPage` | `Create{n}` ✓, `Export` ✓ — **drop the `B-…` handle**; grouping = the mint event |
| **Batch picker / `registry.batches()`** | **DROP → mint-events view** | new `MintEventsPage` | **`list-mint-events`** (NEW op — ADR-035 §0; derived: distinct `created_at` + count + operator) |
| Print / label studio (composer, layouts, payload formats, paper) | CARRY | `PrintPage` (exists) | `Print` ✓ |
| Bind / edit / void → local queue → **submit as one PR** | RESHAPE (rewire) | `BindPage` (exists) + submit | `Transition` ✓ (bind/void), `Edit` ✓; **write path: `stage_to_proposal` in wasm (#194)** + `PollProposal` ✓ + OAuth transport |
| CSV / TSV bulk import + column mapping | CARRY | new `ImportPage` | `Create`/`Edit`/`Transition` batched via dispatch (mapping client-side) |
| Assembly / BOM (mint+bind, `[N]` badge, reverse lookup) | CARRY | extend Detail/Bind | `Create`+`Transition` over **declared `components` relation (M-B)** |
| Typed bind fields per part `kind` | CARRY | `BindPage` type fields | `Describe` ✓ — needs **ADR-039 contract `typeFields` (M-A)** |
| Manufacturer-id + JSON metadata | CARRY | Detail / Grid filter | `List{filter}` ✓ + **contract `properties` (M-A)** |
| OCR text-scan / extract / mint-from-label | CARRY (P2) | scan overlay | client-side (tesseract); `Create` ✓ for mint-from-label |
| QR scan → Bind / Lookup | CARRY | scanner in Bind/Grid | client-side decode; `Resolve` ✓ |
| GitHub PAT auth modal | **RESHAPE** | transport auth (OAuth device flow) | `Whoami` ✓ + **OAuth hand-off in wasm transport (`auth-credential-resolver`, #020)** |
| PWA / offline + SW token enclave | CARRY | webapp PWA + #163 | n/a (re-land #163–167 here) |
| Deploy-time config (title, tabs, allowed code types) | CARRY | webapp config | n/a |
| Session / IndexedDB queue + crash recovery | RESHAPE | webapp `data/` + session | client-side; stage→Diff via **#194** |
| **Live PR submit (browser does the GitHub REST dance)** | **RESHAPE — move into engine** | transport | **`stage_to_proposal` wasm (#194)** + PR `ProposalSink` transport; FE no longer hand-rolls REST |

**Net new engine work the port forces (do in M-A/M-B before M-D write path):**
1. **`list-mint-events`** op (replaces `list-batches`; the batch-picker successor).
2. **`stage_to_proposal` in the wasm core** (#194) — Session.items → Diff → Proposal, so every shell submits identically.
3. **OAuth credential resolver in the transport** (#020) — replaces the browser-only PAT modal.
4. **Declared `components` relation** (M-B) — assembly/BOM parity.
5. **Contract `typeFields`/`properties`** (M-A) — typed bind fields + metadata.

**`batch` removal checklist (folds the `batch-deprecated` obligation into the port):**
- [ ] validators: drop `batch` from `REQUIRED_ALWAYS` header.
- [ ] CLI: remove `--batch` from `mint`/`label`; grouping via mint event.
- [ ] print events: stop carrying `batch`.
- [ ] `web/`: delete batch pickers / `registry.batches()` — do **not** port them.
- [ ] `webapp/`: build `MintEventsPage` over `list-mint-events` instead.
- [ ] migration: map legacy `B-*` handles → mint-event (created_at + audit ref).
- [ ] docs/examples: replace `--batch B-2026-05-…` usages.

---

## 5. Critical path & dependencies

```
            M-A Contract engine (ADR-039)  ◄── the keystone
             │
   ┌─────────┼───────────────┐
   ▼         ▼               ▼
 M-B Data   M-D read        M-C Governance
 model      parity          (manifest/audit
 (ADR-035)  (Grid/Detail     /gate, ADR-034/037)
 +batch     /Print over       │
 retire     protocol)         │
   │         │                │
   └────┬────┘                │
        ▼                     │
   M-D write path  ◄──────────┘  (stage→proposal #194, OAuth, append-only gate)
        │
        ▼
   M-F dogfood spine (#208 subset)  +  M-E release/dossier (parallelizable)
        │
        ▼
     v1.0.0  tag
```

- **M-A is the gate on everything.** Start here.
- **M-B and M-C can proceed in parallel after M-A**, but **M-D's write
  path needs both** (typed model + append-only gate + #194).
- **M-E (release/dossier) is parallelizable** from M-A onward — it's
  pipeline work, not feature work.
- **M-F is the last substantive step** (proves the QMS claim) and needs
  M-B (collections) + M-C (governance) live.

---

## 6. Issue / epic actions (what to create on GitHub)

The existing issues are real but unmilestoned and unstructured. To drive
this:

1. **Create a `v1.0.0` milestone** and 6 epic issues (M-A … M-F), each
   linking the issues listed in §3.
2. **File the two missing engine ops** as issues under M-B:
   "Retire `batch` from core (ADR-035 §0)" and "Add `list-mint-events`
   to the command protocol."
3. **Re-home FE issues to M-D** (don't fix them in `web/`): #163, #164,
   #165, #166, #167, #180, #181, #182 — these re-land on `webapp/`.
4. **Tag `1.x`/deferred** (off the v1.0 path): #183 (LLM extraction),
   #193 (Dolt), #199 (signed payloads — unless pulled into M-C), the
   e-sig rungs, scheduler, Typst→PDF.
5. Keep **#204 as the active head of M-A**; #208 as M-F (spine subset
   for 1.0, full family 1.x).

---

## 7. v1.0.0 Definition-of-Done checklist

- [ ] ADR-039 contract engine canonical + conformance corpus green (native/wasm/FE parity).
- [ ] ADR-035 data model live: typed ids, declared relations, entity store, JSONL primary, attachments, lifecycle timestamps.
- [ ] **`batch` retired** end-to-end (validators, CLI, print, FE; legacy migrated).
- [ ] `parts` + `companies` are declared collections in production.
- [ ] Self-describing registry + manifest + roles; `qx check` consumes them; append-only audit gate + `qx verify --anchors`.
- [ ] `webapp/` at parity for the parts workflow (port §4); e2e suite ported; live submit via wasm/serve transport; `web/` deleted.
- [ ] Reproducible signed releases + distribution integrity + vendored gate; `qx init` repos run the released gate by hash; bootstrap = IQ/OQ baseline.
- [ ] This repo dogfoods `documents`/`personas`/`trainings` as records (eQMS spine proof).
- [ ] All v1.0 obligations in `decisions/obligations.toml` are `met` (no `pending` on the critical path).
