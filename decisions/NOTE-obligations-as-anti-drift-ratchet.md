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
