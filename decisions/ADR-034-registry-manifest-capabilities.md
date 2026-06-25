# ADR-034 — Registry manifest + capabilities (host-enforced authz, tool advises)

- Status: Accepted
- Date: 2026-06-10
- Component / area: the per-registry policy/capability descriptor (the
  "third contract") + how authorization is enforced. Discharges ADR-020's
  deferred "externalise the authz table to config" question.
- Reviewers: Lars Gerchow (accepted 2026-06-11)
- Related: ADR-013 (data repo), ADR-016 (semantic-diff classifier),
  ADR-019 (proposal sink / PR flow), ADR-020 (identity + authz port;
  hardcoded MVP table), ADR-030 (shells, op×spoke parity §8), ADR-033
  (registry anatomy — `.qx/` home)
- Feeds: `decisions/explorations/operations-catalog.md`

## Context

ADR-030 named a third contract — *what a registry exposes and what
operations it allows* — and deferred it. ADR-020 hardcodes the
authorization table (`verified` + `qms-approver` → elevation) and
explicitly left "externalising to config" to a future ADR. The naive
path is to build a per-registry authz system: roles, approvers,
permission checks. That would **duplicate, and drift from, what the git
host already enforces robustly.**

The key realization: on a `github:` registry, GitHub already answers the
hard questions. Repo permissions decide who may open a PR or push; branch
protection requires PRs (the data repo's CONTRIBUTING already forbids
direct `main` commits); **CODEOWNERS** decides who must review which
paths. Reimplementing that in the tool is wasted effort and a drift
hazard.

## Decision

### 1. Authoritative authorization = the host's native model

On the `github:` locator, enforcement is GitHub's, not the tool's:

- **Who may propose / commit** → GitHub repo permissions.
- **PRs required, no direct `main`** → branch protection.
- **Who must review which paths** → `.github/CODEOWNERS`.

The tool does **not** reimplement any of this.

### 2. The tool *classifies and advises* (it does not gate writes)

The ADR-016 semantic-diff classifier + the ADR-020 `Authorizer` compute
an `AuthDecision` (`Allow` / `Warn` / `Block` / `RequiresElevation`) for
two purposes: the **FE/TUI preflight** (tell the operator what will
happen before they submit) and a **CI check annotation** (`pr check`
explains the classification on the PR). `RequiresElevation{ approver_role
}` **maps to a CODEOWNERS rule** — GitHub enforces the human review; the
tool just surfaced *why* it's required. So: tool classifies → CI annotates
→ CODEOWNERS + branch protection enforce.

**The gate logic is the SSoT core, not per-host.** All validation/policy
is the Rust core, so the *same* `pr check` logic runs locally on commit
(pre-commit, advisory) and in CI **from a pinned release artifact** (the
blocking gate). The host contributes only the *merge-block* mechanism —
"require this check + required reviews to merge." Enforcement therefore
decomposes into a **universal core CLI** + a **thin per-host merge-block**;
there is no per-host policy logic to reimplement.

### 3. The manifest declares policy + features (not an authz engine)

`.qx/manifest.toml` (ADR-033 anatomy) declares, per registry:

- **Identity / metadata** — registry id, name, description, owner.
- **Enabled operations at the `(op-family × collection)` grain**
  (2026-06-11 pass, per ADR-035's parameterized ops; default = all reads
  + writes-via-proposal): "no creating vendors, yes minting parts" =
  `Create{vendors}: off, Create{parts}: on`; lifecycle edges are
  addressable (`Transition{parts, →void}` is its own policy object). A
  disabled `(op, collection)` disappears from that registry's shells.
  CI validates the cross-file FK: manifest keys must resolve to
  contract-declared collections.
- **Role → capability map** — *advisory*, keyed on the same
  `{collection, op-kind}` grain (the ADR-016/020/022 unified change
  vocabulary): which change classes need elevation and which role
  approves. This is the source the CODEOWNERS seed is generated from
  (ADR-020's default table becomes the default manifest).

**No render declarations in the manifest** (single-home rule, 2026-06-11):
the *descriptor* declares what renders exist (layouts, groupings, label
fields — structure, PR-gated with the contract); the manifest only
enables/disables *operations* (policy). The former freestanding "feature
flags" dissolve into those two homes — "scan on/off" is just
`decode-image` availability.

The manifest is a thin descriptor, **not** a permission database.

### 4. Role binding = layered (IdP claims + data-repo roles file)

Roles resolve from both, per the user decision:

- **IdP claims** (ADR-020 `Operator.claims`) where the IdP provides them
  (e.g. GitHub team membership, OIDC groups).
- **`.qx/roles.toml`** — a self-describing, versioned,
  PR-reviewed map (`operator-id` / team → roles) that is IdP-agnostic and
  auditable.

On `github:` the **teeth** are CODEOWNERS + branch protection; the
roles/claims drive the *advisory* layer and the **non-GitHub / local**
path (§5).

### 5. The enforcement asymmetry (stated plainly)

- **`github:`** (and future hosted transports — GitLab approval rules,
  Gitea, …): **host-enforced** authz. Strong.
- **`file://`** local / direct-commit: **no PR, no CODEOWNERS** — it is
  **local trust + tool-advisory only**. The classifier still warns, but
  the gate there is the operator's filesystem + git access, not the tool.
  This is an honest, documented limitation, not a bug.

### 6. Bootstrap configures the teeth

Creating a `github:` registry sets **branch protection** (require PR +
required reviews + required check) and **seeds CODEOWNERS** from the
manifest's role map — a `bootstrap-data-repo` responsibility (gh / API
with an admin token). A GitHub App is a later scale upgrade (ADR-030),
**not** required now. Without this step the host has no teeth, so it is
part of "a correctly deployed registry."

## Rationale

Rebuilding repo permissions + reviewer routing would duplicate what
GitHub enforces well and inevitably drift from it. CODEOWNERS is the
native "who must approve which paths" — an exact fit for "destructive
changes need a `qms-approver`." The tool's real value is **semantic
classification** (a CSV diff → an audited decision) and **advice**
(preflight + CI annotation), not gatekeeping writes. Keeping the manifest
a thin policy/feature descriptor — with layered roles so non-GitHub
backends still have an auditable model — discharges ADR-020's open
question by externalizing the table while delegating enforcement to the
host. The asymmetry is acceptable because `file://` is a single-operator
local-trust context by definition; multi-operator registries live on a
host that enforces.

## Consequences

- **ADR-020's hardcoded table becomes the default manifest**;
  `RequiresElevation.approver_role` ↔ a CODEOWNERS team.
- **Bootstrap gains responsibilities**: set branch protection + generate
  CODEOWNERS from the manifest (else "no teeth").
- **FE/CI surface the `AuthDecision`; GitHub enforces it.** The classifier
  runs in both preflight and CI (ADR-016), but the merge gate is the host.
- **`file://` local mode is explicitly weaker** and must be documented as
  local-trust + advisory.
- **Manifest-disabled `(op, collection)` pairs** drop out of a registry's shells — note
  the interaction with ADR-030 §8: the parity matrix is over the *tool's*
  catalog; a registry's manifest may legitimately disable a subset
  (parity = "every shell offers every *enabled* op", not "every op
  always").
- **`manifest.toml` + `roles.toml`** live in `.qx/` (ADR-033).

## Corrections

> **2026-06-11:** four refinements from the audit-identity/anchoring
> session: (1) §4's `roles.toml` is retired as a separate artifact —
> role bindings live in the **`personas` collection** (ADR-036 §1),
> which the CODEOWNERS seed generates from; (2) §5's `file://`
> local-trust asymmetry is removed from the default product — direct-
> write local mode is a flagged future feature (ADR-036 §5; Corrections
> on ADR-030/033); (3) elevation precision: `RequiresElevation` is
> satisfied by a deliberate act under a **2FA-enrolled,
> host-authenticated** identity — *not* a fresh-per-act MFA challenge,
> which GitHub OAuth cannot force or prove for third-party apps
> (ADR-036 §4; fresh-per-act proof is escalation rung E2); (4) §2's
> pinned-artifact gate gains a location story (vendored in the data
> repo, ADR-038 §1) and its run is recorded as evidence in the stream
> (ADR-037 §2). Original text preserved above for audit.

## Open questions / supersession triggers

- **Protection drift self-audit** *(approach resolved 2026-06-11 —
  CI-only, no App)*. Branch protection is a repo *setting*, not a
  committed file, so it can drift silently — a registry that *looks*
  gated but isn't. Resolution: per-PR `pr check` + a scheduled **cron
  Actions** workflow (granted `administration: read`) that audits
  protection + CODEOWNERS + the check workflow against `manifest.toml`
  and **warns / opens an issue** on drift (no auto-heal). No GitHub App
  yet (deferred to ADR-030 scale-up). Accepted caveat: a repo whose
  protection was weakened could also weaken its own audit workflow — the
  App is the eventual hardening. *(Still pending: building it.)*
- **Non-GitHub transport enforcement** — the gate *logic* is the same
  core CLI everywhere (§2 "SSoT core"); a host supplies only a
  *merge-block*. Tiers: **T1** native merge-block + required review
  (GitHub branch-protection + CODEOWNERS; later GitLab MR-approvals /
  Gitea), **T2** CI check but no merge-block (advisory), **T3**
  `file://` / bare git (local-trust + advisory). A registry records its
  tier so its guarantee is explicit; host adapters are deferred until a
  non-GitHub deployment is filed — the abstraction is thin (merge-block
  presence), not per-host logic. Map onto GitLab
  approval rules / Gitea / a generic webhook sink when those adapters land
  (ADR-019 future adapters).
- **Thin GitHub OAuth claims** — team membership isn't in the default
  OAuth claim set without an extra API call; decide whether the
  github_oauth adapter fetches teams to populate `claims`.
- **Can the manifest *tighten* beyond CODEOWNERS** (hard-block an op for
  everyone)? Yes via the enabled-ops allow-list; confirm that's the only
  tightening knob.
- **Disabled-op presentation** — hidden vs visibly-disabled in each
  shell.

## References

- ADR-016 — PR-diff policy (the classifier that feeds the decision)
- ADR-019 — Proposal sink / PR flow
- ADR-020 — Identity & authorization port (default table → default manifest)
- ADR-030 — Shells + op×spoke parity (§8)
- ADR-033 — Registry anatomy (`.qx/` home)
- GitHub CODEOWNERS — <https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners>
- GitHub branch protection — <https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches>
