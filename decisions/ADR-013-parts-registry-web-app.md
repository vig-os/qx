# ADR-013 — Parts registry web app: GH Pages + WASM DuckDB + PR-driven binds

- Status: Superseded by ADR-030 (GitHub Pages target dropped 2026-06-30)
- Date: 2026-05-08
- Component / area: Phase 2 of the part identification system (ADR-012).
  Tooling target: a separate `part-registry` repo (extraction trigger
  defined below).
- Supersedes: none. Builds on ADR-012.

> **Superseded 2026-06-30.** This ADR proposed (never Accepted) a static
> GitHub-Pages SPA as the app. ADR-030 reframed the FE as one UI over the
> shared application layer with three deploy targets, and the serverless
> Pages target is now **dropped**: serverless WASM is read-only (it cannot
> run the device-flow write auth — ADR-030 §"Per-shell consequence"), so it
> never carried the real app. The FE deploy story is the **local-first
> shells**: `qx serve` (static bundle + JSON command API, full read+write
> via the credential resolver; reach a phone over the tailnet with
> `tailscale serve`) and **Tauri v2** native desktop/mobile. The Pages
> workflow is disabled (manual-only). The PR-driven-bind + CI-enforced-
> invariant ideas below remain valid and live on in ADR-030/ADR-034.

## Context

ADR-012 ships the phase 1 tooling — `mint.py`, `label.py`, `bind.py` and
`registry.csv` — so IDs can be minted, labels rendered, and binds
recorded today, in this repo, by one person at a CLI. Three things drive
the need for phase 2:

1. **Lab-floor scanning.** A technician with 30 fresh-bound parts at a
   workbench can't run a Python CLI per part. They need to scan a QR
   with their phone and fill a form. CLI binding is fine for setup; it
   doesn't survive contact with day-to-day operations.
2. **Lookup needs to outlive the project.** The bind ↔ part record has
   to be queryable for the operational lifetime of the hardware
   (decade+). A bespoke server-side app is a long-term liability; ADR-012
   already established that we want infrastructure that survives domain
   changes, vendor-lock changes, and team changes.
3. **Multi-project consumption.** The registry is conceptually a shared
   resource across `exopet`, future PET prototypes (`fd5`), and any
   downstream eXoma product that uses physical part identification.
   Keeping it inside `exopet/` couples its lifecycle to one project.

The constraint set for the phase 2 web app:

- No server to maintain. Static-only deploy.
- The data must live in git (every change is auditable, every bind has
  a commit).
- Camera scanning from a phone, no native app.
- Auth that survives infrastructure churn — ideally piggybacking on
  GitHub identity rather than rolling our own.
- Bind workflow has to absorb 30 binds in 5 minutes without 30 PRs.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Notion / Airtable / Grist** as registry backend | Off-the-shelf form UI, no code | Vendor lock-in at the data layer; export-as-CSV is best-effort and breaks audit; no git-native diff trail; pricing scales per-row | Rejected: violates the "permanence beats convenience" principle from ADR-012 |
| **FastAPI + SQLite** server-side app | Familiar; rich query UI; full control | Requires hosting, TLS, auth, backup discipline; a long-term ops liability for a small team; another domain decision to live with | Rejected: phase-1 already paid the price of avoiding domain coupling — recreating it in phase 2 would be a regression |
| **Tailscale-fronted webapp** (FastAPI on a NUC) | Auth handled by Tailscale; private-by-default | Requires every operator to be in the Tailscale network; doesn't scale across sites/contractors; same hosting/backup ops burden | Rejected: friction for the lab-floor use case |
| **GitHub Issues as the registry** (one issue per ID, bind = comment) | Zero hosting; rich audit trail; mobile UI free | Issues aren't queryable as a table; bind volume blows out the issue list; no strong schema enforcement | Rejected: data shape mismatch |
| **GH Pages + WASM DuckDB + sorted CSV in git, PR-driven bind via the GitHub API** | No server; auth = GitHub identity; data = git history; queries fast in-browser; multi-tenant for free; static deploy | Bind path is a PR (not an instant write); CI must enforce invariants; needs a separate repo to host the Pages site cleanly | **Chosen** |

## Decision

A **static SPA hosted on GitHub Pages**, served from a dedicated
`part-registry` repository (extracted from `exopet` when phase 2 work
starts — see Consequences). The site:

- **Loads `registry.csv`** (canonical, sorted by `id`, in `main`) into
  an in-browser **DuckDB-WASM** instance for SQL queries.
- **Scans QR codes via `getUserMedia` + `@zxing/browser`** (or the
  built-in `BarcodeDetector` API where available).
- **Renders the part page** for any scanned/searched ID: registry row,
  bind history, related parts in the same batch.
- **Bind UX** uses a **batched PR** workflow:
  - Pending binds accumulate in `localStorage` as the operator scans &
    fills. A "submit batch" button opens **one** PR via the GitHub REST
    API containing all pending changes, applied to `registry.csv` while
    preserving the sort order.
  - The user authenticates to GitHub via OAuth device-flow (no client
    secret needed for a public client).
- **Validators are shared between the frontend and CI** — see "Shared
  validation" below. The same Python scripts run in the browser (live,
  as the operator types) and on CI (as the merge gate). The PR is the
  final, authoritative gate; the FE pre-flight prevents most bad
  submissions from ever opening a PR.
- **Validation rules** (one Python module, two execution sites):
  - JSON-schema check on each modified row (required fields, status
    enum, canonical 12-char ID regex).
  - Sort-stability: re-running the sort on the new file must equal the
    file (catches edits that desynchronize order).
  - ID uniqueness: no duplicates introduced.
  - Status transitions: `unbound → bound`, `bound → bound (with rebind
    flag)`, `* → void` only. No back-transitions.
  - Reproducibility: the diff shows only the bound rows changing — no
    unrelated churn.
- **Auto-merge on green checks** plus one approving review (org policy
  setting). For a single-operator workflow, self-approval can be enabled
  on the part-registry repo; for multi-operator, a second human review
  is the gate.

### Shared validation: Pyodide in the FE, native Python in CI

The FE and CI must apply *exactly the same rules* — divergence between
"the form said it was OK" and "CI rejected the PR" wastes operator
time and erodes trust in the form. The cleanest way to enforce that
is to ship **one** validator implementation and run it in both places:

- **Source of truth**: `validators/` directory of pure-Python modules,
  zero non-stdlib dependencies. Functions like
  `validate_row(row) -> list[Violation]`,
  `validate_diff(old_csv, new_csv) -> list[Violation]`.
- **CI**: `python -m validators registry.csv` on every PR push, in a
  GitHub Action.
- **FE**: load Pyodide (~6 MB compressed, cached after first load),
  fetch the same `validators/` module from the repo, run the identical
  functions on the in-memory edited CSV. Live validation as the
  operator types each field; final pre-flight before opening the PR.

The alternative — parallel JS and Python implementations of the same
rules — was considered and rejected: even with the same author, they
*will* drift on the first rule change. The only way to guarantee they
don't is to make them the same code. Pyodide weight (~6 MB after
DuckDB-WASM is ~5 MB; total cold load ~11 MB) is the price of
correctness; both are cached after first install and the PWA model
absorbs the cost on day one.

If the rule set is ever observed to be small and stable enough that a
JSON Schema declaration plus a few cross-row checks would cover it,
Pyodide can be retired and the rules restated as data — but **only**
once the validator surface is mature. Starting with parallel impls
and hoping they don't drift is the trap.

### Direct printing from the FE

Three printing paths were considered for the lab's Brother QL-820NWBc
(USB / BT / Wi-Fi, DK roll labels):

| Path | Mechanism | Verdict |
|---|---|---|
| **OS print dialog** | FE renders labels in HTML at correct mm sizes, calls `window.print()` with `@page { size: 12mm auto; margin: 0 }` matching the DK roll. User picks the Brother in the OS print dialog. | **Chosen** for v1. Cross-platform (iOS via AirPrint, macOS, Windows, Linux, Android). No extra software, no per-device pairing. |
| Web Bluetooth / WebUSB direct | Implement Brother's raster command protocol over USB or BT from JavaScript | Rejected for v1. Chrome/Edge only (no iOS, no Safari). Requires implementing Brother's wire protocol. Worth revisiting if the print-dialog click becomes the dominant friction. |
| Companion print service | Small HTTP service on a lab machine that talks to CUPS | Rejected. Defeats the static-only goal. |

The Brother QL-820NWBc supports AirPrint over Wi-Fi out of the box, so
a phone PWA can print to it through the OS print dialog with no extra
configuration. Layout sizes are matched to DK roll widths via the
`dk-N` presets in `label.py` (DK-22214 = 12 mm → printable 10 mm,
DK-22210 = 29 mm → 25 mm, DK-22225 = 38 mm → 33 mm, DK-22205 = 62 mm
→ 56 mm).

**Multi-label batches**: each label is rendered as its own
`@page { size: <w>mm <h>mm; margin: 0 }` in a single multi-page print
document. On continuous DK tape, the QL-820NWBc's driver auto-cuts at
page boundaries by default (~3 mm cutter gap per label). On die-cut
DK labels (DK-1201 etc.), the printer aligns to the pre-cut boundaries.
A "single strip with crop marks, manual cut" mode is offered as a
secondary option for users who prefer tape economy over an automated
cut — it renders N labels as one long SVG with thin separator lines
between them, no auto-cut.

Concretely:

| Concern | Choice |
|---|---|
| Repo | `MorePET/part-registry` (or whichever org owns the long-term registry) |
| Data file | `registry.csv` — sorted by `id`, ASCII, line-per-part |
| Frontend | Vanilla TS or Svelte; minimal deps; built with `vite`; output to `gh-pages` branch |
| Database | DuckDB-WASM, loads `registry.csv` directly via HTTP fetch |
| Auth | GitHub OAuth device flow (public client, no secret) |
| Bind transport | GitHub REST API: create branch → commit batched diff → open PR |
| CI | GitHub Actions; validators in Python (`pytest`-style) |
| Hosting cost | Free if `part-registry` is public; **GitHub Pro / Team if private** |

## Rationale

**Static-site permanence.** GH Pages from a git repo has the same
permanence properties as ADR-012's ID-only QR: no domain decision to
regret, no server to migrate, no vendor to be locked into. If GitHub
ever raises prices or shuts down Pages, the entire registry is one
`git clone` away from being served somewhere else with no data
migration.

**WASM DuckDB is the right query engine.** For ≤100k parts, sorted CSV
loaded into DuckDB-WASM gives sub-100 ms SQL queries from a phone
browser. No backend, no cache invalidation, no n+1 query problems. We
gain analytics ("show me every PT100 in sdmd_v2 that hasn't been
recalibrated in 12 months") at zero ops cost. **Parquet on Git LFS is
deliberately deferred** — at sub-million-row scale it's premature
optimization, and it sacrifices git-diff legibility for speed we
don't need.

**PR-driven bind preserves the audit invariant.** Every bind is a
commit. `git log` answers "who bound this part to this ID and when"
forever. The cost is that bind isn't instantaneous (minutes vs.
seconds for the PR + CI cycle), but the lab-floor experience is still
"scan, fill, batch, submit" — the latency lives in the merge, not the
scan.

**Batching solves the per-bind PR problem.** The naive shape of the
design (one PR per bind) collapses on contact with operations: 30 PRs
is unworkable. The batched model — pending binds in localStorage,
single submit-as-PR — keeps the audit property while making 30-bind
sessions practical.

**GitHub OAuth device flow is the cheapest auth.** No client secret to
leak, no account directory to maintain, no password reset flow to
build. Anyone with write access to `part-registry` can bind; everyone
else gets read-only. Org-level access controls compose naturally.

## Consequences

- **`part-registry` repo extraction is the trigger to start phase 2.**
  When phase 2 implementation begins, lift `system-design/parts/` out
  of `exopet` into a new `part-registry` repo (preserving git history
  via `git subtree split` or `git filter-repo`). The ADRs ADR-012 and
  ADR-013 move with it (or are referenced by stable URL from `exopet`).
  The CSV and tooling become the registry's single source of truth.
- **Public vs private decision is load-bearing.** If `part-registry` is
  **public**: GH Pages free, anyone can read part metadata (which is
  hardware identification data — usually not sensitive). If **private**:
  needs GitHub Pro ($4/mo personal) or Team (per-user). Unless the
  registry will contain commercially sensitive sourcing data, prefer
  public; non-public BoMs / vendor pricing live elsewhere.
- **Bind operators need GitHub write access** to `part-registry`. This
  becomes a personnel onboarding step — same shape as adding someone
  to the `exopet` repo today.
- **CI validators are part of the design, not optional.** The
  permanence guarantee depends on the registry being *consistent* —
  not just preserved. Concretely, a missing CI check ("don't break
  sort order") would let a single bad commit make every diff unreadable
  thereafter. The validator suite is on the critical path.
- **PWA installability is mandatory for the lab-floor UX.** The site
  must register a service worker so operators can install it as a
  one-tap home-screen icon. Without that, every shift starts with
  finding a bookmark.
- **Phase 1 tooling stays useful.** `mint.py`, `label.py`, `bind.py`
  remain the engineer-facing CLI; the web app is the
  technician-/operator-facing interface. They both write to the same
  CSV. The CLI becomes the "power user" path for batch ops the web
  doesn't surface.

## Open questions / supersession triggers

- **Org plan**: which `MorePET` (or sibling org) tier is in effect?
  Decision depends on whether the `part-registry` repo can be public
  (no plan upgrade needed) or must be private (Pro/Team needed). Out
  of scope to resolve in this ADR.
- **Multi-org namespace**: if `part-registry` is shared across
  `exopet` / `vig-os` / `fd5`, how are project-specific bind fields
  handled — extra columns, or per-project tables? Tentative:
  per-project columns are fine until a column count threshold (~20)
  forces normalization. Re-open then.
- **Cryptographic signing of binds**: not in scope. ADR-012 noted
  tamper-evidence as a future trigger; if EXOPET parts ever require UDI
  / EUDAMED traceability, a signed-bind variant supersedes this ADR.
- **Bulk re-binding** (e.g. moving 200 parts to a different location
  after a lab move) needs a CLI escape hatch, not a 200-row form. The
  CLI from ADR-012 already supports this — the web app doesn't have
  to.

## References

- ADR-012 — Part identification: nano-id + QR labels with mint-then-bind
  workflow.
- DuckDB-WASM: <https://duckdb.org/docs/api/wasm/overview.html>
- GitHub OAuth device flow: <https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow>
- `@zxing/browser` (in-browser QR decode): <https://github.com/zxing-js/browser>
- `BarcodeDetector` API (where supported): <https://developer.mozilla.org/en-US/docs/Web/API/Barcode_Detection_API>
