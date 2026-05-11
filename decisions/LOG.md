# Decision log — part-registry

Append-only chronological record of decisions for the parts registry.
Newest entries first.

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
| 029 | Coverage validator binary `part-registry-coverage` inside `crates/port_tests/`; `coverage.toml` at repo root with six dimensions; prek pre-commit + CI workflow; WARN local / ERROR CI; exemption mechanism with expiry; orphan-row detection; degrades to WARN if SOUP file absent so could land before ADR-028 |

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
