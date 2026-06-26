# ADR-040 — Presets are a declared library; floors are gate-enforced, not compiled

- Status: Proposed
- Date: 2026-06-26
- Component / area: how collections come into existence in a registry,
  and where the "regulated floor" guarantee lives. Refines/supersedes
  **ADR-035 §0 guardrail #1** (the "code-owned preset" stance) and
  **ADR-039 §5** (floor-vs-extend), and leans on **ADR-016** (gate =
  policy authority), **ADR-038** (gate vendored / pin-verify by hash),
  **ADR-034** (CODEOWNERS / manifest), **ADR-036** (personas).
- Reviewers: Lars Gerchow (required for Accepted)
- Related: ADR-012 (id scheme floor), ADR-039 (contract engine — the
  canonical form that presets are written in), the eQMS preset family
  exploration (`platform-vs-registry.md`, `qms-vertical-register-a-part.md`),
  issue #208.

## Context

ADR-039 makes the engine generic: a registry's collections are *declared*
in its `.qx/contract.json` (data), and one Rust engine validates any
contract. But ADR-035 §0 carved out one exception — `parts` ships as a
**code-owned descriptor** (`crates/app/src/preset.rs::parts_descriptor()`)
so its tier-1 invariants (id scheme, lifecycle, required fields) are
**non-weakenable**: a deployer's contract may *extend* parts but not
weaken it, and that guarantee is enforced by living in compiled Rust.

Two problems surfaced (user review, 2026-06-26):

1. **`parts` is opinionated *at the engine level*.** Baking one domain
   into the generic fabric contradicts the whole thesis ("collections
   are data"). It also doesn't scale: an eQMS needs many domains
   (companies, personas, SOPs, orders, CAPAs, trainings, audits…), and
   coding a `_descriptor()` for each is both labor and lock-in.
2. **The compiled floor is already leaky.** ADR-039 §5 itself admits the
   floor "can be *de-facto* weakened" — a contract that simply doesn't
   instantiate the code preset, or renames around it, sidesteps it. So
   the compiled floor buys less rigor than it appears to.

The deployed-repo reality is already the right model: **what collections
a repo has is entirely what `qx init` seeds into `.qx/contract.json`** —
declared data. The only thing that *wanted* to be in code was the
*non-weakenable* property — and that is exactly what the gate
(host-enforced, run by hash, CODEOWNERS-pinned) exists to enforce.

## Decision

1. **Presets are a declared library.** `schema/presets/*.contract.json`
   is a catalog of contract fragments — `parts`, `companies`,
   `personas`, `orders`, `documents`/`sops`, `trainings`, `capas`,
   `ncrs`, `audits`, … The engine knows none of them by name. Adding a
   QMS angle = adding a preset file, not Rust.

2. **`qx init` composes presets.** `qx init --preset parts,companies,personas`
   seeds a composed `.qx/contract.json`. A deployed **"company HQ" is a
   composition of presets** — that composition *is* the spine/bootstrap
   (realizes the ADR-038 §5 "bootstrap = IQ/OQ baseline" idea concretely).

3. **A "floor" is a *declared, gate-enforced* property — not compiled
   Rust.** Floor fields / lifecycle states / id-scheme carry a marker
   in the preset (`floor: true` / `locked`). The **gate** — already
   running the released engine *by hash* (ADR-038) with contract changes
   **CODEOWNERS-pinned** (ADR-016/034) — rejects any PR whose diff
   *weakens* a floor-marked element (drops a floor field, loosens a
   floor enum, removes a floor lifecycle state, redefines the id scheme).
   The regulatory guarantee moves **from the compiler to the gate** —
   consistent with the project's "host-enforces, tool-advises,
   verify-by-hash" model everywhere else.

4. **`parts` becomes preset #1**, not a special engine citizen: the
   most-developed, floor-marked, regulated baseline in the library.
   `parts_descriptor()` is retired in favor of
   `presets/parts.contract.json` with floor markers.

## Worked example — `parts` ↔ `companies`

This is the case that motivated the ADR, resolved in the qx frame ("a
generic id-and-contract-driven entity engine; types are declared; one
gate everywhere"):

- **Both are declared collections.** `presets/parts.contract.json` and
  `presets/companies.contract.json` each declare a first-class entity
  type. The engine knows neither by name; `qx init --preset parts,companies`
  composes both into `.qx/contract.json`.
- **They link by id.** `parts` declares `manufacturer → companies` (a
  typed `reference`). A part carries a `company:…` id; the gate enforces
  the FK (can't reference a company that doesn't exist; `on_unknown`
  policy decides warn vs reject).
- **Adding a domain = declaring a preset**, never Rust. `orders`,
  `personas`, `sops` arrive the same way.
- **The only difference is floor markers.** `parts` marks its tier-1
  minimum (`id` scheme, lifecycle, required fields) `floor: true` — a
  deployer may *extend* parts but the gate rejects any PR that *weakens*
  a floor element. `companies` carries no floor → fully editable content.
  Same mechanism, one declaration apart.

So the earlier "is `companies` a code-owned preset or just example
content?" question **dissolves**: every collection is declared content;
"code-owned floor" is not a kind of collection, it's a *marker* a preset
may carry and the gate enforces. `companies` simply carries none (for
0.x). `parts` carries the regulated floor — and once this ADR lands, it
carries it *as declared markers*, not as compiled `parts_descriptor()`.

## Migration (0.x stays calm)

- Through the 0.x engine build (ADR-039 work), **keep `parts_descriptor()`
  as-is** — do not churn the engine mid-flight.
- Introduce the **library + floor-marker grammar + gate "no-weaken"
  rule** as new, additive capability.
- Then **migrate `parts` into the library** (`presets/parts.contract.json`)
  and delete the code descriptor — tracked as its own item.
- A **spike must first prove floor-enforcement-via-gate holds** (a PR
  that drops a floor field is rejected by the released gate, and the
  bypass routes — non-instantiation, rename-around — are themselves
  caught) before this supersedes the compiled floor.

## Consequences

**Positive**
- The engine is fully generic; the preset catalog scales to every QMS
  angle as *data*. This is the eQMS "preset family" (#208) made concrete
  and uniform instead of code.
- The floor guarantee becomes **auditable** (declared marker + gate +
  CODEOWNERS + by-hash) rather than an implicit property of a Rust
  binary — better for the validation dossier, not worse.
- `qx init --preset …` composition gives the "git-native eQMS bootstrap"
  a clean surface.

**Costs / risks**
- The gate gains **"no-weaken" diff logic** for floor markers (new,
  security-relevant rule — must be conservative and well-tested).
- The contract schema (ADR-039) gains a **floor-marker vocabulary**.
- A transition window where both mechanisms (compiled floor + declared
  floor) coexist until `parts` migrates.
- **Bypass surface**: declared floors only bind PRs that touch the
  contract; the gate must also reject *non-instantiation* / *rename-
  around* of a floor preset where a registry claims that preset (the
  spike's hardest case).

## Open questions (for the spike / acceptance)

- Floor-marker **grammar**: per-field `floor: true`? a preset-level
  `floor: [fields…]`? lifecycle-state-level markers? id-scheme lock?
- **Composition/conflict rules**: two presets declaring the same
  collection or field name — merge, error, or last-wins?
- How does a registry **claim** a preset (so the gate knows which floors
  apply)? A `presets = ["parts@1", …]` line in the manifest (ADR-034)?
- Does the **released gate** carry the floor rules, or do they travel in
  the (CODEOWNERS-pinned) contract? (Leaning: rules in the gate,
  markers in the contract.)
