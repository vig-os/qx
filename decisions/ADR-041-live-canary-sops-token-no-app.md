# ADR-041 — Live authz canary: a SOPS-encrypted token, no GitHub App

- Status: Proposed
- Date: 2026-06-30
- Component / area: how the tool *proves* — against real, ephemeral
  GitHub repos — that its host-enforced authorization (ADR-034) and its
  PR-diff gate (ADR-016) actually bite, with the fewest possible moving
  pieces and one command that runs identically on a laptop and in CI.
  Refines ADR-034 §5-6 (the enforcement asymmetry + "bootstrap
  configures the teeth" — the canary is the *test* that the teeth bite),
  and rewrites ADR-030 §7 / the `canary-pipeline` obligation (the
  `qx-provisioner` GitHub App demotes from "required" to "deferred scale
  upgrade").
- Reviewers: Lars Gerchow (required for Accepted)
- Related: ADR-016 (pr-diff policy gate), ADR-024/025 (repro + signed
  releases — the canary exercises a *released* gate), ADR-030 (multicall
  `qx`, per-shell auth, §7 provisioner App), ADR-034 (host-enforced
  authz — the claim this ADR proves), ADR-037/038 (gate provenance +
  vendoring — the artifact the canary runs)

## Context

ADR-034 decided that authorization is **host-enforced**: GitHub branch
protection + CODEOWNERS + a required PR-diff check are the teeth; the
tool only classifies and advises. ADR-016 decided that registry changes
are gated by `qx check --diff` in CI. Both are *claims about a
configuration*. An untested claim about a regulated control is a
liability, not a control — the audit question is "show me it blocks a
bad change," not "show me the YAML that should."

So the tool needs a **live canary**: a pipeline that stands up a real
repo, wires the protections, and then *tries to break through them* —
golden-path changes must merge, malformed/authz-violating changes must
be blocked by the host, not merely frowned at by the tool. Constraints
inherited from the regulated posture (see [[qx-concept]]):

- **Few moving pieces / everything in the repo / one path everywhere.**
  A verification apparatus that only runs in a bespoke CI context, or
  depends on an external service that can rot over a 10-15yr retention
  horizon, is itself a liability.
- **Ephemeral, never the real registry.** The canary must create and
  tear down throwaway repos; a red-test that mutates a live registry is
  not a test.
- **Local == CI.** The same command a developer runs must be the command
  CI runs — divergence is where "green in CI, broken on the desk" lives.

The `canary-pipeline` obligation originally NAMED a `qx-provisioner`
GitHub App as the authenticator. But ADR-030 §7 already says the default
`GITHUB_TOKEN` suffices and **the App is the scale/UX upgrade** — so the
App was always optional and the obligation over-specified. A GitHub App
is also exactly the kind of external moving piece the posture warns
against: an installation, a private key, a webhook surface, a thing that
can be uninstalled out from under the audit.

## Decision

### 1. The canary is a command, not a bespoke workflow

`qx canary` (and a thin `just canary`) is the whole pipeline. CI invokes
the *same* command a developer invokes. There is no canary logic in
workflow YAML beyond "check out, provide the age key, run `qx canary`."
This is the ADR-034/016 "one gate everywhere" principle applied to the
gate's own test harness.

### 2. Auth = a SOPS-encrypted admin PAT; the age key is the only secret

- A fine-grained, **vig-os-owned** admin PAT (create/delete repos +
  administration on throwaway repos only) is committed to the repo,
  **SOPS-encrypted with age**.
- The **age private key is the single environment-provided secret**:
  the developer's keyring locally; one `GH_AGE_KEY` → `SOPS_AGE_KEY`
  secret in CI. Nothing else is injected.
- `qx canary` decrypts the PAT at runtime via SOPS. The *decryption
  path* is identical local and CI; only the age-key *source* differs.

This keeps the secret surface minimal (one key, rotatable, and the
encrypted blob is auditable in git history) and removes the App's
installation/webhook surface entirely.

### 3. What a run does

Per invocation, against a freshly created ephemeral repo:

1. **Provision** — create `vig-os/qx-canary-<runid>`, seed it via
   `qx init` / bootstrap, configure branch protection + CODEOWNERS + the
   required `qx check --diff` gate (the released, pinned gate — ADR-034
   §6, ADR-038 vendoring).
2. **Golden path** — `mint → open PR → qx check --diff (green) → merge →
   assert the record landed`. Proves a *valid* change flows.
3. **Red matrix** — drive the conformance corpus as **real PRs**:
   malformed diffs, authz-violating edits, floor-weakening attempts.
   Each MUST be blocked *by the host* (required check red / CODEOWNERS),
   not merely flagged by the tool. A red PR that merges is a canary
   failure.
4. **Teardown** — `always()` delete the ephemeral repo; an **orphan
   sweeper** removes any `qx-canary-*` repos leaked by a killed run.

### 4. The impurity boundary

A pure `nix flake check` is sandboxed (no network, no secrets), so the
*live* canary cannot live inside it. The split:

- **Offline logic** (corpus construction, expected-verdict tables, the
  provisioning plan) is pure and `flake check`-able.
- **The live run** is an explicitly impure command that CI shims with
  the age key. This is honest about what needs the network + a token,
  and keeps the sandboxed check fast and deterministic.

### 5. Evidence chain

A green canary run is the **satisfied-evidence** for a cluster of
otherwise-hard-to-prove obligations, because it exercises them through a
real host:

- `pr-diff-policy-gate` (a real PR is blocked by the required gate),
- `protection-drift-selfaudit` (protection + CODEOWNERS asserted present
  and enforcing),
- host-enforced-authz (the ADR-034 teeth demonstrably bite),
- `spoke-feature-parity` (a seeded data-repo runs the same gate).

### 6. The App is deferred, not chosen-against forever

The `qx-provisioner` GitHub App remains a **scale/UX upgrade** (ADR-030
§7): if per-developer PAT management or org-wide provisioning becomes a
burden, the App is the answer. Until then it is a deferred option, and
ADR-034's base remains "an admin token." Nothing here forecloses it.

## Rationale

Two auth contexts must not be conflated (the clarity that matters):

- **(A) A tenant's deployed eQMS** authenticates with *their own* gh
  login (ADR-030 credential resolver) — self-contained, zero secret from
  us (this is the ADR-038 forking keystone).
- **(B) The canonical repo's canary** testing the *tool* uses the
  vig-os-owned SOPS'd PAT, and it only ever touches throwaway sandboxes.

The SOPS-token model wins on the posture's own terms: the encrypted PAT
+ one age key is fewer moving pieces than an App (no installation, no
webhook, no separate key service), everything material lives in the repo
(the encrypted blob is in git; the plaintext never is), and one command
runs everywhere. "Test the validator by making it validate real bad
PRs" is the same spirit as ADR-038's incumbent-validates-succession.

## Consequences

- `qx canary` + `just canary` join the CLI surface; a `.sops.yaml` and
  an encrypted PAT blob join the repo; `GH_AGE_KEY` joins CI secrets.
- The one human, one-time step (the **user-wall**): generate the age
  keypair, mint the vig-os fine-grained PAT, SOPS-encrypt it, add
  `GH_AGE_KEY` to CI. Everything downstream of that is code.
- Ephemeral repos cost API calls + brief namespace churn under
  `vig-os/qx-canary-*`; the orphan sweeper bounds leakage.
- Needs the first pushed release tag (the canary runs a *released* gate)
  — so it is sequenced after reproducible-signed-releases /
  distribution-integrity land.

## Open questions / supersession triggers

- Exact fine-grained PAT scopes (create/delete repo + administration;
  confirm the minimum that still lets branch-protection be set).
- Whether the orphan sweeper runs as part of every `qx canary` or as a
  separate scheduled pass.
- Whether local runs should default to a *personal* throwaway namespace
  to avoid contending on the shared vig-os PAT during development.
- If PAT/secret management becomes a burden at scale → revisit the
  deferred `qx-provisioner` App (ADR-030 §7).

## References

- ADR-016 (PR-diff policy gate), ADR-030 §7 (provisioner App, per-shell
  auth), ADR-034 §5-6 (enforcement asymmetry, bootstrap configures the
  teeth), ADR-038 (gate vendoring — the artifact the canary runs).
- Decision captured 2026-06-30; see the `canary-pipeline` obligation and
  [[deploy-canary-forking-architecture]].
