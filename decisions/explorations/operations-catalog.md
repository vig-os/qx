# Exploration — Operations catalog, shell availability, and user stories

- Date: 2026-06-10
- Status: Exploration (NOT a commitment — feeds the operations/`Request`
  catalog in ADR-030 and the deferred registry-manifest ADR)
- Method: three fresh agents mined the three implementations in parallel
  (Python tooling, the `web/` SPA, the Rust crates/ports). This doc
  synthesizes their findings.
- Feeds: ADR-030 (`crates/app` `Request`/`Response` enum), the deferred
  manifest/capabilities ADR.

## The organizing principle (the key finding)

Mining surfaced a distinction that should drive the whole design:

> **Operations are universal; I/O surfaces are shell capabilities.**

A domain *operation* (mint, bind, render a label, decode an image,
validate a diff) belongs in `crates/app` and is exposed by **every**
shell. What varies per shell is the *I/O surface* an operation plugs
into — a camera to capture an image, a printer/OS-dialog to deliver a
label, an interactive browser for OAuth. Those are **capabilities of the
host**, not separate features. So "scan a QR" decomposes into
`capture-image` (shell capability: web/Tauri only) + `decode-image`
(core operation: everywhere). "Print" decomposes into `render-label`
(core, everywhere) + `deliver-print` (shell capability: GUI dialog / CLI
file / server artifact). This keeps "every app fully capable" honest:
the *operation* is everywhere; only the *I/O* differs.

Pure UI conveniences (theme toggle, error-report screenshot) are
**shell-local features**, not `Request` variants, and stay out of the
core catalog.

## Implementation reality (so we cut from facts)

The Rust core is much further along than the strangler-fig status
implies. Genuinely implemented with tests: `domain`, `validators`,
`codec` (encode/decode/format/svg), `config`, `observability`,
`storage_csv_git` (native), `transport_github_pr` (real reqwest + PR
status mapping), `identity_git_config`, `identity_github_oauth` (incl.
device flow), `signing_git_commit` (verify real; sign is a git-native
placeholder by ADR-024 design), the `wasm` façade, and all three CLI
`run_*` flows.

**The one load-bearing gap:** production proposal submission is **gated
off**. `Wiring::from_config` (`crates/cli/src/lib.rs:508`) installs only
`DryRunSink`; a live submit returns `BadArg` ("…wired through Config in a
follow-up", #35 Phase 3). So mint/bind/void *run and classify* but
**never open a real PR yet**, even though `GithubPrProposalSink` is fully
implemented and just unreachable through `Config`. Un-gating this is
priority-1 for any "fully capable" claim.

Other gaps: no `Authorizer` impl (policy lives in
`validators::policy_decision`, returning `AuthDecision` without the
trait); `crates/port_tests` conformance bodies are placeholders; QR
decode exists in Rust but is **excluded from the wasm façade** (FE keeps
`zxing-wasm`); multi-up sheet composition and printer calibration are
**Python-only**.

## The operation catalog (candidate `Request` variants)

Status legend: **R** = implemented in Rust core · **Rg** = implemented
but gated/unwired · **Py** = Python-only (port needed) · **Web** =
web-only · **New** = not built anywhere · **port** = surface-only work
(exists, not yet exposed through `app`).

### A. Read / query — read-only, safe for every shell incl. unauthenticated

| Operation | Status | Source of truth |
|---|---|---|
| `get-part` (by 14-char canonical id) | R | `Repository::get_part` |
| `resolve-part` (by 8-char prefix/human id; ambiguity-aware) | R | `cli::resolve_part` |
| `list-parts` (filter: status/kind/vendor/location/minted-event; sort; page) | R | `Repository::list_parts` |
| `list-mint-events` (derived: distinct `minted_at` + count + operator; replaces `list-batches` — `batch` retired, ADR-035 §0) | port | audit log / `minted_at`; was web `registry.batches()` |
| `list-print-events` (filter) | R | `Repository::list_print_events` |
| `list-audit-events` (filter) | R | `Repository::list_audit_events` |
| `registry-stats` → `Count{collection, filter, by}` (counts by status/kind/…) | New | web planned (Stats tab) |
| `snapshot-hash` (reproducibility) | R | `Repository::snapshot_hash` |

### B. Write — produce a Proposal; require identity + pass policy

| Operation | Status | Source of truth |
|---|---|---|
| `mint-parts` (N ids → proposal) | Rg | `cli::run_mint` |
| `bind-part` (id + metadata → proposal) | Rg | `cli::run_bind` |
| `rebind-part` (overwrite bound metadata, `--rebind`) | Rg | `cli::run_bind` |
| `void-id` (sticker spoiled/lost → proposal) | Rg | `cli::submit_void` |
| `poll-proposal` (status of a submitted PR) | R | `ProposalSink::status` |

### C. Label / print — render is universal; delivery is a shell capability

| Operation | Status | Source of truth |
|---|---|---|
| `render-label` (ids, layout vert/horz/flag, size/tape, format, micro → SVG) | R | `codec::render_label` |
| `recommend-format` / `format-warning` | R | `codec::format` |
| `compose-sheet` (multi-up die-cut sheet → SVG/PNG/PDF) | Py | `tools/sheet.py` |
| `log-print-event` (append print audit) | R | `Repository::append_print_event` |
| *deliver-print* (OS dialog / native / file / artifact) | shell capability | web `openPrintWindow`; CLI file out |

#### Print model — REVISED (decision 2026-06-10, pending two clarifications → own ADR)

Move away from today's "emit one SVG file per ID." The print request
becomes **structured data** (a `Request::Print` variant — same protocol
as everything else) with an ergonomic **fast-path** CLI form. Example:

```
pr -p -n 10 --size 52 --unit px --format horz --chars 44 --padding 2
```

- `-p` print · `-n 10` = ten **freshly-minted** IDs (mint+print fused)
- `--size 52 --unit px` = label sizing in device px
- `--format horz` = QR left, human-ID text right
- `--chars 44` = the human ID as **two rows of 4** (the 4/4 grouping)
- `--padding 2` = **minimum** 2px padding

Hard constraints:
- **px-true QR**: every QR module (dot) is an identical *integer* px
  size — no fractional modules / aliasing. Generalizes the
  `tools/printer_test_62mm.py` pixel-grid discipline (authored at the
  printer's native 11.226 px/mm) into a first-class codec mode.
- **padding is a floor, fill-to-max**: within a batch, smaller QR
  footprints are padded so every label shares the batch's max-QR
  footprint (uniform physical labels), with `--padding` as the minimum.

Open sub-questions (block the ADR):
1. **px↔physical binding** — is `--size … --unit px` device dots at the
   printer's native DPI (physical mm derived), or abstract units mapped
   by a fixed px/mm? Does `--unit mm` also exist?
2. **mint+print vs proposal timing** — `-n 10` mints (→ a PR) *and*
   prints. Do labels print optimistically while the mint PR is in
   flight, or only after it merges? (mint-then-bind correctness.)

This redesign is large enough to be its **own ADR** (label rendering +
structured print-request model: px-true, padding-fill, fast-path),
referenced from the operations catalog.

### D. Codec

| Operation | Status | Source of truth |
|---|---|---|
| `qr-encode` (payload → matrix) | R | `codec::qr::encode` |
| `decode-image` (image buffer → text; QR+MicroQR) | port | `codec::qr::decode_qr` (exists, not in wasm façade) |
| *capture-image* (camera) | shell capability | web `ui/scanner.ts` (zxing-wasm) |

#### Scan pipeline — one processor, many sources (DRY / SOLID)

A still image, a replayed video, and a live camera are the **same
pipeline**; only the ends differ:

```
FrameSource → decode-image → Resolve{id} → RollingAccumulator → Sink
 (shell cap)      (core)        (core)          (core)          (shell)
```

- **`FrameSource`** (the dependency-inversion seam): still image =
  length-1 stream · video file = frame replay · live camera = frame
  stream · image dir = batch. All yield `Frame`; the processor is
  source-agnostic — "replay a video / an img / live" is *effectively the
  same*.
- **Core processor** (`crates/app`, e.g. a `scan` module): per frame,
  `decode-image` → `Resolve{id}` each symbol against the registry
  (bound/unbound/queued/unknown — the web's `resolveStatus` colouring) →
  a **`RollingAccumulator`** that dedupes across frames, tracks
  first/last-seen, and debounces. Pure + deterministic.
- **`Sink`** (shell): GUI multi-snapshot overlay · TUI list · CLI batch
  JSON · MCP response.
- **Test dividend:** because the processor takes a `FrameSource`, a
  recorded video / image fixture *is* the test harness for the live
  path — deterministic, no camera. Same catalog principle: scan =
  `capture-frames` (shell capability) + `decode+resolve+roll` (core op).
- Replaces today's `web/src/ui/scanner.ts` (its own zxing-wasm decode +
  resolve): TS shrinks to `FrameSource(camera)` + `Sink(overlay)`;
  decode/resolve/roll move into the shared core — this is the
  `decode-image` / drop-zxing-wasm decision realized. → **ADR-032**.

### E. Validate / policy — read-only, no identity; this is the CI/`pr check` surface

| Operation | Status | Source of truth |
|---|---|---|
| `validate-registry` (schema/header/sort/unique/fk/status) | R | `validators::*` |
| `validate-diff` (vs base; status transitions) | R | `validators::validate_diff` |
| `classify-diff` (→ actions) | R | `Diff::classify` |
| `policy-decision` (diff → Allow/Warn/Elevate/Block) | R | `validators::policy_decision` |

### F. Identity / auth — universal; mechanism varies by shell (the IdentityProvider port's job)

| Operation | Status | Source of truth |
|---|---|---|
| `whoami` (current operator) | R | `IdentityProvider::current` |
| `login` (OAuth device flow / git-config / token) | R | `identity_github_oauth`, `identity_git_config` |
| `logout` (clear cached token) | R | `TokenStore::clear` |
| `capabilities` (effective ops for identity × manifest) | Rg | `IdentityProvider::capabilities` (MVP default) → manifest ADR |
| `verify-signature` (commit/audit signature) | R | `VerificationProvider::verify` |

### G. Connection / admin

| Operation | Status | Source of truth |
|---|---|---|
| `open-registry` (locator `file://` \| `github:`) | R | `config` + `cli::bootstrap_data_repo` |
| `sync-registry` (fetch+reset local clone) | R | `cli::bootstrap_data_repo` |
| `bootstrap-registry` (create a new data repo) | Py | `tools/bootstrap-data-repo.sh` (bash; create path) |
| `config-show` | R | `config::from_env` |

### H. Shell-local (NOT core `Request`s)

`capture-scan` (camera, web/Tauri) · `deliver-print` (per shell) ·
`report-error` (web/Tauri) · `toggle-theme` (GUI) ·
`printer-calibration-test` (dev/CLI tooling) · `analyze-label-layout`
(dev tooling).

## Availability matrix (operation × shell)

Shells: **CLI** · **TUI** · **GUI** (web + Tauri desktop/mobile — same
webview) · **Srv** (`pr serve` HTTP + MCP) · **CI** (`pr check`,
headless). ✓ = exposed · ◐ = via a shell capability (I/O differs) · —
= not applicable.

| Operation | CLI | TUI | GUI | Srv | CI |
|---|:--:|:--:|:--:|:--:|:--:|
| get/resolve/list-parts, list-mint-events | ✓ | ✓ | ✓ | ✓ | ✓ |
| list-audit/print-events, registry-stats | ✓ | ✓ | ✓ | ✓ | ✓ |
| snapshot-hash | ✓ | ✓ | ✓ | ✓ | ✓ |
| mint / bind / rebind / void | ✓ | ✓ | ✓ | ✓ | — |
| poll-proposal | ✓ | ✓ | ✓ | ✓ | ✓ |
| render-label | ✓ | ✓ | ✓ | ✓ | ✓ |
| deliver-print | ◐ file | ◐ file | ◐ dialog/native | ◐ artifact | — |
| compose-sheet | ✓ | ✓ | ✓ | ✓ | — |
| qr-encode | ✓ | ✓ | ✓ | ✓ | ✓ |
| decode-image | ✓ | ✓ | ✓ | ✓ | ✓ |
| capture-scan | — | — | ◐ camera | — | — |
| validate-registry/diff, classify, policy | ✓ | ✓ | ✓ | ✓ | ✓ |
| whoami / login / logout | ✓ | ✓ | ✓ | ✓ | ◐ token |
| capabilities | ✓ | ✓ | ✓ | ✓ | ✓ |
| verify-signature | ✓ | ✓ | ✓ | ✓ | ✓ |
| open / sync / bootstrap-registry | ✓ | ✓ | ✓ | ✓ | ◐ |
| config-show | ✓ | ✓ | ✓ | ✓ | ✓ |

Reading: writes are everywhere except CI (CI *gates*, it doesn't
author). Print *delivery* is the only operation whose surface genuinely
differs per shell. Camera capture is GUI-only — but `decode-image` (the
operation) is everywhere, so a CLI/server can decode an uploaded image.

## User stories (by persona)

**Lab operator (bench).** Scans an unbound sticker with the phone camera
(GUI), binds it to a part with type/vendor/location, queues a few more,
submits the batch as one PR. Reprints a damaged label from a lookup.
Mints a fresh sheet of IDs and prints a multi-up die-cut sheet before a
labeling session. — *needs:* mint, bind (batched), render+deliver-print,
compose-sheet, capture-scan→decode, submit, reprint.

**Registry maintainer / admin.** Bootstraps a new data repo for a
project. Reviews incoming bind/mint PRs (the policy gate already
classified them). Adjusts which operations the registry exposes and to
whom (→ manifest). — *needs:* bootstrap-registry, list/validate, policy
visibility, capabilities/manifest.

**Integrator / agent (MCP).** A local agent points `pr mcp` at a folder;
a hosted agent hits `pr serve`'s MCP. Queries parts, validates a
proposed diff, and (if authorized) opens a bind PR — all through the same
`Request` set, gated by the same identity/manifest. — *needs:* the full
read + validate + write catalog as MCP tools (falls out of the protocol).

**CI bot (deployed data repo).** On every PR, runs `pr check --diff`:
validate-registry + validate-diff + classify + policy-decision; blocks or
elevates per ADR-016; posts a check. — *needs:* the validate/policy
surface, headless, no identity.

**Auditor.** Reads the audit + print logs, verifies commit signatures,
recomputes the snapshot hash to confirm reproducibility. — *needs:*
list-audit/print-events, verify-signature, snapshot-hash.

## What's needed (gap list, roughly priority-ordered)

1. **Un-gate production PR submission** — wire `GithubPrProposalSink`
   through `Config` (the #1 blocker; the sink is built).
2. **Extract `crates/app` + the `Request`/`Response` enum** from the cli
   bins and wasm façade (ADR-030 step 1) — the above catalog *is* the
   enum.
3. **Surface read/query** (`list-parts`/`list-batches`/`get-part`/
   stats/audit/print) through `app` so the new FE + TUI + MCP share one
   query path (web issue #10's data-grid rides on this).
4. **Surface `decode-image`** from the Rust codec → makes scan universal
   and is the path to dropping `zxing-wasm` (drift reduction).
5. **Decide sheets/calibration** — port `compose-sheet` into the core (if
   all shells need multi-up sheets) or keep it Python/dev-only.
6. **Authorizer / capabilities** — formalize `policy_decision` as the
   authorizer and define the manifest (its own ADR).
7. **Fill `port_tests` conformance bodies** (already tracked, ADR-027).

## Feature-set decisions (resolved 2026-06-10)

- **Un-gate submission:** YES, priority-1 — wire `GithubPrProposalSink`
  through `Config` (it's built, just unreachable). The difference between
  a demo and a tool.
- **Scan codec:** surface Rust `decode-image` through the app layer +
  wasm façade and **drop `zxing-wasm`** — one codec everywhere (the
  ADR-017 drift goal); decode becomes a universal op. Gate: A/B the
  `rxing` decode path against `zxing-wasm` on real Micro QR scans first
  (already an ADR-017 cutover criterion). → **ADR-032** (scan pipeline +
  the `FrameSource → processor → Sink` design under §D).
- **Printing:** REVISED — see "Print model" under §C above. Structured
  print request + fast-path CLI, px-true QR, padding-as-floor/fill-to-max.
  Gets its own ADR once the two open sub-questions are answered.
- **Query richness:** **full filterable data-grid in v1** (multi-field
  filter, sort, paginate, free-text, stats) — `list-parts` /
  `registry-stats` ship the web #10 grid from the start; shared by FE +
  TUI + MCP.
- **Manifest/capabilities** descriptor remains its **own ADR** (deferred,
  per ADR-030 open questions).
- **Collections metamodel (2026-06-11, ADR-035 §0):** the entity-op
  family collapses into one parameterized set —
  `Create / Get / List / Edit / Transition / Promote { collection, … }` +
  `Describe` — over declared collections (parts, types, products,
  vendors, locations). `mint` = `Create{parts}` sugar,
  `create-vendor` = `Create{vendors}`, `void` = `Transition{parts,void}`,
  `bind` = `Transition{parts,→bound,fields}` (Transition takes an
  optional fields payload; status-changing ⇒ Transition,
  status-preserving ⇒ Edit), `rebind` = `Edit{parts,…}` on a bound part.
  Plus: universal `Resolve{id}` (global id space), generic
  `List{collection, filter}` (the v1 data-grid generalizes to every
  collection), `Count{collection, filter, by}` (single-field group-by —
  replaces `registry-stats`; never a join), `Export{collection, format}`
  (generated artifact, never committed), one shared `Selection`/`Filter`
  type for every selection-taking op (Print included), and declared
  relations with **derived backlinks** ("vendor's parts" =
  `List{parts, vendor=id}`, rendered via descriptor-declared labels —
  never stored twice). Collection roster: parts, types, products,
  vendors, locations (NO batches collection — `batch` is retired;
  mint-events are a derived view, ADR-035 §0). Groups A/B above
  remain the *semantic* catalog; the wire protocol and the §8 parity
  matrix use the parameterized form.
