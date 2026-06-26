# qx — Roadmap (0.x → 1.0)

**Status:** living plan (2026-06-26). Sequencing doc, not an ADR. Ties
the open ADRs / obligations / issues into an ordered path. Supersedes
the ad-hoc, mostly-closed milestones (Foundation / Web App Phase 2 / …)
as the *forward* plan.

> The project is at **~v0.9.x** today. This roadmap describes the **0.x
> trajectory** (engine + parts + new FE + governance maturing) and
> reserves **1.0 for the validation + e-signature bar** — not a feature
> tag.

---

## 0. Vision

**qx is a generic, id-and-contract-driven entity engine on a git/PR
backbone, where one hash-pinned gate validates every change — local, CI,
and every shell alike.** That makes it, concretely, a **git/GitHub-native
eQMS — local-first, governed by the PR you already trust.** What no
current eQMS (SaaS: Greenlight Guru, MasterControl, Qualio, …) offers:

- **Your records *are* a git repo** — NDJSON + a declared contract, in
  *your* repo. Sovereign, exportable, vendor never custodies them.
- **The PR *is* the controlled-change record** — branch to draft, review
  by diff, approve via CODEOWNERS, and the **merge is the accountable
  act**. Real version control, real diffs, real review.
- **Local-first** — the full engine runs offline (CLI/desktop); the gate
  runs *by hash*; no server in the trust path.
- **One generic fabric; QMS angles are declared presets** — parts,
  companies, personas, SOPs, orders, CAPAs, trainings — data, not
  bespoke modules ([[ADR-040-presets-as-declared-library-gate-enforced-floors]]).
- **Audit-grade by construction** — append-only audit + anchor ledger +
  persona-attributed signed commits (post-identification today; the
  e-signature *ceremony* at 1.0, see [[NOTE-identity-audit-posture-0x]]).

`parts` is preset #1 (the most-developed, regulated one). A full eQMS is
the *same engine* with more presets composed in.

**How first-class entries & links work — `parts` ↔ `companies`.** Both
are just **declared collections** in the contract (first-class entity
types); the engine special-cases neither. Each entry carries a **typed
id** (`part:…`, `company:…`) and they **link by id** — a part declares
`manufacturer → companies` (a typed reference the gate enforces: you
can't bind a part to a company that doesn't exist). You add a domain by
**declaring a preset** (`schema/presets/<x>.contract.json`, seeded via
`qx init --preset`), never by writing Rust. The *only* difference between
`parts` and `companies` is **floor markers**: `parts` carries them (the
regulated tier-1 minimum a deployer may extend but not weaken,
gate-enforced); `companies` doesn't (editable content). Same mechanism,
different declaration — which is the entire "code-owned vs example"
question, dissolved. A "company HQ" registry is just
`parts + companies + personas + …` composed, all linked by id, all under
the one gate.

---

## 1. What the tags mean

- **0.x** (now → through the build): the engine becomes generic and
  regulated-*capable*; parts (and companies/personas as library presets)
  run in production on the new FE; governance is **declared and
  gate-enforced**; identity posture = **attribution + integrity** (Part
  11 §11.10-ready), explicitly **not** an e-signature system.
- **1.0** (the gate): the **validation + e-signature bar** — rung **E1**
  in-record signing (meaning-tagged, bound to the merge hash,
  gate-verified) + the validation dossier (VP/VSR, traceability matrix
  export, SOUP harnesses green). Only then does a defensible Part 11
  §11.50/§11.70/§11.200 / Class-B-*validated* claim hold.

This split is the whole point: ship a maturing, honest 0.x; reserve the
compliance claim for one well-fenced 1.0.

---

## 2. Current state snapshot (~v0.9.x, 2026-06-26)

| Layer | State |
|---|---|
| **CLI / `qx`** | Done-ish: 14 subcommands (`mint/bind/label/print/list/resolve/describe/count/export/whoami/check/init` + gated `serve/mcp/tui`). |
| **Command protocol** (`qx-app::dispatch`) | Real: `Resolve, List, Count, Describe, Create, Edit, Transition, Print, Export, PollProposal, Whoami`. **Missing: a mint-events / audit-query op.** |
| **Contract engine (ADR-039)** | Parser + structural rules exist; canonical-form rewrite, meta-schema, conformance corpus, `$ref` object-schema = pending (#204). |
| **Data model (ADR-035)** | Largest pending cluster: typed ids, declared relations, entity store, attachments, lifecycle timestamps, **batch retirement**. JSONL adapter exists, not primary. |
| **Presets** | `parts` is code-owned (`parts_descriptor()`); `qx init` seeds a `parts+companies+contacts` contract. **Target: a declared preset library + gate-enforced floors (ADR-040).** |
| **Governance (ADR-016/033/034/036/037)** | Gate code-side exists; manifest/roles schema, personas, append-only diff rule, `qx verify --anchors`, canary = pending. ADR-036/037 still **Proposed**. |
| **Front end** | `web/` (vanilla TS, ADR-014) = feature-rich + e2e-covered but slated for retirement. `webapp/` (React over the protocol, ADR-030 §3) = read-capable scaffold (Grid/Detail/Count/Print + mock/http/wasm transports), **no write path**. |
| **Release (ADR-024/025/028/038)** | install.sh + runner image + anchor/bundle templates shipped; reproducible signed releases, vendored gate, distribution integrity = pending. ADR-038 still **Proposed**. |

---

## 3. Epics (the 0.x build → 1.0 gate)

Letters = sequence/dependency. Each lists exit criteria, the ADRs/
obligations it discharges, and issues it absorbs or needs created.

### M-A — Contract engine keystone (ADR-039) · gates everything
- **Exit:** canonical `contract.json` (fixed scalar set); meta-schema;
  `validate_record` enforces every §2 facet incl. `pattern` (done) and
  `$ref` object-schema; commit-resolved effective-dating; native ⇄ wasm
  ⇄ FE-vitest conformance corpus green. **Includes the wasm packaging +
  parity harness** (don't treat it as one bullet).
- **Discharges:** `contract-canonical-form`, `contract-ssot-validation`,
  `effective-dated-versioning`.
- **Issues:** #204, #191, #192, #194.
- **MVP cut (M-A.1):** scalar set + meta-schema + flat validation +
  native/wasm parity. Defer `$ref` + commit-resolution edge policy to
  M-A.2 so downstream unblocks weeks earlier.

### M-B — Data model, collections metamodel & **preset library** (ADR-035 + ADR-040)
- **Exit:** typed ids `(scheme,value)`; declared relations + derived
  backlinks (`components`/assembly, referential + acyclicity); entity
  store over `collections/<name>.jsonl` (JSONL **primary**, CSV
  export-only); attachments (object shape, not bare string); lifecycle
  timestamps; **`batch` retired** (validators/CLI/print/FE; legacy `B-*`
  migrated); **`list-mint-events` op exists**; **`stage_to_proposal`
  lifted into wasm (#194)**.
- **Preset library (ADR-040):** `schema/presets/*.contract.json` catalog
  (parts/companies/personas/orders/sops/…); `qx init --preset a,b,c`
  composition; floor = declared marker + gate "no-weaken" rule; **`parts`
  migrates from `parts_descriptor()` into the library** (after the
  floor-enforcement spike passes).
- **Discharges:** the ADR-035 cluster incl. `batch-deprecated`,
  `typed-ids`, `declared-relations`, `entity-store`, `jsonl-storage`,
  `attachments-content-addressed`, `lifecycle-timestamps`,
  `collections-metamodel`, `export-never-committed`.
- **Issues:** #190, #193 (Dolt — *1.x, designed here*). **New:** "retire
  `batch`", "add `list-mint-events`", "preset library + composable init
  (ADR-040)", "floor-enforcement-via-gate spike".

### M-C — Governance, authz & audit integrity (ADR-016/033/034/036/037)
- **Prereq:** **accept ADR-036 + ADR-037** (still Proposed) before build.
- **Exit:** self-describing registry (`.qx/{contract,manifest,roles}`);
  `qx check` consumes the manifest; CODEOWNERS ⊆ `personas`; append-only
  audit diff rule; `qx verify --anchors` offline; merge-sync witness;
  canary; the **identity posture** of [[NOTE-identity-audit-posture-0x]]
  on the record (attribution+integrity; NOT e-sig).
- **Discharges:** `registry-self-describing`, `core-plus-custom-schema`,
  `operator-workspace`, `registry-manifest`, `capability-grain`,
  `host-enforced-authz`, `pr-diff-policy-gate`,
  `audit-append-only-gate-rule`, `merge-sync-witness`,
  `pr-verify-offline`, `personas-collection`, `pr-is-truth-write-path`.
- **Issues:** #195 (Part 11 control matrix — *scopes the 1.0 claim*),
  #199 (signed payloads). **New:** "qx-provisioner App or a lighter
  canary substitute".
- **Note:** `personas-collection` depends on M-B's typed-ids/collections
  — it is **not** fully parallel to M-B.

### M-D — Front-end port `web/` → `webapp/` (ADR-030 §3) — **split read/write**
- **M-D-read (now):** Grid/Detail/Count/Print parity over already-stable
  ops. Safe before M-A/M-B.
- **M-D-write (after M-A/M-B + the write-path spike):** Bind/mint/edit/
  void/submit, assembly, typed fields, mint-events view, CSV/OCR.
- **Open architecture fork (spike):** the wasm transport cannot do
  networked PRs and `transport_github_pr` is `#[cfg(not wasm)]` — so
  "live submit" forks into (a) a browser-fetch `ProposalSink` in wasm
  (keeps static-Pages serverless), (b) require `qx serve` (kills
  serverless writes), or (c) hosted broker (1.x). **Spike this against
  the parts user stories before M-D-write.** Same spike resolves OAuth
  device-flow-from-static-SPA (CORS) vs keeping PAT.
- **Exit:** punch-list §4 lands on `webapp/`; e2e parity; live submit via
  the chosen transport; **`web/` retired via a separate strangler gate**
  (both apps deployed side-by-side ≥ N PRs, then delete — not an M-D
  exit line).
- **Discharges:** `spoke-feature-parity` (ADR-030 §8 FEATURE-MATRIX),
  `auth-credential-resolver` (#020 — lift to its own mini-epic; no
  contract dep, can start parallel to M-A).
- **Issues:** #194, #163, #164–167, #180–182, #211. **Re-home to here,
  do not fix in `web/`.**

### M-E — Release engineering & validation dossier (ADR-024/025/028/038)
- **Prereq:** **accept ADR-038** (still Proposed) before build.
- **0.x-shippable subset:** reproducible signed releases + curl-able
  install.sh pin-by-hash (already mostly satisfied). **Defer to 1.x/1.0:**
  vendored gate + musl static + host-independent-CI-as-derivations +
  canary, and the full **validation dossier** (VP/VSR, traceability
  export via the `coverage-joiner` #029).
- **Issues / unmapped obligations to home:** `coverage-joiner` (#029),
  `print-fold-audit-spine`, `crypto-reopen-triggers-watched`.

### M-F — Dogfood the eQMS spine — **1.1, not 0.x**
- `documents` + `personas` + `trainings` presets composed into this
  repo's own contract; QM-001/QSP-001/002/SOP-PART-001 as records.
- **Why 1.1:** `trainings` ack is e-sig territory — land it *with* rung
  E1 (1.0), not before. For a 0.x dogfood signal, note that `decisions/`
  + `obligations.toml` already function as a proto-QMS (zero new work).
- **Issue:** #208 (preset family).

---

## 4. Port punch-list — `web/` → `webapp/` (with `batch` retired)

Verdict: **CARRY** (re-land ~as-is) · **RESHAPE** (model changes) ·
**DROP→X**. "Engine op needed first": ✓ = in protocol, **bold** = pending.

| `web/` feature | Verdict | `webapp/` target | Engine op needed first |
|---|---|---|---|
| Lookup grid, fuzzy search, deep-link `/<ID>` | CARRY (M-D-read) | `GridPage` | `List` ✓, `Resolve` ✓ |
| Status/column filters, sortable | CARRY (M-D-read) | `GridPage` filters | `List{filter,sort}` ✓ |
| Detail card + inline edit | CARRY | `DetailPage`/`EntityDetail` | `Resolve` ✓, `Edit` ✓ |
| **Mint a "batch" → print plan/export** | **RESHAPE** | new `MintPage` | `Create{n}` ✓, `Export` ✓ — **drop the `B-…` handle**; grouping = the mint event |
| **Batch picker / `registry.batches()`** | **DROP → mint-events view** | new `MintEventsPage` | **`list-mint-events`** (NEW op, ADR-035 §0) |
| Print / label studio | CARRY (M-D-read) | `PrintPage` | `Print` ✓ |
| Bind/edit/void → queue → **submit one PR** | RESHAPE (rewire) | `BindPage` + submit | `Transition` ✓, `Edit` ✓; **write path: `stage_to_proposal` wasm (#194)** + `PollProposal` ✓ + the **write-path fork (M-D spike)** |
| CSV/TSV import + mapping | CARRY | new `ImportPage` | batched `Create`/`Edit`/`Transition` |
| Assembly / BOM | CARRY | extend Detail/Bind | `Create`+`Transition` over **declared `components` relation (M-B)** |
| Typed bind fields per `kind` | CARRY | `BindPage` | `Describe` ✓ — needs **contract `typeFields` (M-A)** |
| Manufacturer-id + metadata | CARRY | Detail/Grid | `List{filter}` ✓ + **contract `properties` (M-A)** |
| OCR scan/extract/mint-from-label | CARRY (P2) | scan overlay | client-side; `Create` ✓ |
| QR scan → Bind/Lookup | CARRY | scanner | client-side; `Resolve` ✓ |
| GitHub PAT auth | **RESHAPE** | transport auth | `Whoami` ✓ + **OAuth/CORS decision (M-D spike); `auth-credential-resolver` #020** |
| PWA/offline + SW token enclave | CARRY | webapp PWA + #163 | n/a |
| Deploy-time config | CARRY | webapp config | n/a |
| Session/IndexedDB queue + recovery | RESHAPE | webapp `data/` | client-side; stage→Diff via #194 |
| **Live PR submit (browser REST dance)** | **RESHAPE — move into engine** | transport | **#194 + write-path fork** |

**`batch` removal checklist:** validators `REQUIRED_ALWAYS` · `--batch`
CLI flag · print events · `web/` pickers (don't port) · build
`MintEventsPage` over `list-mint-events` · migrate legacy `B-*`
(spec the key + collision rule first) · docs/examples.

---

## 5. Critical path & sequencing (corrected)

```
        M-A Contract engine (ADR-039) ◄── keystone (do M-A.1 first)
          │
   ┌──────┼─────────────────────────────┐
   ▼      ▼                              ▼
 M-B-core (typed-ids + collections      M-D-read (Grid/Detail/Print
 + entity-store)  ──► M-B-rest          over stable ops — safe now)
   │  (relations, batch retire,           │
   │   preset library, list-mint-events)  │
   ▼                                      │
 M-C Governance (needs M-B typed         │
 personas; accept ADR-036/037 first)     │
   │                                      │
   └──────────────┬───────────────────────┘
                  ▼
         M-D-write (write-path spike resolved → #194,
         OAuth, batch-retired engine, append-only gate)
                  │
                  ▼
   strangler gate → retire web/    +   M-E-light (parallel from M-A)
                  │
                  ▼
        0.x feature-complete  ─────────►  1.0 (E1 e-sig + dossier)
                                          + M-F spine dogfood (1.1)
```

- **M-A.1 first; it gates everything.** `auth-credential-resolver` and
  M-E-light have no contract dep → start in parallel.
- **M-C is not parallel to M-B** for `personas` (needs typed-ids).
- **M-D-read now; M-D-write after the write-path spike + M-A/M-B.**
- **`web/` deletion is a separate ratification gate, not an M-D exit.**

---

## 6. Open decisions — tracked, not blocking (file as issues/spikes)

From the pre-implementation review. Strategic forks (your call, tracked):
1. **Reg claim scope (#195):** 0.x = attribution+integrity (Part 11
   §11.10-ready), NOT e-sig; 1.0 = E1 + dossier. *(NOTE-identity-audit-posture-0x.)*
2. **FE write-path fork:** spike wasm-fetch-sink vs `qx serve` vs broker,
   against the parts user stories (M-D).
3. **Preset library / floor-as-gate-rule (ADR-040):** spike
   floor-enforcement-via-gate before retiring `parts_descriptor()`.
4. **`companies` = library preset content** (not a code floor) for 0.x.

Spec-level (resolve inside the relevant epic):
- `qx check` commit-resolved versioning leaks on **contract-shape
  changes** (collection rename / ref-target / enum-tightening on
  *untouched* records) — define the policy in M-A.
- `manifest.toml`/`roles.toml` schema + **personas genesis**
  (chicken-and-egg) — M-C.
- **attachment-as-string vs object** contradiction (code vs ADR-035 §4) — M-B.
- `list-mint-events` **key/collision shape** + legacy `B-*` migration
  mechanism — M-B.
- **append-only audit rule** byte-level definition — M-C.
- **OAuth device-flow CORS** from a static SPA — M-D spike.

Unmapped accepted obligations to home: `coverage-joiner` (#029, → M-E),
`print-fold-audit-spine` (→ M-B), `crypto-reopen-triggers-watched` (→ M-E),
`qx-provisioner` App / canary substitute (→ M-C).

---

## 7. Issue / milestone actions

1. Create a **`0.x` milestone** (and a future **`1.0` gate** milestone)
   + epic issues M-A…M-F linking the issues in §3.
2. File the **spikes**: preset-library/floor-enforcement (ADR-040),
   FE write-path, and (existing) #204 contract engine.
3. File new engine-op issues: "retire `batch`", "add `list-mint-events`".
4. **Re-home FE issues to M-D** (don't fix in `web/`): #163–167, #180–182.
5. Tag **1.x/deferred:** #183, #193, #199 (unless pulled into M-C), e-sig
   E2–E4, scheduler, Typst→PDF, GitHub-App hosting.
6. **Accept ADR-036/037/038** (move Proposed → Accepted) before M-C/M-E.
7. Ratify ADR-040 (after the floor spike) and migrate `parts` into the
   preset library.

---

## 8. 1.0 gate — Definition of Done (the validation/e-sig bar)

- [ ] 0.x feature-complete: generic engine + parts (+ companies/personas
      as library presets) in production on `webapp/`; `web/` retired.
- [ ] **Preset library** live; floors declared + gate-enforced; `parts`
      migrated out of code (ADR-040 accepted, spike passed).
- [ ] `batch` retired end-to-end.
- [ ] Self-describing registry + manifest + roles; append-only audit
      gate + `qx verify --anchors`.
- [ ] **Rung E1 e-signature** (in-record, meaning-tagged, gate-verified).
- [ ] **Validation dossier**: VP/VSR, Part 11 traceability matrix export
      (`coverage-joiner`), SOUP harnesses green, reproducible signed
      releases + vendored gate by hash.
- [ ] Bootstrap composes the spine = IQ/OQ baseline; this repo dogfoods
      `documents`/`personas`/`trainings` (M-F).
- [ ] No `pending` obligations on the 1.0 critical path.
