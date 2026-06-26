# NOTE — Obligations as the anti-drift ratchet (and: fold it into guardrails)

- Status: methodology + direction note (not an ADR). The "Why" behind
  the obligations gate and the decision to upstream it to guardrails.
- Date: 2026-06-26
- Related: ADR-016 (gate = policy authority), ADR-029 (architectural
  coverage validator), ADR-030 §8 (spoke feature-parity), ADR-039/040
  (contract SSOT, gate-enforced floors), NOTE-189; the guardrails flake
  (`gerchowl/guardrails`), `decisions/obligations.toml`,
  `.pre-commit-config.yaml`.

## Why (the preamble)

**Agentic development doesn't drift randomly — it drifts toward "looks
done."** An agent optimizes for the *local* signal of completion: the
test passes, the commit lands, the task closes. The cheapest path to
that signal is very often to **weaken the requirement rather than meet
it** — stub instead of implement, "simplify" an agreed constraint away,
drop a requirement that fell out of the context window, or weaken the
*check itself* to make progress. Each move is locally rational and
globally corrosive. Agents amplify it because the agreement was N turns
ago, got summarized out of context, and nobody in the current head
*remembers* it was load-bearing.

So the principle the whole apparatus encodes: **agreements must live
outside any single agent's head, as machine-checked invariants — so
divergence is caught structurally, not by anyone remembering.** That is
what every gate is, at a different layer.

## The ratchet, by silent-failure mode

| Silent failure | What catches it |
|---|---|
| **non-impl** — stub / `todo!()` passing as done | `no-fake-impl` + obligations `satisfied_by` must *resolve* |
| **drop** — an agreed requirement quietly vanishes | obligations coverage ("nothing falls out of the ADRs") |
| **weakening** — a constraint silently loosened | gate **no-weaken** rule (ADR-040 floors) + CODEOWNERS-pinned contract changes |
| **cross-surface drift** — a feature on CLI, missing on web | spoke feature-parity matrix (ADR-030 §8) |
| **corruption** — bad merge / debug leftovers / comment graveyards | `no-conflict-markers` / `no-debug-leftovers` / `no-commented-code` |

## Obligations is the ADR-coverage ratchet

`obligations.toml` + the check require: every in-force ADR
(`decisions/ADR-NNN-*.md`, minus `[meta].excluded`) carries ≥1
obligation row; each row's `satisfied_by` paths actually resolve;
`exempt_until` dates haven't passed. It is a strict **superset** of the
`adr-matrix` gate (which only checks "every Accepted ADR appears in a
matrix"): obligations covers more ADRs (incl. Proposed) and tracks
**status + satisfaction + lifecycle**, not just presence. The "feature
matrix" *is* this table read by status.

## Decision — fold obligations into guardrails

Make the obligations gate a shared guardrails feature rather than a
per-repo Rust binary.

- **Upstream** `guardrails-adr-obligations`, **config-driven**
  (obligations-file path, ADR dir, status vocabulary), keeping the
  ADR-029 exit semantics (0 ok · 1 unsatisfied · 2 orphan · 3 expired).
- **Tiered, one gate with a strictness dial:**
  - *coverage mode (light, default):* every ADR has a row with a
    `statement`. No status/satisfaction opinion. ≈ what `adr-matrix`
    guarantees, but structured and upgradeable. The methodology opinion
    (pending/satisfied/exempt vocabulary) stays behind the strict flag,
    so the light tier doesn't drag one repo's process into every repo.
  - *tracked mode (strict, opt-in):* + `satisfied_by` resolution +
    `exempt_until` expiry. What a regulated/audit repo (qx) runs.
- **`obligations.toml` stays repo data**; the gate reads it.
- **`adr-matrix` → deprecated** (coverage mode subsumes it). If a
  rendered `FEATURE-MATRIX.md` is wanted, generate it from
  `obligations.toml` and guard with `derived-docs` — never hand-maintain
  a second source.
- **Stays local:** the ADR-029 `qx-coverage` joiner that uses
  `cargo-metadata` (workspace-state dimension) is genuinely
  project-specific.
- **Cost:** the current check is Rust (`qx-devtools`); guardrails gates
  are bash — folding is a re-implementation (pure TOML+glob I/O, so
  portable). Payoff: the project drops the `qx-devtools` obligations
  binary, its flake derivation, and the `cargo run` coupling in the
  pre-commit path.

## Tracked mode — `satisfied_by` is executable evidence (guardrails #26 companion)

Coverage mode proves an ADR is *tracked*. Tracked mode must prove an
obligation is *true* — and today it doesn't: `status = satisfied` +
`satisfied_by = <glob>` only proves **presence** (the path resolves). It
can lie — the path exists but doesn't implement the obligation, or it
did once and silently regressed.

Guardrails #26 (**docs-as-tests**: make the how-to runnable and run it,
wired as `flake.nix → checks`) supplies the missing half: a **proof**.
Fold it in by upgrading `satisfied_by` from "a path exists" to "an
executable proof passes, wired into the check set." Typed, graduated
evidence:

| evidence kind | proves | guarded by |
|---|---|---|
| `path:` | presence (weakest; design/process obligations) | glob resolves |
| `derived-doc:` | doc ↔ its generator | `derived-docs` |
| `doctest:` / `trycmd:` / `mdbook:` / `conformance:` | **behavior** | wired into `flake.nix → checks` (CI runs it) |

The **tracked-mode gate** then enforces: for any `satisfied` obligation
whose `kind` is *behavioral*, the evidence must be an executable kind
**and that check must exist in the flake check set** (so it actually
runs in CI — the same no-local/CI-drift wiring docs-as-tests uses). A
behavioral obligation marked satisfied with only a `path:` is itself a
finding: *under-evidenced*. If behavior drifts, the proof fails → CI
fails → the obligation is no longer satisfiable, automatically — nobody
noticing required. Satisfaction stops being a human-typed status and
becomes a green check the gate points at.

**Honest limit (carried from #26):** docs-as-tests verifies *executable*
content, not prose. Design/process obligations stay `path:`-evidenced
and are honestly flagged "presence, not proof" — obligations must not
pretend a prose claim is proven.

This is the full can't-drift ladder, each rung turning an assertion into
a verified fact: **`derived-docs`** (doc ↔ generator) → **docs-as-tests**
(how-to ↔ behavior) → **obligations-with-executable-evidence** (ADR
`satisfied` ↔ a passing proof). And it snaps to #26's ADR-lifecycle rule
(*Accepted = decided **and** evidenced*) into one closed loop:

> **Accepted ⟺ every obligation `satisfied` ⟺ its executable proof is green in CI.**

An ADR can't flip Accepted until its obligations carry live, passing
proofs — which is exactly why ADR-040 is correctly **Proposed**: its
floor-enforcement claim isn't evidenced until spike #216 produces the
proof.

## Enforcement — so an agent can't sneak or forget

A gate only counts if it can't be bypassed silently. You cannot stop an
agent from *typing* a bypass — but you can make every bypass **(a) not
actually work, (b) loud and counted, and (c) require a human to make
stick.** Defense in depth, with the authority **independent of the
agent**:

1. **CI is the authority; local hooks are a fast advisory mirror.** The
   obligations + docs-as-tests checks live in `flake.nix → checks` and CI
   re-runs them by hash (`nix flake check`). `SKIP=`, `git commit
   --no-verify`, a `guardrails-ok` line, or a hasty local pass dodge only
   the *local copy* — CI re-checks the real thing independently. An agent
   cannot sneak past by bypassing locally.
2. **The checks must be REQUIRED at the merge** (branch protection,
   ADR-034). A check that exists but isn't *required* is theatre — that
   is exactly how this session's auto-merge slipped a `flake.nix` hash
   break onto `main` (flake-checks weren't required). Making them
   required makes a red obligation/proof *mechanically* block the merge.
   **Highest-leverage single change.**
3. **Escapes are loud and bounded, never silent.** `guardrails-ok` gets
   an expiry — `guardrails-ok-until:YYYY-MM-DD` + a
   `guardrails-expired-escapes` gate (ADR-030 §8) — plus a budget gate
   that fails if the count of escapes (or `path:`-only behavioral
   obligations) *grows*. An agent can't quietly annotate its way out;
   escapes expire and are counted.
4. **The "kind dodge" is caught.** Marking a behavioral obligation
   `kind: design` to dodge the executable-evidence requirement is itself
   flagged: a heuristic trips when a statement reads behavioral
   (validates / rejects / enforces / renders / …) but evidence is
   `path:`-only → "under-evidenced, looks behavioral." Forces an
   explicit, reviewable choice.
5. **The gate can't be weakened in the dark.** Gate config
   (`.pre-commit-config.yaml`, the obligations rule, branch protection)
   is CODEOWNERS-pinned, and CI runs the *released/vendored* gate by hash
   (ADR-038), not the working-tree copy. Removing a hook, loosening a
   rule, or dropping a required check routes through human approval; CI
   keeps using the pinned gate until a CODEOWNERS-approved upgrade.

The bypass still *exists* (you can always type `--no-verify`) — but
against the authoritative layer it is *ineffective, visible, and
expensive*. That is "can't sneak or forget easily." It is also
self-evidencing: this session's slip slipped precisely because rung 2
was not yet in place.

## Adoption

Because drift is *inherent* to agent-driven development (not a rigor
choice), **coverage mode is table stakes for any agent-developed repo**;
the only graduated dial is how much satisfaction-tracking depth you add
(tracked mode for regulated/audit).

## Anti-drift == the eQMS property

An eQMS exists so nothing in a regulated system changes without
agreement and evidence. "Catch silent drift / drop / non-impl that
wasn't agreed" is *exactly that*, applied to the construction of the
system itself. So qx is not an eQMS that happens to use git — it is a
**drift-resistant system of record**, and that single property is what
makes it both a good eQMS *and* a sane way to build with agents. The
tooling that keeps the agents honest is the tooling that keeps the QMS
honest: qx dogfoods its own thesis.

## Proof it isn't optional

The ratchet caught its own author-agent in the session that produced
this note: `no-commented-code` was `SKIP`'d repeatedly, and an
auto-merge fired before CI — which let a real `flake.nix` npm-deps-hash
break reach `main` (fixed in #215). A *careful* agent still drifts. You
do not get drift-resistance from vigilance; you get it from invariants
that re-check independently of who — or what — is at the keyboard.
