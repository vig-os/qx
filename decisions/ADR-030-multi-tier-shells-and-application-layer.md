# ADR-030 — Multi-tier shells over one application layer + command protocol

- Status: Accepted
- Date: 2026-06-10
- Component / area: cross-cutting — frontends/shells, plus a new
  application (use-case) layer between the ports and every shell.
  Extends ADR-017's "multi-target deploy" from a named option into a
  committed architecture.
- Reviewers: Lars Gerchow (accepted 2026-06-11)
- Supersedes: ADR-014 (web-app extension/SSOT/plugin model — the
  TypeScript SPA it describes is retired)
- Related: ADR-016 (PR-diff policy), ADR-017 (Rust core, ports/adapters),
  ADR-018 (Storage port), ADR-019 (Proposal sink port), ADR-020
  (Identity & authorization port), ADR-021 (Config), ADR-023 (Threat
  model), ADR-024 (Crypto baseline), ADR-027 (Port conformance),
  ADR-029 (Architectural coverage validator)

## Context

ADR-017 settled the foundational shape — Rust core, ports/adapters,
one canonical implementation of codec/validators/policy/audit/identity
— and *named* the deploy surfaces (native CLI, WASM for the FE, and as
an explicit open question: Tauri desktop, Tauri 2 mobile, uniffi,
embedded). It deliberately did **not** commit to the frontend strategy
("Whether to ship Tauri / Tauri-mobile / uniffi … This ADR preserves
the option but does not commit").

Three things have since become load-bearing:

1. **No application layer exists, so "thin shell" is aspirational, not
   structural.** The shells that exist today each wire the ports
   themselves: `crates/cli/src/bin/{mint,label,bind}.rs` compose
   `Repository` / `ProposalSink` / `IdentityProvider` directly, and
   `crates/wasm` is a separate hand-written façade. Two shells already
   means two copies of "propose a bind," "validate a diff," "list
   parts." Adding TUI, a local server, Tauri desktop, mobile, and an
   agent (MCP) surface on top of that structure reimplements the same
   orchestration five more times — the exact drift class ADR-017 was
   created to kill, reintroduced one layer up.

2. **The current TypeScript web app (ADR-014) is a development drag.**
   Its hand-rolled Tab/Layout/Plugin extension model, the TS port of
   `label.py`, and the ad-hoc validators were a reasonable spike but
   are now friction: every domain change touches both Rust and a
   parallel TS reimplementation, and the SPA carries plugin
   infrastructure heavier than the app needs. It will be scrapped and
   rebuilt as a thin shell over the Rust core, with worthwhile features
   ported.

3. **New required surfaces.** The project now wants, beyond CLI + web:
   a terminal UI, a local HTTP server (browser/other clients point at
   `localhost`), desktop and mobile apps, an **MCP** surface so agents
   can operate a registry (both locally via a tier-1 binary and against
   a hosted registry), and **CI as a first-class shell** — the same
   engine running headless in a deployed part-registry's GitHub Actions
   to gate PRs (realizing ADR-016).

Two interaction modes must hold for every shell: point it at **a local
folder that is a deployed part-registry git repo**, or at **a GitHub
repo that is a part-registry**. The same operations must be available
either way; only what the *target registry* exposes and what the
current *identity* is authorized to do may differ — never which shell
is being used.

The constraint that ties all of this together: **every app, always
fully capable.** A feature added once must appear in every shell, and
capability differences must come only from the deployment (which
adapters are wired) and the policy/identity, not from per-shell
feature lag.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo — each shell wires ports itself** | No new layer; already how cli/wasm work | Every shell reimplements orchestration; N copies drift; "fully capable" depends on per-shell discipline that demonstrably already failed (cli vs wasm) | Rejected — reintroduces ADR-017's drift bug at the use-case layer |
| **Typed Rust application API + hand-written FFI façade per non-Rust shell** (wasm-bindgen for web, uniffi for mobile, bespoke HTTP handlers) | Idiomatic per platform | N façades to keep in parity; the FFI boundary becomes the new drift surface; "add a feature" means editing every façade | Rejected as the primary model; **retained as a triggered escalation** (see Decision) for native-ergonomics relief only |
| **One application layer exposing a single serde command protocol (`dispatch(Request) -> Response`), wrapped by every shell** | One place to add a feature; full capability falls out by construction; the same command set serves CLI, TUI, HTTP, WASM, Tauri bridge, and MCP; JSON-Schema for the protocol is derivable (schemars) so MCP tools + a versioned compatibility artifact are near-free | Everything funnels through a serializable command enum; slightly less idiomatic than direct typed calls on hot native paths | **Chosen** |
| **Keep/extend the TS web app (ADR-014 model)** | No FE rewrite | Parallel TS reimplementation of domain logic; plugin infra heavier than needed; ongoing two-sided maintenance | Rejected — scrap and rebuild as a thin shell |
| **Astro for the registry app** | Great routing/content-collections for docs | Content-first / islands / zero-JS grain fights a stateful, offline, serverless SPA that hosts a WASM core in-process and must run in a Tauri webview; Tauri's scaffolding is Vite-centric | Rejected for the app (Astro reserved for an optional separate docs site) |
| **Separate build artifact per frontend** | Independent release cadence; explicit boundaries | More packaging/release surface; duplicated wiring; works against "fully capable" | Rejected in favour of 3 bundle families |

## Decision

Introduce a single **application layer** and route **every** shell
through it via one serde command protocol. Concretely:

### 1. Application layer + command protocol

A new library crate `crates/app` sits between the ports and the shells.
It owns the use-cases (list parts, resolve a label, propose a bind,
validate a diff against policy, append audit/print events, …) and
exposes a single entry point:

```
app::dispatch(&AppContext, Request) -> Response      // Request/Response: serde + schemars
```

- `Request` / `Response` are `serde`-serializable enums (per ADR-035 §0:
  ~10 op-families parameterized by collection — `Create/Get/List/Edit/
  Transition/…{collection, …}` — not one variant per
  operation). `schemars` derives their JSON Schema.
- `AppContext` holds the wired ports for the active connection (see §4)
  plus the resolved identity/authz (ADR-020).
- **Architectural invariant** (enforced by the ADR-029 coverage
  validator): shell crates depend on `app` only. A shell **must not**
  import `storage*` / `transport*` / `identity*` / `signing*` adapter
  crates directly. Capability differences between deployments come only
  from `(connection-wired adapters) × (registry policy) × (identity
  authz)` — never from which shell is running.

### 2. Native binary — one multicall `pr` (clap)

Replaces ADR-017's three separate `mint`/`label`/`bind` binaries with a
single `pr` binary (clap, derive). Subcommands, each a thin shell over
`app::dispatch`:

| Subcommand | Shell | Stack |
|---|---|---|
| `pr mint` / `pr label` / `pr bind` … | CLI | clap; `mint`/`label`/`bind` may stay as busybox-style symlinks for back-compat |
| `pr tui` | terminal UI | **ratatui** + crossterm |
| `pr serve` | local/remote HTTP server: JSON command API + static web assets + **MCP over HTTP** | axum (tokio) + `rmcp` |
| `pr mcp` | stdio **MCP** server for local agents (tier 1) | `rmcp` |
| `pr check --diff` | **CI gate** — headless policy enforcement on a PR (ADR-016) | clap; no UI |

Heavy shells (ratatui, axum, rmcp) live behind **cargo features** so a
minimal CLI/CI build stays lean (consistent with the lean-end-product /
compile-target tiering discipline).

### 3. Webview UI — one Vite + React + TS + Tailwind SPA

Replaces `web/`. One build artifact serves three deploy targets via a
**transport abstraction** injected at build/runtime — the UI never
knows which:

| Transport | Used by |
|---|---|
| **WASM in-process** (`crates/wasm` façade over `app::dispatch`) | web on GitHub Pages (serverless) |
| **HTTP `fetch`** | browser → `pr serve` (localhost or remote) |
| **Tauri `invoke`** | Tauri desktop/mobile → Rust `app` in-process |

Data layer stays thin (e.g. TanStack Query over the transport) because
ground truth and validation live in Rust. Worthwhile features from the
old `web/` (lookup, print pipeline, camera scan, bind queue) are ported;
the Tab/Layout/Plugin model is not.

### 4. Connection / locator

A `Connection` resolves a target string to a wired adapter set:

| Locator | Wired adapters (MVP) |
|---|---|
| `file:///path/to/registry` | `storage_csv_git` (local) + local-branch transport + `identity_git_config` |
| `github:owner/repo` | GitHub-backed storage + `transport_github_pr` + `identity_github_oauth` |

The locator is the one knob that selects "point at a folder" vs "point
at a gh repo"; every shell takes it the same way.

### 5. Bundles — 3 families

- **Native** — one `pr` binary = CLI + TUI + server + MCP + CI.
- **Webview** — one Vite/React UI = web (serverless WASM) + Tauri v2
  desktop + Tauri v2 mobile.
- **Native-mobile (optional, deferred)** — `uniffi` bindings only if a
  webview-on-mobile UX proves insufficient.

### 6. FFI policy — protocol-first, hybrid as a triggered escalation

The serde command protocol is the boundary for all out-of-process /
cross-language shells now. A typed in-process Rust API for native shells
(the "hybrid") is **filed as an escalation** with this trigger: *a
native shell (TUI/CLI) needs more than two (de)serialization round-trips
on a hot interactive path, OR the protocol exceeds ~40 op-families and
native call-sites need compile-time narrowing.* (Restated 2026-06-11:
ADR-035 parameterizes ops over collections, so the unit is op-families,
not raw enum variants — the parameterized family is ~10.) Until a
trigger fires, protocol-only.

### 7. CI gating + GitHub App

A deployed part-registry's CI runs the same `pr` engine: a reusable
GitHub Actions workflow fetches the released `pr` binary and runs
`pr check --diff` to gate PRs; the default `GITHUB_TOKEN` suffices to
post a check and block merge. A **GitHub App** is the scale/UX upgrade
(central policy across many registries, richer Checks output, a bot
identity, and a host for the webhosted `serve` + MCP + OAuth identity) —
same engine inside, not a reimplementation.

### 8. Feature-parity enforcement (no lazy stubs)

"Every app fully capable" is enforced, not trusted, against two distinct
failure modes: **(A)** an op is missing from a spoke, and **(B)** an op
is present but hollow (`todo!()`, faked, "wire later"). Layered, strongest
first:

| Layer | Mechanism | Catches | Status |
|---|---|---|---|
| 0 | guardrails `no-fake-impl` gate (`todo!`/`unimplemented!`/placeholder) at commit | **B** | **already active** (guardrails flake wired) |
| 1 | generate the mechanical spokes (CLI subcommands, MCP tools, HTTP routes) **from the `Op`/`Request` catalog** — projections of one enum, can't omit a variant | **A** | structural |
| 2 | exhaustive `match` on `Request` in `dispatch` + every router, **no `_ =>` arm** (clippy `wildcard_enum_match_arm`); new variant → build breaks until handled | **A** | compile-time |
| 3 | spoke-parity test: one SSOT catalog (`strum::EnumIter`), each spoke declares `surfaced() -> Set<Op>`; assert `catalog − surfaced − exempt == ∅`; CI regenerates a committed `FEATURE-MATRIX.md` (op × spoke) and fails on drift | **A** for human-surfaced spokes (TUI/web) | test + matrix |
| 4 | per-spoke contract smoke test: round-trip **every op through each spoke's real transport** once (CLI invoke, HTTP, WASM, MCP tool) and assert non-error shape | **B** (declared-but-broken) | test |

Layers 0–2 are hard structural guarantees and nearly free given the
protocol architecture; 3–4 cover the spokes that can't be fully
generated. This lands as **dimension 7 (spoke parity)** of the ADR-029
coverage validator, reusing its existing exemption-with-expiry mechanism
(`coverage.toml` `[exemptions]`, exit code 3) and WARN-local / ERROR-CI
posture — ADR-029 §Forward-compatibility already provides for new
dimensions "via the same mechanism," so no change to the Accepted
decision is required. **Honest limit:** layers 0–2 guarantee *wired*,
layer 4 guarantees *responds*; only human review guarantees a UI surface
is actually *usable* rather than technically-present.

**Designed for extraction.** The validator splits into a generic
*joiner* (declared obligations × feeder JSON → matrix + exemptions-with-
expiry + exit codes) and project-specific *feeders* (op×spoke, crates,
ports, SOUP). The joiner and the feeder-JSON schema
(`{dimension, obligation, satisfied, citation, exempt_until}`) are
generic governance, not part-registry-specific. Two pieces ship to the
shared guardrails flake **now** — the expiry primitive
(`guardrails-ok-until:YYYY-MM-DD` + a `guardrails-expired-escapes` gate)
and the feeder-JSON schema published there as a documented convention —
so the contract is fixed before either side builds against it. The
joiner itself stays in-repo to avoid abstracting on a sample size of one.
**Extraction trigger:** promote the joiner into guardrails when a second
consumer needs it (another repo, or guardrails benching coverage of its
own gate set).

### Build order

1. `crates/app` + `Request`/`Response` — extract use-cases out of the
   cli bins and the wasm façade. Unblocks everything.
2. `pr` binary: CLI parity → `serve` → `mcp` → `tui`.
3. New Vite/React/TS/Tailwind UI over the WASM transport; port features;
   delete `web/`.
4. Tauri v2 desktop (wraps the UI; `invoke` → `app` in-process).
5. Tauri v2 mobile (same UI; native camera/scan plugin).
6. CI gating: reusable workflow + `pr check`; GitHub App later.

## Rationale

**One protocol makes "fully capable" a property, not a chore.** Any
shell that can serialize a `Request` and render a `Response` is, by
construction, feature-complete. A new operation is one `Request` variant
+ one `dispatch` arm; it appears simultaneously in CLI, TUI, HTTP, web
(WASM), Tauri, and MCP. This is the use-case-layer analogue of ADR-014's
"adding a tab never edits the core" invariant, generalized across every
shell and enforced by the ADR-029 validator.

**MCP falls out for free — and validates the protocol choice.** An MCP
tool is `dispatch()` with a JSON-Schema wrapper; `schemars` derives the
schemas off the `Request` variants. Choosing the command protocol means
a tier-1 binary (`pr mcp`) and a hosted registry (`pr serve`) become
agent-operable with almost no extra surface. Agent access is gated by
the **same** identity/authz (ADR-020) + policy as any other shell — no
special path; a hosted MCP with write tools is an explicit threat-model
item (ADR-023).

**Protocol-first over per-platform FFI façades** because N hand-written
façades are the precise drift surface ADR-017 set out to eliminate.
Native ergonomics are the only thing the hybrid buys, so it is a
triggered escalation, not a default.

**Scrap the web app rather than refactor it.** The parallel TS domain
reimplementation is the root cost; making the UI a thin transport client
deletes that whole class of work. Vite/React/TS/Tailwind is the honest
fit: it builds once to a serverless WASM-hosting SPA *and* the Tauri v2
webview for desktop and mobile. Astro's content-first grain fights a
stateful, offline, in-process-WASM app, so Astro is reserved for an
optional separate docs site.

**One multicall binary** collapses the entire native family — CLI, TUI,
server, MCP, CI — into a single artifact to build, sign (ADR-024), and
distribute, with cargo features keeping the minimal build lean.

## Consequences

- **`crates/app` becomes a mandatory seam.** Shells depend on `app`
  only; the ADR-029 coverage validator gains a rule that no shell crate
  imports an adapter crate. The cli bins and the wasm façade are
  refactored to route through `dispatch`.
- **`web/` and the ADR-014 model are retired.** ADR-014 is superseded;
  its Tab/Layout/Plugin interfaces and the TS `label.py` port are
  deleted. Ported feature parity is tracked per feature, not assumed.
- **ADR-017 is refined**: three separate binaries → one `pr` multicall;
  Tauri desktop + mobile move from "open question" to committed;
  MCP and a local server join as new shells.
- **The `Request`/`Response` enum is a versioned compatibility
  surface.** Its `schemars` JSON Schema should be snapshot-tested (like
  the `schema/registry-contract.json` data contract) so a breaking
  change to the protocol is caught in review.
- **Cargo feature discipline**: `tui`/`serve`/`mcp` behind features;
  CI builds the minimal profile; release builds the full `pr`.
- **New threat surface**: hosted MCP/HTTP write paths are gated by the
  identity/authz port + registry policy; covered under ADR-023 and must
  not bypass `dispatch`.
- **Reproducible-build (ADR-024) and port-conformance (ADR-027)
  disciplines are unchanged** and now also cover the `app` layer.
- **Tauri/uniffi toolchain**: committing desktop + mobile adds the Tauri
  v2 toolchain (and platform SDKs for mobile) to the build matrix.
- **Feature-parity is gated, not trusted** (§8): the `Op` catalog is the
  SSOT projected into CLI/MCP/HTTP; routers match it exhaustively; a
  `FEATURE-MATRIX.md` is generated and CI-diffed; a spoke that drops or
  stubs an op fails CI (or `no-fake-impl` at commit). Adding an `Op`
  obliges either surfacing it in every spoke or filing an expiring
  exemption in `coverage.toml`.

## Open questions / supersession triggers

- **Registry manifest / capabilities descriptor.** This ADR assumes a
  registry's exposed features + allowed operations are
  `manifest × identity-authz`, but does **not** specify the manifest
  artifact (its schema, where it lives in the data repo, how it
  versions against the schema contract). That is entangled with the
  feature-set discussion and gets its own ADR. Until then, capabilities
  are computed from identity/authz (ADR-020) + wired adapters only.
- **Hybrid-FFI escalation** trigger as stated in Decision §6.
- **uniffi native-mobile** is built only if the Tauri-webview mobile UX
  is insufficient (scan latency, native feel).
- **Embedded / kiosk** target remains deferred (ADR-017) — re-opens on
  a deployment requirement.
- **Do `serve` and the GitHub App share one binary/runtime**, or is the
  App a separate deployment of the same `app` layer? Decide when the
  hosted/multi-registry requirement is concrete.

## References

- [ADR-014 — Web app architecture](ADR-014-web-app-architecture.md) (superseded)
- [ADR-016 — PR-diff policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
- [ADR-017 — Rust core, ports/adapters, multi-target deploy](ADR-017-rust-core-ports-adapters.md)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
- [ADR-029 — Architectural coverage validator](ADR-029-architectural-coverage-validator.md)
- clap — <https://docs.rs/clap>
- ratatui — <https://ratatui.rs>
- axum — <https://docs.rs/axum>
- rmcp (Rust MCP SDK) — <https://github.com/modelcontextprotocol/rust-sdk>
- schemars — <https://docs.rs/schemars>
- Tauri v2 (desktop + mobile) — <https://v2.tauri.app>
- Vite — <https://vite.dev>
- TanStack Query — <https://tanstack.com/query>
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
