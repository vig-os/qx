# Decision log — qx

Append-only chronological record of decisions for the parts registry.
Newest entries first.

## 2026-06-11 — Audit evidence: identity, trail integrity, gate lifecycle (ADR-036…038 Proposed)

**Context:** a long interactive session ("expand into a qx?") walked
the e-signature, audit-trail, anchoring, vendoring, upgrade and CI
questions to ground. The platform-expansion question itself stays
exploratory (`explorations/platform-vs-registry.md`); the decided slice
landed as three Proposed ADRs plus Corrections on 030/033/034.

| ADR | Decision (headline) |
|---|---|
| 036 | `personas` as a collection (operator = typed FK; CODEOWNERS ⊆ active personas; resolves ADR-035's deferred operators question); accountability resolves host-side at merge (accountable-approver default, author-strict knob); signed commits = authorship+integrity, not identity; elevation = deliberate act under 2FA-*enrolled* host-auth (explicitly NOT fresh-per-act MFA); PR-is-truth, `file://` writes demoted to flagged-future; deferred rungs E1–E4 (Sigstore in-record, WebAuthn-UV presence, PAdES, QES) with triggers; GDPR note on public logs |
| 037 | Per-entry `chain_hash` retired (redundant on-host; serializes concurrent PRs) → content_hash + gate append-only diff rule + **checkpoints** by the serialized anchor job; merge/review witness synced into the stream (durable vs API retention); producer = claim vs gate = evidence provenance (artifact sha + env digest recorded from the pin-verify step; pin is CODEOWNERS-gated); **anchor ledger = GitHub immutable releases** — per-push anchor + nightly heartbeat (silence unambiguous) + monthly bundle-if-changed + pre-audit; anchors only after `pr verify` green; ancestry/fast-forward rule = the tamper signal; freeze-line semantics stated honestly (window interior not proven); knob `anchor = releases \| +tsa \| +witness-org \| +rekor-public`; external watcher pulls bundles offline |
| 038 | Gate **vendored** in `.qx/gate/` (blob+sha+attestation+source+Nix recipe; no LFS; measured 0.8–1.6 MB zst/binary); availability ≠ trust (pin-verify-before-exec unchanged); history retains all versions (rotation = forbidden rewrite); in-repo beats user-side fork structurally (anchor seals the gate; atomic upgrade PR; one-bundle evidence); upgrades = incumbent validates succession + successor shadow-run + monotonicity; compat = metamodel parse floor + derived per-op floors (re-print survives, new mint doesn't — federated degradation, no cliff); CI = pipeline-as-derivation (Nix), runner image → ghcr, data-repo workflows are logic-free shims, blocking gate stays the vendored zero-network binary; curl-able `install.sh` bootstrap |
| 040 | **Proposed.** Presets are a declared **library** (`schema/presets/*.contract.json` — parts/companies/personas/orders/sops/capas/…), not Rust; the engine knows none by name. `qx init --preset a,b,c` **composes** them — a "company HQ" = a composition (= the spine). The non-weakenable **floor moves from compiled `parts_descriptor()` to a declared marker + a gate "no-weaken" diff rule** (gate runs by hash, contract changes CODEOWNERS-pinned) — refines/supersedes ADR-035 §0 guardrail #1 + ADR-039 §5 (which already admits the compiled floor is de-facto weakenable). `parts` becomes preset #1, not a special engine citizen. 0.x keeps the code floor; spike must prove gate-enforcement (incl. non-instantiation/rename-around bypass) before migration. See [[NOTE-identity-audit-posture-0x]] for the parallel 0.x identity posture |
| 041 | **Proposed.** Live authz **canary** = a command (`qx canary` / `just canary`), not bespoke workflow YAML — CI runs the same command a dev runs (the ADR-034/016 "one gate everywhere" applied to the gate's own test). Auth = a fine-grained **vig-os-owned admin PAT, SOPS-encrypted (age) in-repo**; the **age key is the ONLY secret** (`GH_AGE_KEY` in CI, dev keyring local). A run: provision an ephemeral `qx-canary-*` repo → golden-path (mint→PR→`qx check --diff`→merge→assert) + red-matrix (conformance corpus as REAL PRs, each blocked by the host not the tool) → `always()` teardown + orphan sweeper. Impurity boundary: offline logic is `flake check`-able, the live run is an explicit impure command CI shims. Green run = satisfied-evidence for pr-diff-policy-gate / protection-drift-selfaudit / host-enforced-authz / spoke-feature-parity. Two auth contexts kept distinct: (A) a tenant's eQMS = their OWN gh login (ADR-038 forking), (B) the canary = the vig-os SOPS'd PAT, throwaway repos only. `qx-provisioner` App demoted to deferred scale upgrade (ADR-030 §7). Refines ADR-034 §5-6; rewrites the `canary-pipeline` obligation. See [[deploy-canary-forking-architecture]] |

**Implementation landed with the ADRs:** `anchor.yml.tmpl` +
`bundle.yml.tmpl` (data-repo ledger workflows), bootstrap `--anchor`
seeding + `install.sh` curl entry, tool-repo `checks.obligations` flake
output + thin `ci.yml` shim, release.yml runner-image→ghcr job,
obligations rows for 036–038.

**Review note:** the session explicitly *corrected two of its own
earlier proposals* before filing — per-entry hash chaining (dropped for
checkpoints after the concurrency/redundancy review) and "rung-2 =
fresh 2FA" (weakened to enrolled+authenticated after checking what
OAuth can actually prove). Both corrections are recorded in the ADRs.

## 2026-06-11 — Design corpus completed + accepted (ADR-031…035; 030–035 → Accepted)

**Context:** continuation of the 2026-06-10 multi-tier session. Five
further ADRs were authored interactively, then the whole set was put
through a two-agent generalization review and a contradiction pass, and
flipped to Accepted.

**ADRs authored:**

| ADR | Decision (headline) |
|---|---|
| 031 | Label render + structured print request: px-true QR (`symbol_px % modules == 0`), device-dot sizing, padding-as-floor/job-fills-to-max, optimistic mint+print with pre-flight; later generalized to `Print{collection, Selection}` with named presets in the descriptor's render block |
| 032 | Scan pipeline: `FrameSource → decode-image → Resolve{id} → RollingAccumulator → Sink` — one processor, sources/sinks per shell; drop `zxing-wasm` after an rxing A/B gate |
| 033 | Registry anatomy: one repo = one registry; self-describing data repo (own versioned contract, `[min,max]` tool compat); scalar custom-field types + `attachment`; operator workspace (`registries.toml`) |
| 034 | Manifest + capabilities: host-enforced authz (branch protection + CODEOWNERS; tool classifies/advises); SSoT-core gate runs locally + in CI from a pinned artifact; manifest grain = op-family × collection × edge; CI-only protection-drift self-audit (no App yet) |
| 035 | Registry data model — the capstone: collections metamodel (one engine, descriptors, code-owned presets, one meta level); git-native NDJSON **entity store** (global typed ids `(scheme,value)`, micro-core, kind tree w/ inheritance, generic List/Count, no join DSL); declared relations + derived backlinks; JSONL primary / CSV export-only; `batch` **retired** (audit spine is the mint event); `minted_at`≡`created_at`, `bound_at`→`transitioned_at[bound]` under the materialization rule; print events folded into the ONE audit stream; content-addressed attachments |

**Generalization review (2026-06-11):** two fresh-context agents
(`a6db4773`, `a08dc250`) swept the corpus. Adopted (13): created_at
unification, lifecycle-timestamp materialization rule, `{collection,
op-kind}` unified change vocabulary (016/020/022), print-fold,
kind-as-descriptor-capability, collection-generic Repository port
(ADR-018 refinement note), uncommit `registry.csv`, manifest capability
grain, shared `Selection`/`Filter`, scan→`Resolve{id}`,
Transition-with-payload (bind/rebind dissolve), render-metadata
single-home, batches-roster regression scrub. Rejected (rightly):
printer-profiles-as-entities, logs-as-collections, stream-descriptor
meta-machinery. Deferred to open question: operators directory
collection; type-BOM vs as-built.

**Contradiction pass:** four wording-level contradictions found and
fixed (ADR-030 "one variant per operation" → op-families; ADR-031 §1
`mint_count` residue; ADR-032 diagram `resolve-part` → `Resolve{id}`;
ADR-034 "feature-flag" residue). No structural contradictions. Two
intentional asymmetries re-confirmed as documented-not-bugs: `file://`
local-trust enforcement; fail-open audit vs lenient materialization
cross-check.

**Acceptance:** ADR-030…035 flipped Proposed → Accepted (reviewer Lars
Gerchow, 2026-06-11). README index reconciled with file statuses —
ADR-016…029 had been Accepted in-file while the index said "none yet";
ADR-014 status corrected to "Superseded by ADR-030". Obligations
registry at 51 rows / 21 in-force ADRs, gate green throughout.

**Next:** implementation per ADR-030 build order — `crates/app`
(collection engine + protocol) first, then un-gate live PR submission.

## 2026-06-10 — Multi-tier shells over one application layer (ADR-030)

**Context:** the user asked for a multi-tier app design — CLI, TUI,
Tauri (desktop + mobile), local server ("to-localhost"), web, potential
mobile, and CI — interacting either with a local deployed part-registry
git folder or a GitHub part-registry repo, with every app "always fully
capable" and the ground truth + validation always in the Rust core.
Review of the existing ADR set showed ADR-017 already commits the
Rust-core/ports/adapters shape and names these surfaces, but (a) leaves
the frontend strategy as an explicit open question, (b) has no
application layer — `crates/cli` bins and `crates/wasm` each wire the
ports themselves, so "thin shell" was discipline not structure — and
(c) the ADR-014 TypeScript web app is a development drag the user wants
scrapped.

**Decisions reached interactively** (recorded in ADR-030, Proposed):

| Fork | Decision |
|---|---|
| Core↔shell boundary | Single serde command protocol `app::dispatch(Request)->Response`; typed-Rust "hybrid" filed as a triggered escalation (>2 (de)serialize round-trips on a hot native path, or >~40 `Request` variants) |
| Bundles | 3 families: one native `pr` binary (CLI+TUI+serve+MCP+CI); one Vite/React/TS/Tailwind webview (web + Tauri desktop + Tauri mobile); optional uniffi deferred |
| Scope | CLI + TUI + server + web + Tauri desktop + mobile all committed; embedded still deferred |
| Web app | Scrapped — supersedes ADR-014; features ported individually |
| FE stack | Vite + React + TS + Tailwind (Astro rejected for the app — content-first grain vs serverless stateful WASM SPA; reserved for optional docs site) |
| TUI / CLI | ratatui + crossterm; clap derive, consolidated `pr` multicall binary (revises ADR-017's three separate bins) |
| MCP | Yes — `pr mcp` (stdio, tier 1) + MCP-over-HTTP in `pr serve`, via `rmcp` + `schemars`; an MCP tool is `dispatch()` + JSON-Schema, so it falls out of the protocol choice and is gated by the same identity/authz |
| CI gating | The same `pr` binary runs `pr check --diff` in a deployed repo's GitHub Actions (realizes ADR-016); GitHub App is the scale/host upgrade (multi-registry, hosted serve+MCP+OAuth), not a reimplementation |

**Deferred to the next session:** the registry **manifest / capabilities
descriptor** (what a deployed registry exposes + allows) — entangled with
the feature-set discussion the user wants next; gets its own ADR. Until
then capabilities = identity/authz (ADR-020) × wired adapters.

**Feature-parity guardrail (ADR-030 §8):** the user asked for a
guardrail/tests ensuring every spoke exposes the same features (no lazy
stubs). Recorded as a five-layer scheme attacking two failure modes —
op *missing* from a spoke (layers 1–3: catalog-generated spokes,
exhaustive match, parity test + generated `FEATURE-MATRIX.md`) and op
*present-but-hollow* (layer 0: guardrails `no-fake-impl`, already active;
layer 4: per-spoke contract smoke test). Lands as dimension 7 of the
ADR-029 validator, reusing its exemption-with-expiry mechanism; ADR-029
forward-compat got a light cross-ref (no change to its Accepted decision).

**Upstream-to-guardrails triage:** most of the parity scheme is
app-architecture-specific (depends on the `dispatch`/spoke shape) and
stays in part-registry. The one generic primitive worth upstreaming is
**time-boxed escape hatches** — `guardrails-ok` is currently permanent;
an expiring variant (`guardrails-ok-until:YYYY-MM-DD` + a gate that fails
once the date passes) generalizes ADR-029's exemption-with-expiry into
the shared toolbelt and is already on guardrails' roadmap ("generated
escape registries"). Candidate PR pending user go-ahead.

**Built this session — the first feeder (consumer-side):** rather than
leave the coverage story on paper, shipped the structured shortlist of
"what falls out of the ADRs" as data + a prek gate so nothing is lost
logically:

- `decisions/obligations.toml` — 21 rows, one per load-bearing ADR
  commitment (`id`, `adr`, `statement`, `kind`, `status`,
  `satisfied_by`/`tracking`/`exempt_until`). Structured data, not prose.
- `tools/obligations_check.py` — benches reality against it: row-schema,
  `satisfied_by` path resolution, `pending` requires `tracking`, expiring
  exemptions (exit 3), orphan refs (exit 2), and coverage teeth — every
  in-force ADR (minus `[meta].excluded` = 014/015) must have ≥1 row.
  Emits feeder-JSON for the future ADR-029 joiner. Verified: clean=0,
  expired-exemption=3, orphan=2, and a new ADR with no row Fails the
  prek hook with "something fell out".
- `.pre-commit-config.yaml` — `adr-obligations` hook runs it when any
  ADR or `obligations.toml` changes.

This is ADR-029 dimension 4 as a standalone prek feeder, built before the
full Rust joiner per the extraction discipline (joiner extracts upstream
on a second consumer). The expiry semantics (`exempt_until` → exit 3)
are the consumer-side mirror of the proposed guardrails `guardrails-ok-
until` primitive.

**Process notes:** ADR-030 is **Proposed**, not Accepted — awaiting
review. The architectural invariant (shells depend on `crates/app` only,
never on adapter crates) is assigned to the ADR-029 coverage validator
to enforce. ADR number 026 remains unused; 030 keeps numbering monotonic.

## 2026-05-11 — SOUP discipline + architectural coverage validator (ADR-028 + ADR-029)

**Context:** the user surfaced a regulatory gap during the foundation
phase: the OSS dependencies pulled in by the Rust workspace are SOUP
per IEC 62304 §5.3 + §8.1.2, but the ADR set named "audit-grade" as a
property without declaring the software safety class, enumerating the
SOUP, or specifying per-class validation. Separately, the user asked
for CI/pre-commit/prek tooling so coverage-of-the-architectural-surface
cannot silently drift (forgotten crate, forgotten conformance call,
forgotten SOUP entry, silently-resolved trigger).

Two parallel subagent analyses were commissioned (no code modified by
the analyses):

- SOUP analysis (agent `a909e6ce`) — produced the Class B (conditional)
  classification recommendation, the three-tier SOUP scheme (1/2/3
  proportional to safety contribution), the eight validation harnesses
  H1–H8 in priority order, and the maintenance plan per §6.1.
- Coverage-tooling analysis (agent `ae92ea58`) — produced the
  `coverage.toml` schema, the binary location (`crates/port_tests/`
  bin target), the prek-vs-pre-commit recommendation (prek primary,
  canonical pre-commit fallback), the WARN-locally / ERROR-CI split,
  and the four exit codes (with code 3 reserved for expired
  exemptions).

**Outcomes:** two new ADRs Accepted.

| ADR | Decision |
|---|---|
| 028 | Class B (conditional on downstream device verification); ISO 13485 §7.3 design control entry recorded as 2026-05-10; SOUP inventory at `soup/inventory.toml`; H1–H8 harnesses as ADR-027 Tier 5; quarterly health workflow; release blocking on Tier 5 failure |
| 029 | Coverage validator binary `qx-coverage` inside `crates/port_tests/`; `coverage.toml` at repo root with six dimensions; prek pre-commit + CI workflow; WARN local / ERROR CI; exemption mechanism with expiry; orphan-row detection; degrades to WARN if SOUP file absent so could land before ADR-028 |

**Process notes:**

- The user's prompts ("oss = SOUP", "nano-id collision bench", "Micro
  QR roundtrip", "prek for coverage") were the right questions caught
  in the right window — before the five in-flight implementation
  agents (#26 codec, #27 validators, #29 storage_csv_git, #30
  identity+signing, #34 observability) had fully landed. SOUP and
  coverage become *additive* harness work rather than retrofit on
  merged code.
- ADRs 012, 014, 015, 018, 019, 020, 021, 022, 025 are still
  Proposed — the foundation-set sweep on 2026-05-10 flipped 016, 017,
  023, 024, 027 to Accepted; the SOUP-driven re-review for 028
  retroactively makes the entry-into-design-control date explicit.
- Critical gaps surfaced by the SOUP audit that the ADR set did not
  previously address: (a) IEC 62304 software class not named;
  (b) ISO 13485 §7.3 design-control entry date implicit; (c) no SOUP
  inventory; (d) `qrcode 0.14` upstream is dormant — load-bearing
  Class-3 SOUP with no surveillance owner; (e) ADR-013 "data = git
  history" does not defend against SOUP-induced on-disk-format drift.
  All five are addressed in ADR-028 §Decision / §Open questions.
- The conversation explicitly walked back the earlier framing that
  consequence tier alone fixes class — ADR-023's "regulatory finding"
  is a *process* consequence; IEC 62304 §4.3 requires *worst-credible-
  harm* classification. The two are different.

**Cross-references:** ADR-028, ADR-029, ADR-027 (Tier 5 extension);
the five in-flight foundation PRs each carry inventory + coverage
obligations once the seed files land; the coverage tool implementation
is a follow-up issue (the ADR is the framework).

**References:** ADR-028, ADR-029, ADR-017, ADR-023, ADR-024, ADR-027;
IEC 62304:2006/AMD1:2015 §4.3, §5.3, §6.1, §8.1.2; ISO 13485:2016
§7.3.2, §7.3.5; SOUP analysis report (agent a909e6ce);
coverage-tooling analysis report (agent ae92ea58).

## 2026-05-10 — Architectural reset: Rust core, ports/adapters, multi-target deploy, audit-grade crypto-MVP

**Context:** the conversation started with a narrow question — "is the
Python QR + 4/4 / 4/4/4 / 5/5/4 properly built and tested? if yes,
update examples; also wire it through Pyodide to the FE." Tests were
partial (default `4/4/4` only); examples were stale (12-char IDs vs.
the 14-char canonical from ADR-012); the FE was hardcoded to 4/4 and
shipped a separate JS QR encoder. Issue #3 (Pyodide PRIORITY) was the
queued resolution.

The conversation widened deliberately. In sequence the user asked
about:

1. mismatches between FE state and stated principles (yielded an
   audit of #3, #6, #10, #11, #13, #14, #16, #18, #23 against
   ADR-013/014)
2. Rust-vs-Python WASM, with portability to standalone desktop and
   mobile (research subagent confirmed `qrcode-rust2` + `rxing`
   viable; ~1–1.5 MB gzipped vs. ~6 MB Pyodide)
3. tool decomposition for swappable backends (CSV → SQLite/DuckDB/
   Dolt/file-per-entry without redesign)
4. 12-factor configurability + structured logging + auth-as-a-port
5. cryptographic security: distribution integrity, identity
   attestation, action non-repudiation, audit-log tamper-evidence,
   per-row chain of custody, signed/reproducible distribution
6. whether git signed commits + branch protection on GH would
   suffice (honest answer: covers ~60% of the threat model the user
   later picked; misses long-term non-repudiation, no-operator-keys
   UX constraint, per-row attribution, compromised-CI defense)
7. whether Sigstore could be bolt-on rather than refactor (yes, if
   the data model + traits are forward-compatible from day one) and
   whether tests should enforce that discipline (yes — four-tier
   conformance + parity + drift framework)

The threat-model interview yielded:

- **Adversaries in scope:** external attacker, insider with repo
  write, compromised CI runner. **Out of scope:** compromised
  operator device.
- **Assets in scope:** registry contents, per-part chain of custody,
  long-term non-repudiation.
- **Consequence tier:** regulatory finding / contractual breach
  (audit-grade, ISO 13485 / IEC 62304 / Notified Body review). Not
  life-critical (skips formal verification + HSM).
- **UX constraint:** no bespoke key infrastructure for operators.
  Identity piggybacks on existing IdP login (GitHub for now).
- **Crypto MVP:** git signed commits + branch protection + signed
  tags + reproducible builds. Per-row Sigstore deferred with
  explicit re-open triggers T1–T6.

**Outcomes:** nine ADRs Proposed today (one updated, eight new),
~4,050 lines total, all cross-referenced. Status: all Proposed
(formal review pending; methodology requires named reviewers for
Accepted).

| ADR | Decision |
|---|---|
| 016 (updated) | PR-diff policy stands; FE preflight runs Rust validators (not Pyodide) compiled native + WASM |
| 017 | Rust workspace + ports/adapters + strangler-fig migration; supersedes the Pyodide direction in ADR-014 §"Pyodide migration trigger" |
| 018 | Storage as a port; CSV+git first adapter; SQLite/DuckDB/Dolt/file-per-entry future |
| 019 | Proposal sink as a port; GitHub PR first adapter; data-repo split (code stays in `MorePET/part-registry`, data moves to a separate repo) |
| 020 | Identity & authorization as a port; git-config + GitHub OAuth first adapters; OIDC/mTLS/Sigstore future |
| 021 | 12-factor configuration via `crates/config/`; every hardcoded path in `label.py:33-78` and `web/src/config.ts` migrated to env-driven |
| 022 | Observability via Rust `tracing` + audit-csv layer; `print_log.csv` becomes one slice of the broader audit log; request_id propagates across CLI / FE / CI |
| 023 | Threat model fixed; MVP crypto scope = git signed commits + branch protection + signed tags + reproducible builds; deferred controls have explicit re-open triggers |
| 024 | Crypto baseline MVP per ADR-023; SigningProvider trait designed forward-compatible so Sigstore-keyless adapter slots in later without schema change |
| 027 | Four-tier port test discipline: trait conformance, forward-shape, cross-adapter parity, drift-detection (lint-as-test); enforced in CI |

**Process notes:**

- The conversation deliberately delayed all written work until the
  threat-model + crypto direction converged (the user's principle:
  "subagents for codifying decisions you've already made, foreground
  for making decisions"). Once converged, the eight new ADRs were
  written in ~30 minutes via parallel subagents (3 in foreground:
  016 update, 017, 023; 7 in subagents: 018, 019, 020, 021, 022,
  024, 027).
- The Rust workspace scaffold was landed in the same session
  (`cargo check --workspace --all-targets` passes with exit 0). 17
  crates, trait stubs only, no production logic. Python CLIs are
  untouched per strangler-fig discipline.
- Github issues updated mechanically: #3 closed (superseded by
  ADR-017); #5, #6, #10, #11, #13, #16, #18, #19, #23 cross-ref
  comments added; eleven new foundation-phase issues #25–#35 filed
  to track the strangler-fig migration steps.
- One in-flight correction caught: my initial framing positioned
  Sigstore-everywhere as the recommendation. The user's question
  "git signed commits with branch protection — is that not enough?"
  exposed that I had drifted from the stated UX constraint ("no
  bespoke key infrastructure"). The MVP scope was rebuilt around
  that constraint and Sigstore moved to a documented re-openable
  deferred state. Correction is preserved in ADR-023's alternatives
  table.

**References:** ADR-016 (updated), ADR-017, ADR-018, ADR-019,
ADR-020, ADR-021, ADR-022, ADR-023, ADR-024, ADR-027; issues #3
(closed), #5, #6, #10, #11, #13, #16, #18, #19, #23 (commented);
new issues #25–#35; Rust workspace scaffold at `crates/` (cargo
check passes); CI workflow at `.github/workflows/rust.yml`.

## 2026-05-08 — PR-diff-based policy enforcement, not FE-declared intent

**Context:** review/approval policy needs to distinguish routine binds
from destructive actions such as row deletion or voiding. `CODEOWNERS`
and FE-originated metadata are both insufficient: `CODEOWNERS` is
path-based only, and FE declarations are not authoritative for CLI or
manual edits.

**Outcomes:** ADR-016 (PR-diff-based policy enforcement for registry
changes), Status: Proposed. The architectural rule is:

1. policy is derived from the **git diff in the PR**
2. CI classifies semantic change types (`row_bind`, `row_edit`,
   `row_void`, `row_delete`, `header_change`, `bulk_change`, …)
3. enforcement is CI-side and therefore FE-independent

The FE may still suggest labels or PR body summaries later, but those
are advisory only. The canonical enforcement point is the diff-aware
validator/policy engine.

**References:** ADR-016.

## 2026-05-08 — Label format: 14-char canonical, 4/4 display, Consolas font, size-based auto-select

**Context:** ADR-012 specified 12-char IDs displayed as 4/4/4 in Courier
monospace. Small prints (≤ 8mm) showed text at ~1.3mm font — below the
"readable" threshold for thermal printing. The original "always show all 12"
was conservative; the design didn't account for measured font metrics or
the collision triage UX that the web app provides.

**Outcomes:** ADR-012 updated (Status: Proposed). Key changes:

1. **Canonical ID: 12 → 14 chars.** Micro QR M4 at error M holds 14
   alphanumeric chars (2-char headroom was unused). Same QR footprint
   (17×17 modules). Collision space 32¹⁴ ≈ 1.2×10²¹.
2. **Display: 4/4/4 → 4/4 by default.** 8-char prefix displayed as
   2 rows of 4. 2-row formats give ~35% bigger font than 3-row.
   Collisions at 8 chars are negligible (P ≈ 0.00005 at 10k parts)
   and triaged by operator context + QR scan.
3. **Font: Courier → Consolas.** Measured via Pillow textbbox: Consolas
   has highest x-height ratio (0.56), true monospace advance (0.55,
   zero variance across the 32-char alphabet). Courier New's wider
   chars (0.62 advance) cause horizontal overflow at 4/4 on small sizes.
4. **Size-based format selection:** 4/4 for sizes ≤ 8mm, 4/4/4 for ≥ 10mm,
   5/5/4 for ≥ 12mm. Warning system when chosen format is sub-optimal.
5. **2/2 and 3/3 formats dropped** — strictly dominated by 4/4 (same font,
   fewer chars).

Measurement script at `tools/layout_analysis.py` documents the full
analysis: font metrics, horizontal fit, utilization, legibility tiers.

**References:** ADR-012, issue #22, `tools/layout_analysis.py`.

## 2026-05-08 — Print event log (CLI side)

**Context:** issue #12 — a printed-but-unbound label is a real
artifact, but `status` (per ADR-012) is the *logical* unbound/bound/void
relationship and cannot represent multiplicity (a sticker can be
reprinted). Audit traceability needs an event log, not a status
promotion.

**Outcomes:** ADR-015 (Print event log), Status: Proposed. New
`print_log.csv` at the repo root with the schema
`id,printed_at,printed_by,layout,size_mm,extra,copies,output_mode,batch_label`.
`label.py` grows `--log` / `--no-log` (default on), `--operator`
(default `$USER`), `--output-mode` (default `dk-continuous-auto-cut`).
After every successful render of all SVGs the script appends one row
per ID and re-sorts by `printed_at` for stable diffs. `extra` is a
JSON-encoded string of layout-specific options (`{}` for vert/horz,
`{"cableOd":N}` for flag).

**Process notes:** the FE wiring (queue print events into the same
PR pipeline as bind diffs) and the validator wiring (FK to
registry.csv, sort-stability, header equality) are explicitly out of
scope for this CLI-only change — separate follow-up work tracked in
the web app and validators issues. The CLI prints a stderr warning
on local FK miss but still logs; CI is the source of truth for
orphan events.

**References:** ADR-015, issue #12, `label.py`, `print_log.csv`.

## 2026-05-08 — Web app spike (architecture + Lookup/Print/Bind tabs + Error Report plugin)

**Context:** ADR-013 specified the phase 2 web app deployment shape;
user requested a working spike (`web/` directory in this repo) the
same day to validate the architecture and start running labels through
the print path.

**Outcomes:** ADR-014 (web app architecture: extension interfaces,
SSOT, plugin model), Status: Proposed. Working SPA at `web/` with
Vite + TypeScript build, deployed to GitHub Pages via the
`.github/workflows/pages.yml` action on every push to `main` that
touches `web/**` or `registry.csv`.

**Process notes:** the architecture commits to three small interfaces
— `Tab`, `Layout`, `Plugin` — each with its own registry. Adding a
new extension is one file + one registry line + zero core changes.
This is an explicit invariant captured in ADR-014.

Three SSOTs locked:

1. `src/config.ts` — repo slug, registry URL, ID alphabet/length/regex,
   QR border, tape sizes (`pt-N` for P-touch, `dk-N` for QL DK rolls),
   default size.
2. `src/registry/schema.ts` — registry row shape + field metadata
   (`FIELDS` array with `label`, `editable`, `meaningfulFrom`).
   Lookup detail view, Bind form, future validators all read from
   here.
3. `src/registry/registry.ts` — sole `Registry` interface; data layer
   abstracts CSV-from-raw.githubusercontent.com today, will be
   DuckDB-WASM later. Tabs depend on the interface, never on `fetch`.

A drift risk was acknowledged and explicitly traded: the SVG layout
renderers in `web/src/layouts/` are a TypeScript port of `label.py`.
The proper SSOT (Pyodide-loaded `label.py` so FE and CLI run literally
the same code) is the long-term direction per ADR-013 but was deferred
for spike speed. The migration trigger is captured in ADR-014: any
layout-change PR that requires editing both sides, or a roundtrip-test
failure traced to FE-CLI divergence.

The Error Report plugin demonstrates the plugin model end-to-end:
`html2canvas-pro` snapshot → clipboard write → opens prefilled GitHub
issue URL with environment and description, no OAuth token required.

The Bind tab is fully scaffolded with a real localStorage queue but
the GitHub-API submission path is stubbed — the user clicks "submit
batch" and gets an alert showing the queued rows. Implementing the
real OAuth device flow + REST API batch PR creation is a sub-task of
issue #1.

User added a follow-up: Lookup tab should also expose inline edit
that funnels through the same bind-queue infrastructure (DRY — the
queue knows about row diffs, doesn't care whether the diff originated
in a bind or an edit). Filed as a sub-task of issue #1; ADR-014
references it in Consequences.

**References:** ADR-012, ADR-013, ADR-014, `web/`, issue #1.

## 2026-05-08 — Repository extracted from MorePET/exopet

**Context:** ADR-012 (Part identification) and ADR-013 (Parts registry
web app) were drafted in `MorePET/exopet/system-design/parts/` during a
single design session on 2026-05-08. ADR-013 identified "when phase 2
work begins" as the trigger to extract; user moved the extraction
forward to bootstrap the registry as a standalone, public, share-able
artifact and to start labeling parts the same day.

**Outcomes:** new repo `MorePET/part-registry` (public). Files
relocated:

- `system-design/parts/{mint,label,bind,test_labels}.py`,
  `registry.csv`, `examples/` → repo root
- `system-design/decisions/{ADR-012,ADR-013}-*.md` → `decisions/`
- `system-design/decisions/{METHODOLOGY,ADR-template}.md` →
  `decisions/` (audit framework carried over)

The original ADR-012 and ADR-013 files are the canonical source going
forward in this repo. The `MorePET/exopet` decisions index has been
updated to add an "externally hosted ADRs" section pointing readers
here. ADR numbering continues from 014 onward in this repo; the 001-011
ADRs are exopet-specific hardware decisions and stay there.

History was *not* preserved via `git filter-repo` / `git subtree split`
— the parts code was new on the same day, history was minimal, and the
urgency (lab needs to print labels today) outweighed the audit benefit
of preserved history. The exopet-side LOG entry from 2026-05-08
remains as the historical record of how the design evolved.

The repo starts public to remove paid-plan dependencies for GH Pages
deployment (per ADR-013) and to bootstrap quickly. Plan is to move
private once the registry contains operational data — though ADR-013's
argument that the registry data is generally non-sensitive (hardware
IDs + locations, not vendor pricing) means public may end up being the
steady state.

**Process notes:** the GitHub issue tracking phase 2 implementation
work was filed on `MorePET/exopet#13` before extraction; transferred
to `MorePET/part-registry` as part of this move so the work item
lives with its target repo.

**References:**
[`MorePET/exopet/system-design/decisions/LOG.md`](https://github.com/MorePET/exopet/blob/main/system-design/decisions/LOG.md)
(entries from 2026-05-08 documenting the original design session);
ADR-012; ADR-013.
