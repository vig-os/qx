# ADR-028 — SOUP validation per IEC 62304 §5.3 + §8.1.2

- Status: Accepted
- Date: 2026-05-11
- Component / area: cross-cutting — declares the software safety class,
  enumerates Software Of Unknown Provenance, fixes the per-class
  validation regime, and assigns maintenance ownership
- Reviewers: Lars Gerchow
- Related: ADR-013 (data = git history), ADR-017 (Rust workspace),
  ADR-018 (Storage port), ADR-019 (Proposal sink), ADR-020 (Identity),
  ADR-022 (Observability), ADR-023 (Threat model + crypto-MVP),
  ADR-024 (Crypto baseline), ADR-025 (Distribution integrity),
  ADR-027 (Port conformance + drift), ADR-029 (Architectural coverage
  validator — enforces the obligations declared here)

## Context

`METHODOLOGY.md` names IEC 62304 as one of four audit audiences. The
foundation set (ADRs 016 through 027) commits the project to
"audit-grade" properties — but does not name the software safety
class, does not enumerate the OSS dependencies as SOUP, and does not
specify how each SOUP component is validated against the role it
plays. IEC 62304:2006/AMD1:2015 §5.3 ("Software of Unknown
Provenance") is not optional for Class B or Class C software: every
SOUP item must be identified (§5.3.2), its functional and
performance requirements documented (§5.3.3), and verified that it
fulfils those requirements (also §5.3.3). §8.1.2 adds the
problem-reporting obligation: SOUP issues that affect the device
must be tracked.

A fresh-agent SOUP audit performed 2026-05-11 against the foundation
set surfaced five gaps:

1. **No software safety class is declared.** The closest stated
   framing is ADR-023's *consequence tier* — "regulatory finding /
   contractual breach… not life-critical" — but that is a
   *process* consequence, not the IEC 62304 §4.3 *worst-credible-harm*
   classification the standard requires.
2. **No ISO 13485 §7.3 design-control entry date is recorded.**
   The project crossed into design control on 2026-05-10 when
   ADR-017 / ADR-023 / ADR-024 / ADR-027 went Accepted; the
   transition is implicit but not stated.
3. **No SOUP inventory exists.** Dependencies appear only in
   `Cargo.toml`s; their classification, validation method, and
   maintenance ownership are nowhere documented.
4. **`qrcode 0.14` — the Standard QR + Micro QR encoder for the
   permanent physical label — is dormant upstream** (ADR-017 §Open
   Questions already flagged this). Using a dormant SOUP in a
   load-bearing role without surveillance ownership is exactly the
   §5.3 case the standard requires to be managed.
5. **ADR-013's "data = git history" property does not defend
   against SOUP-induced on-disk-format drift.** A `serde` / `time`
   / `uuid` version bump can silently change CSV or JSON
   serialization; git diff stays continuous while the audit trail
   becomes semantically discontinuous. No current test covers this.

The SOUP audit also produced a per-crate validation plan with
priority order, a three-tier classification scheme proportional to
each SOUP's contribution to a safety-relevant function, and a
maintenance plan keyed to upstream responsiveness.

This ADR records the regulatory framework. ADR-029 records the
mechanical enforcement (the obligations validator that ensures
nothing in this framework gets silently dropped).

## Alternatives considered

### Option set A — software safety class

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **No class declared, leave implicit** | Lowest immediate friction | Indefensible at any audit; §4.3 is non-optional; ADR-028 itself cannot proceed without a class because §5.3 obligations are class-dependent | Rejected |
| **Class A** (no injury or damage possible) | Minimum §5.3 obligations | Defensible *only* if every downstream device treats every registry datum as recomputed/reverified at point of use. That commitment is not documented today and is downstream of this project's authorship | Rejected for the unconditional case |
| **Class B (conditional on downstream verification)** | Matches the actual worst-credible-harm chain (mis-attributed sensor ID → wrong calibration coefficients attached → wrong reconstructed activity → wrong dosimetry, bounded by downstream PET-system QA at scan time). Imposes §5.3 obligations proportional to the registry's actual contribution. Honors ISO 14971 §5.4 risk-control proportionality | More verification effort than Class A; requires downstream's risk file to record "registry data is a verified input" | **Chosen** — accurate to the actual hazard chain |
| **Class C** (death or serious injury possible) | Maximum rigour | Implausible without a much longer hazard chain than this project owns; would impose disproportionate validation cost | Rejected |

### Option set B — SOUP validation regime

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **No SOUP discipline beyond `Cargo.lock`** | Zero engineering | Fails §5.3.2 (identification) and §5.3.3 (verification) outright | Rejected |
| **Inventory only, no validation harness** | Cheap | Fails §5.3.3 verification clause; identifies SOUP but doesn't verify it fulfils its requirements | Rejected |
| **Per-crate ad-hoc validation tests with no central inventory** | Fragments concern across crates; familiar pattern | No single audit artifact; ADR-029 cross-check (inventory ↔ `cargo metadata`) cannot run | Rejected |
| **Three-class scheme (1/2/3) proportional to safety contribution, inventory at `soup/inventory.toml`, Class-3 validation harnesses as ADR-027 Tier 5, maintenance plan keyed to upstream responsiveness** | Audit-defensible per §5.3 + §6.1 + §8.1.2; single-artifact audit surface; proportional cost; composes with ADR-027 and ADR-029 | Up-front scheme design; per-Class-3 harness costs | **Chosen** |
| **Full §5.3 rigor for every dependency (no class scheme)** | Maximum traceability | Disproportionate to risk; would impose photo-corpus tests on `thiserror` | Rejected |

### Option set C — inventory location

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Inline in each `Cargo.toml` via custom metadata | Co-located with the dep declaration | Spreads inventory across 17 files; auditor must aggregate manually; tooling parses `[package.metadata]` which is not portable | Rejected |
| Single top-level `SOUP.md` (Markdown only) | Auditor-friendly | Not machine-readable; ADR-029 coverage tool cannot consume it programmatically | Rejected |
| **`soup/inventory.toml` (machine-readable) + auto-rendered `SOUP.md` (auditor view)** | Single source of truth, machine-consumable, auditor-readable rendering | Two-file relationship to maintain — but the rendering is mechanical | **Chosen** |

## Decision

### 1. Software safety class

The project is **IEC 62304:2006/AMD1:2015 Class B**, conditional on
the downstream PET-system risk file recording "registry data is a
verified input" as the controlling mitigation. The conditionality is
load-bearing: if a downstream device's risk file is amended to
*not* re-verify registry data at use, this classification re-opens
and may promote to Class B unconditional or — if a hazard chain to
serious injury becomes credible — Class C.

The class is recorded as a **design input** under ISO 13485 §7.3.2.

### 2. ISO 13485 §7.3 design-control entry

Design controls under ISO 13485 §7.3 **apply to this project as of
2026-05-10**, the date ADR-017 / ADR-023 / ADR-024 / ADR-027
transitioned from Proposed to Accepted. From that date, this
folder's ADRs are design inputs and outputs; LOG.md is the design
history record; CI is the design verification record per ADR-016 +
ADR-027.

### 3. SOUP classification scheme

Every dependency declared in workspace `Cargo.toml` or any per-crate
`Cargo.toml` is SOUP per §5.3.2. Each is classified into one of
three tiers proportional to its worst-credible contribution to a
safety-relevant property:

- **Class 1** — battle-tested foundational libraries whose failure
  modes are loud and quickly diagnosed (e.g. `serde`, `thiserror`,
  `anyhow`, `clap`). Required: pinned version, inventory entry,
  passive monitoring.
- **Class 2** — specialized but mature libraries where failures
  can be silent but bounded to non-safety-critical surfaces (e.g.
  `tracing`, `figment`, `time`, `uuid`, `wasm-bindgen`). Required:
  Class-1 obligations plus a targeted feature-use behavior test
  and a maintenance signal (upstream issue tracker URL, last
  release check).
- **Class 3** — libraries whose failure directly affects a safety-
  relevant property (e.g. `qrcode` Micro QR encoder, `rxing`
  decoder, future `git2`, future `reqwest`, the canonical-ID
  generator). Required: Class-2 obligations plus a dedicated
  validation harness under ADR-027 Tier 5, named surveillance
  owner, explicit re-validation trigger on every upstream change.

### 4. SOUP inventory

The inventory lives at `soup/inventory.toml` (machine-readable) at
the repo root. An auto-rendered `SOUP.md` (auditor view) is
regenerated by the ADR-029 coverage tool on every CI run.

Required fields per inventory entry:

```toml
[[crate]]
name = "qrcode"
version = "=0.14.0"
class = 3
role = "qr_encoder_standard_and_micro_m4"
used_by = ["codec"]
maintenance_url = "https://github.com/kennytm/qrcode-rust"
last_release_check = "2026-05-11"
upstream_status = "dormant"               # active | maintenance-mode | dormant | unmaintained
validation_harness = "crates/port_tests/src/soup/qr_roundtrip.rs"
surveillance_owner = "codec-maintainer"
revalidation_trigger = "any-version-bump"
notes = """
ADR-017 §Open Questions flags upstream dormancy. Pin held until
qrcode-rust2 (active fork) publishes to crates.io or is vendored.
H2 + H3 harnesses run on every CI; matrix-fingerprint golden file
catches encoder drift.
"""
```

Class-1 entries omit `validation_harness` and may use simpler
`revalidation_trigger = "major-version-bump"`.

### 5. Validation harnesses (ADR-027 Tier 5)

Eight harnesses are committed in priority order. Each lives under
`crates/port_tests/src/soup/`:

| ID | Crate(s) | Purpose | Priority | Lands with |
|---|---|---|---|---|
| **H1** | canonical-ID generator (nano-id or equivalent) | Alphabet fidelity (32 chars, ADR-012 set) + 1×10^7 collision bench + per-position chi-square uniformity + RNG-source attestation across native/wasm32 targets | **Highest — cheapest** | Foundation #26 codec PR or immediate follow-up |
| **H2** | `qrcode` × `rxing` × foreign decoder | Micro QR M4 roundtrip on 10⁴ canonical IDs; cross-decoder agreement; matrix-fingerprint golden file | **Highest — load-bearing** | Foundation #26 codec PR |
| **H8** | `serde`, `time`, `uuid`, `csv` | On-disk-format drift via stored golden fixtures (`AuditEntry`, `Operator`, `PrintEvent`, `Proposal`); byte-equality round-trip | High — closes ADR-013 gap | Foundation #29 storage_csv_git PR |
| **H5** | `git2` (or git-shell) | Signed-commit verification parity vs `git verify-commit` shell-out on a fixture of signed/unsigned/revoked/expired commits | High — backs ADR-024 | Foundation #30 identity+signing PR |
| **H7** | `tracing-subscriber` audit-CSV layer | 10⁵ events under N-thread concurrency: no drops, no reorders, no truncation, CSV parseable | High — backs ADR-022 | Foundation #34 observability PR |
| **H6** | `reqwest` / `rustls` | TLS pinning + bad-cert rejection against `mockito` fixture server | Medium — backs ADR-020 OAuth + ADR-019 transport | Foundation #30 + #31 PRs |
| **H4** | `rxing` real-world Micro QR corpus | Field-photo corpus (bootstrap batch `B-2026-05-08-sheet-1`) decoded by `rxing` vs `zxing-wasm` reference; flag any divergence | Deferred — needs photo corpus | Post-foundation, before any non-developer rollout |
| **H3** | `qrcode` (Standard QR) | ISO/IEC 18004:2015 Annex A test-vector conformance | Medium — codec maturity | Codec hardening, post-foundation |

The harness output contract is one JSONL record per `(crate,
harness, ci-run)` plus an aggregated `soup-report.{jsonl,md}` written
to the data repo's `releases/<tag>/` directory per ADR-019 split.

### 6. Maintenance plan (§6.1 / §8.1.2)

Per Class:

| Class | Inventory | Validation harness | Maintenance signal | Re-validation trigger | Owner |
|---|---|---|---|---|---|
| 1 | required | no | upstream major-release feed | major bump | crate maintainer |
| 2 | required | targeted feature test | upstream issue tracker monitored; critical ≤30d | major or minor bump | crate maintainer |
| 3 | required | full ADR-027 Tier 5 harness | upstream issue tracker + CVE feed; critical ≤14d (security) or ≤30d (functional) | any bump | named surveillance owner |

A **quarterly SOUP-health workflow** runs `cargo audit`,
`cargo outdated`, and a dormancy script that compares each Class-3
SOUP's last-commit date to its responsiveness window. Output is a
generated Markdown table appended to a quarterly tracking issue
under label `soup-health`.

### 7. Failure-handling policy

- **PR-time**: Tier 5 SOUP harness failure is a `required_status_check`
  (per ADR-024 §Branch protection). PRs cannot merge with red Tier 5.
- **Release-time**: Tier 5 failure blocks tag promotion in the
  reproducible-build matrix (ADR-024 §4 + ADR-025).
- **Class 1**: no harness; failures surface as compile errors.
- **Class 2**: harness present; PR-merge is policy-call (label
  `soup-tier2-fail` for triage), release-blocking.
- **Class 3**: PR-blocking AND release-blocking. Override only via
  an explicit waiver ADR (`ADR-NNN — Waiver: <SOUP-name> <date>`).

## Rationale

**Why Class B conditional rather than Class A unconditional.** The
worst-credible-harm chain (mis-attribution → wrong calibration →
wrong dosimetry → patient harm) terminates at a downstream device's
QA boundary today; the project does not own that termination. Class
A is defensible only if downstream's risk file *records* the
verification commitment. We do not own that record. Class B
conditional makes the dependency explicit and gives the project a
re-open trigger (the downstream device's risk file changing) that
would otherwise be silent.

**Why three SOUP classes rather than two or five.** Two classes
(Class 1 / Class 3) lose the middle tier — `tracing` is not
foundational-library-trivial but is also not photo-corpus-grade
load-bearing. Five classes over-engineer for the current SOUP set
size (~20 crates today). Three classes match the natural break
between "loud failures" / "bounded silent failures" / "safety-
relevant silent failures." Same logic as ISO 14971 §5.4
risk-control proportionality.

**Why inventory at repo root rather than under `decisions/`.**
`soup/inventory.toml` is consumed by the ADR-029 tool on every CI
run; it is operational data, not a decision record. Decisions about
the SOUP go in this ADR; the live inventory tracks current state.
Same separation as the project's existing `registry.csv` (data) vs.
`decisions/ADR-012` (decisions).

**Why ADR-027 Tier 5 rather than a separate crate.** ADR-027
already names the test discipline; auditors locate the discipline by
reading one ADR. A separate `crates/soup/` fragments the audit
surface. Tier 5 has the same shape as Tiers 1–4 (generic functions
in `port_tests`, called from fixed harness tests).

**Why JSONL + Markdown output rather than just one.** JSONL is
machine-consumable for the ADR-029 coverage tool and for future
trend dashboards. Markdown is what an auditor reads. Both regenerated
mechanically from the same harness run.

**Why the H1–H8 priority order.** H1 (alphabet/collision) defends
the registry primary key; cheapest and most universally load-bearing.
H2 (Micro QR roundtrip) defends the permanent physical label; ADR-017
step-1 acceptance requires it anyway. H8 (format drift) closes the
ADR-013 gap surfaced in the audit. H5 (signed-commit verification)
defends the ADR-024 chain. H7 (audit log under load) defends ADR-022
integrity. H4 (real-world corpus) is high-value but blocked on photo
acquisition; deferred. H3 (ISO 18004 conformance) is rigour, not
foundation-blocking.

## Consequences

This ADR commits the project to:

- **Class B (conditional)** software safety classification is the
  controlling design input from this date forward. Re-validation
  rigour is sized to Class B per §5.3.3.
- **ISO 13485 §7.3 design-control entry on 2026-05-10** is the
  recorded transition date. The design-history file timeline begins
  here.
- **`soup/inventory.toml` must accompany every dependency-adding
  PR.** Adding a dep without an inventory entry is a `required_status_check`
  failure once the ADR-029 coverage tool lands.
- **Class-3 SOUP cannot land without a Tier 5 validation harness.**
  No exceptions; waivers require a named waiver ADR.
- **A surveillance owner must be named for every Class-3 SOUP.**
  Ownership is a role, not an individual.
- **A quarterly SOUP-health workflow runs `cargo audit` + dormancy
  check.** Output is a labelled issue, not an email.
- **Tier 5 failure is release-blocking.** ADR-024 §Branch protection
  + ADR-025 release flow both gate on the Tier 5 result.
- **A waiver mechanism exists** but is deliberately heavyweight: a
  per-SOUP waiver ADR. This makes "we know this is broken and chose
  to ship anyway" auditable.
- **The seed `soup/inventory.toml`** lands in this PR (or the next)
  populated with the 14 currently-declared workspace dependencies,
  classified per §3, with the H1–H8 harness paths pre-noted (paths
  may be empty until the harness lands).
- **The five in-flight foundation PRs** (#26 codec, #27 validators,
  #29 storage_csv_git, #30 identity+signing, #34 observability) each
  add their incoming deps to the inventory at the same commit. The
  ADR-029 coverage tool will enforce this once it lands.

This ADR does **not** commit the project to:

- A specific class for every future dependency. Classification is
  per-PR judgement against §3.
- Standing up `cargo-vet` or other audit-trail tooling beyond
  `cargo-audit` at this time (deferred — re-open if a customer
  audit requires the full attestation chain).
- A specific `qrcode` swap path. The pin held against the dormant
  upstream is intentional and re-evaluated quarterly.

## Forward-compatibility

When ADR-029 lands, the coverage tool reads `soup/inventory.toml`
as one of its six dimensions and emits a fail when:
- a dep in `cargo metadata` is missing from the inventory
- a dep in the inventory no longer appears in `cargo metadata` (orphan)
- a Class-3 entry has an empty `validation_harness` path
- a Class-3 entry's harness file does not exist on disk

When a future ADR adds `cargo-vet` integration, the inventory's
`upstream_status` field gets a complement field for the
vet attestation. Forward-compatible.

When the project promotes to Class C (re-open trigger below),
every Class-2 SOUP escalates to Class 3 by default; the inventory's
`class` field is re-evaluated.

## Re-open triggers

This ADR is reviewed and reconsidered when any of the following
occurs:

- **R1 — Software class changes.** Downstream device's risk file
  removes the "registry data is a verified input" mitigation,
  promoting the class to Class B unconditional or Class C. Activates:
  full re-validation of every Class-2 SOUP at Class-3 rigour;
  potential additional harnesses.
- **R2 — Any ADR-023 trigger T1–T6 fires.** Adversary / asset /
  consequence / UX changes can change SOUP scope. Activates: SOUP
  inventory re-review at the new threat model.
- **R3 — Any Class-3 SOUP goes dormant >9 months.** Specifically
  `qrcode` upstream — currently dormant, watched monthly. Trigger
  fires automatically via the quarterly SOUP-health workflow.
  Activates: vendor / fork / replacement decision; potentially a
  successor adapter.
- **R4 — A SOUP CVE forces an emergency response and reveals a
  missing harness.** Activates: harness backfill + post-mortem
  ADR amendment.
- **R5 — IEC 62304 standard is amended materially.** Activates:
  re-audit of all §5.3 / §8.1.2 obligations against the new text.

## Open questions / supersession triggers

- **Whether `rustls` (pure-Rust TLS) or `native-tls` (OS-conditional)
  is the better choice for `reqwest`** in the GitHub OAuth and
  Proposal-sink adapters. `rustls` preserves ADR-024 §Reproducible
  builds (no OS-conditional binaries); `native-tls` inherits the
  platform's certificate store. Recommendation pending until
  foundation issue #30 lands: prefer `rustls` unless a customer
  environment requires Windows SChannel integration.
- **Whether the canonical-ID generator should be in `crates/domain/`
  or `crates/codec/`.** The SOUP audit recommends domain so the H1
  harness can invoke it without depending on codec; ADR-017's
  strangler-fig step 3 implied codec. To be resolved in the H1
  implementation PR.
- **Whether `git2` (libgit2 binding) or git-CLI shell-out is the
  right substrate for storage / signing.** `git2` is a richer API
  but adds a C dependency; shell-out is reproducible-build cleaner
  but has more cross-platform surface. To be resolved in foundation
  issue #29 / #30 implementation.
- **Whether `cargo-vet` integration belongs in the next ADR or
  this one.** `cargo-vet` would give per-version supply-chain
  attestations; useful for a customer audit but adds substantial
  surface today. Deferred — re-open with a customer audit ask.

## References

- IEC 62304:2006/AMD1:2015 §4.3 (Software safety classification),
  §5.3 (Software of Unknown Provenance), §6.1 (Software maintenance
  planning), §8.1.2 (SOUP problem reporting)
- ISO 13485:2016 §7.3 (Design and development controls),
  §7.3.2 (Design and development inputs), §7.3.5 (Design and
  development verification)
- ISO 14971:2019 §5.4 (Risk control measures)
- [ADR-012 — Part identification](ADR-012-part-identification.md)
- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md)
- [ADR-022 — Observability](ADR-022-observability-tracing-audit.md)
- [ADR-023 — Threat model + crypto-MVP](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-024 — Crypto baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
- [ADR-025 — Distribution integrity](ADR-025-distribution-integrity.md)
- [ADR-027 — Port conformance + drift](ADR-027-port-conformance-tests.md)
- [ADR-029 — Architectural coverage validator](ADR-029-architectural-coverage-validator.md)
- METHODOLOGY.md
- LOG.md (2026-05-10 architectural reset; 2026-05-11 SOUP discipline)
- SOUP analysis report — agent `a909e6ce`, 2026-05-11
