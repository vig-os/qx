# ADR-024 — Cryptographic baseline (MVP)

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — concrete implementation of the
  MVP cryptographic controls fixed by ADR-023; defines the
  `signing/` workspace crate (per ADR-017) and the branch-protection
  configuration that enforces the controls in CI
- Reviewers: Lars Gerchow
- Related: ADR-016 (PR-diff policy), ADR-017 (Rust core), ADR-018
  (Storage port), ADR-019 (Proposal sink), ADR-020 (Identity port),
  ADR-022 (Observability + audit), ADR-023 (Threat model + crypto-MVP
  scope), ADR-025 (Distribution integrity), ADR-027 (Conformance
  tests)

## Context

ADR-023 fixed the threat model and the boundary between the MVP
cryptographic posture and the deferred (Sigstore-keyless,
Rekor-anchored, hash-chained) posture, with named re-open triggers
T1–T6.

This ADR is the **implementation companion** to ADR-023. It pins
the exact mechanisms — git-side and Rust-side — that deliver the
MVP controls and that preserve the bolt-on path for the deferred
ones.

Upstream constraints:

- **ADR-017** — signing lives in `crates/signing/`; compiles to
  native (CLI) and `wasm32-unknown-unknown` (FE) without divergence.
- **ADR-019** — proposals carry signatures end-to-end; the data
  shape accepts whatever variant the active `SigningProvider` emits.
- **ADR-020** — `Operator.pubkey: Option<PubKey>` is plumbed through
  for forward-compat; the MVP adapter does not populate it.
- **ADR-022** — `AuditEntry.signatures: Vec<Signature>` is round-
  tripped blindly by storage; MVP populates one `GitCommit` per entry.
- **ADR-023** — MVP is git-only signing + branch protection +
  reproducible builds + forward-compatible trait shapes.

The engineering question this ADR answers: **how**, concretely, the
signing trait, the MVP adapter, branch protection, and the
reproducible-build pipeline are built so that (a) they enforce the
MVP controls today and (b) the deferred adapters drop in by adding
a crate, not by refactoring callers.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo** — no signing, no branch protection beyond GitHub defaults, `--operator $USER` strings in CSVs | Zero engineering | Indefensible per ADR-023 §"Decision"; fails the QMS audit posture in [`METHODOLOGY.md`](METHODOLOGY.md); cannot distinguish a forged row from a real one | Rejected |
| **Sigstore-everywhere from day one** — per-row Fulcio signing, Rekor anchoring, self-hosted Sigstore stack, Cosign for binary releases | Maximum audit-grade rigour from the first commit; no future migration | 2–3 weeks engineering before any feature work resumes; ongoing self-hosted Fulcio + Rekor ops; over-builds for the regulatory tier ADR-023 fixed | Rejected — preserved as the deferred upgrade path per ADR-023 trigger T2/T3 |
| **Hybrid** — git for commits + Sigstore-keyless for per-row attribution + Rekor for long-term anchor | Honours "no operator key infrastructure" UX constraint via Sigstore's keyless flow; rigorous | ~1 week to stand up self-hosted Sigstore; ongoing ops; deferred per ADR-023 | Rejected for MVP — kept as the documented next step |
| **Git signed commits + branch protection + signed tags + reproducible builds + forward-compatible trait shapes** | MVP scope per ADR-023; bolt-on path preserved by `SigningProvider` / `VerificationProvider` / `Signature` enum; ~2 hours of CI + branch-protection setup; lean on key infrastructure operators already have (GitHub-registered SSH/GPG) | Long-term verifiability tied to GitHub remaining authoritative; per-commit attribution, not per-row; CI bot key compromise = forged signature within bot scope | **Chosen** — directly implements the MVP scope ADR-023 fixed |

## Decision

The MVP cryptographic baseline is five concrete commitments,
each implementing a numbered item of ADR-023 §"MVP crypto scope":

1. **Signed commits required on `main`** of both governed repos
   (code + data, per ADR-019). GitHub branch protection enforces;
   CI also runs `git verify-commit` defence-in-depth.
2. **Branch protection** captured as a checked-in declarative file
   (`.github/branch-protection.yaml`). The file is the source of
   truth; a CI apply-job reconciles GitHub settings to it.
3. **Signed git tags** (annotated, GPG- or SSH-signed) for every
   `v*` release. The release manager's key is the only signing
   surface for tags; CI rejects unsigned tags.
4. **Reproducible Rust builds** via pinned toolchain
   (`rust-toolchain.toml`), pinned `wasm-bindgen-cli`, checked-in
   `Cargo.lock`, deterministic build profile, and a CI matrix that
   builds each release tag on two independent hosts and compares
   artifact hashes. Mismatch = release blocked.
5. **`SigningProvider` + `VerificationProvider` traits** in
   `crates/signing/`, with `Signature` shaped so
   `signing_sigstore_keyless` is one new variant + one new crate,
   not a refactor. MVP adapter `signing_git_commit/` records the
   binding from `AuditEntry` to the commit SHA it landed in.

Deferred per ADR-023 (not built today): per-row Sigstore-keyless
signing, self-hosted Sigstore infrastructure, hash-chained audit
log (`chain_hash` column reserved per ADR-027 but algorithm not
implemented), Cosign-signed binaries beyond signed tags, WebAuthn
/ passkey-bound action signing.

## Trait shape

The trait surface in `crates/signing/src/lib.rs` is fixed by this
ADR. Future adapters add variants and crate folders; they do not
change the traits.

```rust
#[non_exhaustive]
pub enum SigAlgorithm {
    GitCommitGpg,
    GitCommitSsh,
    SigstoreKeyless,    // future
    Cosign,             // future
}

#[non_exhaustive]
pub enum Signature {
    GitCommit { commit_sha: String, signer_key_id: KeyId },
    Sigstore {
        cert: Vec<u8>,
        sig: Vec<u8>,
        rekor_proof: RekorProof,
    },                  // future
}

pub struct SigningContext<'a> {
    pub operator: &'a Operator,    // ADR-020
    pub payload: &'a [u8],
    pub action: ActionKind,        // ADR-022
    pub timestamp: Timestamp,
}

pub trait SigningProvider {
    fn algorithm(&self) -> SigAlgorithm;
    fn sign(&self, ctx: &SigningContext<'_>) -> Result<Signature, SignError>;
}

#[non_exhaustive]
pub enum VerificationSource {
    GitVerifyCommit,
    GitHubVerifiedApi,
    SigstoreRekor,             // future
}

pub enum Verification {
    Verified { at: Timestamp, source: VerificationSource },
    Unverified { reason: String },
    Invalid { reason: String },
}

pub trait VerificationProvider {
    fn algorithms(&self) -> &[SigAlgorithm];
    fn verify(&self, payload: &[u8], sig: &Signature, op: &Operator)
        -> Result<Verification, VerifyError>;
}
```

The MVP `signing_git_commit` adapter is **not** a cryptographic
implementation. Git does the cryptography when the commit is
created (operator's GPG/SSH key). The adapter's `sign()` consults
the commit context and records the binding from `AuditEntry` to
commit SHA:

```rust
impl SigningProvider for GitCommitSigner {
    fn algorithm(&self) -> SigAlgorithm { self.detected_algorithm }

    fn sign(&self, ctx: &SigningContext<'_>) -> Result<Signature, SignError> {
        let commit_sha = self.repo.pending_commit_sha(ctx)?;
        let signer_key_id = self.repo.signer_key_id_for(ctx.operator)?;
        Ok(Signature::GitCommit { commit_sha, signer_key_id })
    }
}
```

Verification dispatches to `git verify-commit` (CLI) or GitHub's
verification API (FE). All other crates depend on
`Box<dyn SigningProvider>` / `Box<dyn VerificationProvider>`;
wiring a new adapter is a one-line change in `crates/cli/` or
`crates/wasm/`.

## Branch protection configuration

Branch protection rules live in
`.github/branch-protection.yaml` (or `branch_protection.tf` if
Terraform is already in play for repo settings — to be picked at
land time). The file is the source of truth; an apply-job in CI
reconciles GitHub's settings to it on merge to `main`. Drift
between the file and the live settings is a CI failure.

Required protections on `main` of both repos:

```yaml
# .github/branch-protection.yaml — illustrative shape
branch: main
protections:
  require_signed_commits: true
  required_pull_request_reviews:
    required_approving_review_count: 1
    dismiss_stale_reviews: true
    require_code_owner_reviews: true        # composes with CODEOWNERS
  required_status_checks:
    strict: true                             # branch must be up-to-date
    contexts:
      - validators
      - conformance-tests                    # per ADR-027
      - semantic-diff-classifier             # per ADR-016
      - reproducible-build-host-1            # per §Reproducible builds
      - reproducible-build-host-2
  enforce_admins: true                       # no admin bypass
  allow_force_pushes: false
  allow_deletions: false
  required_linear_history: true              # rebase/squash, no merges
  block_creations: false
  lock_branch: false
```

`required_linear_history: true` is **recommended** — it keeps the
audit-log chain readable and pairs naturally with squash-merge of PR
diffs (ADR-016's authority artifact). It is debate-open at review:
some projects prefer merge commits for review-thread preservation.
The default in this ADR is linear; reviewers may overturn.

`enforce_admins: true` is **non-negotiable** for the MVP. ADR-023's
threat model explicitly includes "insider with repo write access";
permitting admin bypass collapses that defence.

## Reproducible builds

Anyone must be able to rebuild from a tagged commit and obtain the
same artifact hash.

Pinning surface:

- `rust-toolchain.toml` at workspace root — channel, components,
  targets. Toolchain bumps are PRs.
- `Cargo.lock` checked in (per ADR-017).
- `wasm-bindgen-cli` version pinned to match the `wasm-bindgen`
  crate version (mismatch produces non-deterministic JS glue).
- Build profile in workspace `Cargo.toml`:

```toml
[profile.release]
codegen-units = 1
lto = "fat"
strip = "symbols"
panic = "abort"
# RUSTFLAGS="--remap-path-prefix=$HOME=. --remap-path-prefix=$CARGO_HOME=." in CI
```

CI matrix on every release tag:
- `build-host-1` (e.g. `ubuntu-24.04` GitHub-hosted) and
  `build-host-2` (e.g. `ubuntu-24.04-arm` or a self-hosted runner)
  build the same tag with identical `RUSTFLAGS` and produce
  `sha256` hashes per artifact. A reconcile job compares hashes;
  mismatch = release blocked.
- The same matrix runs per-PR as advisory; only release tags block.

Drift between hosts is escalated as a release-blocking issue.
Common causes documented: unpinned `cc` toolchain, `glibc`
differences (mitigated by musl target), embedded build timestamps
(mitigated by `SOURCE_DATE_EPOCH`).

## CI verification jobs

Three new CI jobs land alongside this ADR. All are merge-blocking on
PRs against `main`.

1. **`verify-signed-commits`** — runs `git verify-commit` on every
   commit in the PR range. Rejects PRs that contain an unsigned or
   un-verifiable commit. Cross-references against GitHub's "Verified"
   API for defence-in-depth.
2. **`verify-signed-tags`** — runs on tag-push events. Runs
   `git verify-tag $TAG`; rejects unsigned or unverified tags.
   Required before the reproducible-build matrix is dispatched.
3. **`branch-protection-drift`** — reads
   `.github/branch-protection.yaml`, compares to the live GitHub
   branch protection settings via the GitHub API, and fails if they
   differ. Runs on every push to `main` and nightly.

The semantic-diff classifier required by ADR-016 already runs on
every PR; it is unaffected by this ADR but is listed in the branch
protection `required_status_checks` as a required gate alongside the
new ones.

## Forward-compatibility — how Sigstore drops in

The `Signature` enum is `#[non_exhaustive]` and already reserves the
`Sigstore { cert, sig, rekor_proof }` variant. The future
`signing_sigstore_keyless` adapter is a new crate
(`crates/signing_sigstore_keyless/`) implementing
`SigningProvider` and `VerificationProvider`. Adoption is:

1. Add the crate; implement the two traits.
2. Wire it into `crates/cli/` and `crates/wasm/` behind the
   `signing.provider = "sigstore_keyless"` config key (per ADR-021's
   12-factor configuration).
3. Storage adapters need no change — `AuditEntry.signatures` is
   already `Vec<Signature>` and the round-trip conformance test
   (per ADR-027) already covers a Sigstore-shaped fixture.
4. Operator workflow gains an OIDC step at sign-time; no GPG/SSH
   key required for non-developer operators (resolves trigger T3).

The `chain_hash: Option<Hash>` column on `AuditEntry` is reserved
per ADR-023 §"MVP crypto scope — fixed" item 6. The chaining
algorithm itself is not implemented today; activating it (per
trigger T5 or T2) is a subsequent ADR that picks the algorithm
(Merkle, blockchain-style linear hash, …) and a backfill strategy.

## Rationale

**Why git's signing surface for the MVP.** ADR-023 fixed the threat
model and concluded the adversaries in scope (external, insider
with repo write, compromised CI) are defended by what git already
provides. Sigstore on top would not change *probability* of
compromise; it would change *provability of attribution*, which is
deferred per trigger T2.

**Why traits with only one MVP adapter.** ADR-017 made ports/
adapters the architectural shape. Defining the traits now — costing
a few hours — forces every caller to depend on the abstraction.
When the Sigstore adapter lands, callers do not change.

**Why `#[non_exhaustive]`.** Adding a variant is not a breaking
change at the Rust ABI level. Storage adapters that round-trip the
enum (ADR-018) need no recompilation when a new variant is added.

**Why declarative branch protection.** GitHub's UI has no version
history and no review discipline — settings can be silently
weakened by any admin. A checked-in file makes weakening the rules
a PR that ADR-016's diff classifier sees and that an auditor can
read in `git log`. `branch-protection-drift` CI ensures live
settings match the file.

**Why `enforce_admins: true` is non-negotiable.** ADR-023's threat
model includes "insider with repo write access". Repo admins are
the strongest insiders by definition; an MVP that lets admins
bypass branch protection is one social-engineered admin away from
indefensible.

**Why two-host reproducible builds.** A single-host "reproducible"
build proves only that the build is deterministic on that host.
Two independent hosts prove determinism *across environments*,
which is the property an auditor or downstream verifier needs.

**Why `signing_git_commit` does no cryptography.** Git already does
it correctly with mature, audited implementations. Re-implementing
in Rust adds a cryptographic surface the project does not need to
maintain. The adapter's job is to *record the binding* between
audit entry and commit so verification later can re-establish it.

## Consequences

This ADR commits the project to:

- **`crates/signing/` lands with the MVP adapter** before any
  storage adapter writes to the audit log. Storage adapters call
  `SigningProvider` to populate `AuditEntry.signatures`.
- **All contributors register a GPG/SSH signing key with their
  GitHub account.** CI rejects unsigned commits via
  `verify-signed-commits`; PRs with unsigned commits cannot merge.
- **Every release is a signed annotated tag.** The release manager
  role is named (single individual at MVP). Tag-signing keys are
  rotated per release-manager handover.
- **Two-host reproducible-build matrix on every release tag.**
  Adds ~5–10 min of CI per release. Mismatch escalates as a
  release-blocking issue.
- **Branch protection captured in `.github/branch-protection.yaml`**
  in both repos. Changes land via PR with required review.
  `branch-protection-drift` runs nightly and on every push to `main`.
- **CI bot tokens are scoped to non-merge operations only.** The
  bot may comment, label, run checks; it cannot merge to `main` or
  push tags. Closes the residual risk noted in ADR-023.
- **ADR-027 conformance test covers a Sigstore-shaped fixture**
  even though MVP code paths do not produce one, so storage
  adapters round-trip the future variant before activation.
- **`Operator.pubkey: Option<PubKey>`** is plumbed through but
  stays `None` at MVP — key identity lives on the commit object.
  Sigstore activation populates this per signing event.

This ADR does **not** commit the project to:

- Standing up Sigstore infrastructure (deferred per T2).
- Per-row signatures separate from the commit (deferred per T4).
- Hardware-backed keys (deferred per T1).
- Cosign-signed binaries beyond signed git tags (deferred per T2,
  ADR-025).
- WebAuthn / passkey-bound action signing (deferred per T2 or T4).
- Hash-chained audit log beyond git's commit DAG (deferred per T5;
  `chain_hash` column reserved but unused).

## Re-open triggers

This ADR is reviewed when any of ADR-023's T1–T6 fires. Mapping
from trigger to deferred control activated under this ADR:

- **T1** (compromised operator device in scope) — activates
  `signing_yubikey` / `signing_secure_enclave` adapters; branch
  protection requires hardware-backed key attestation; supersedes
  the "GPG/SSH key" guidance.
- **T2** (external auditor / regulator request) — activates
  `signing_sigstore_keyless`, Rekor anchoring, self-hosted
  Sigstore. Branch protection unchanged; the trait layer absorbs it.
- **T3** (operator-key friction blocks a non-developer workflow) —
  activates `signing_sigstore_keyless` via OIDC for non-developer
  operators; developers may stay on git-commit signing.
- **T4** (per-row attribution required) — `AuditEntry.signatures`
  length > 1; supersedes "one `GitCommit` per entry".
- **T5** (long-term verifiability gap contested) — Rekor anchoring
  of historical entries + hash-chained audit-log algorithm;
  `chain_hash` gains a populated value.
- **T6** (consequence tier becomes safety-critical) — full
  Sigstore-everywhere, formal third-party crypto audit, HSM
  evaluation; ADR-024 superseded wholesale by a v2 successor.

When a trigger fires, the activating event is recorded in `LOG.md`
per ADR-023. The successor ADR back-references via
`Supersedes: ADR-024`.

## Open questions / supersession triggers

- Whether `required_linear_history: true` survives review. Default
  is linear (keeps the audit log readable, pairs with ADR-016);
  reviewers may prefer merge commits for thread preservation.
- Whether `.github/branch-protection.yaml` is hand-rolled or uses
  an existing action (`gh-branch-protection-rules`, Terraform's
  `github_branch_protection`). Deferred to land time; the YAML
  contract holds regardless.
- Whether the data repo (ADR-019) needs a stricter MVP than the
  code repo. Same threat model applies; differential controls add
  complexity. Resolved with storage / proposal-sink ADRs.
- Whether `signer_key_id` holds the GPG long-key-id, SSH-key
  fingerprint, or both (tagged enum). Deferred to the
  `signing_git_commit` implementation PR.
- Whether reproducible-build host 2 runs on `ubuntu-24.04-arm`
  (cross-arch confidence) or a self-hosted x86_64 runner (closer
  to the production build environment). Deferred to the CI PR.

## References

- [`METHODOLOGY.md`](METHODOLOGY.md)
- [ADR-016 — PR-diff-based policy enforcement](ADR-016-pr-diff-policy-enforcement.md)
- [ADR-017 — Rust core, ports/adapters, multi-target deploy](ADR-017-rust-core-ports-adapters.md)
- [ADR-018 — Storage as a port](ADR-018-storage-port.md)
- [ADR-019 — Proposal sink as a port](ADR-019-proposal-sink-port.md)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-025 — Distribution integrity](ADR-025-distribution-integrity.md)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
- Git signed commits — <https://git-scm.com/book/en/v2/Git-Tools-Signing-Your-Work>
- GitHub commit signature verification —
  <https://docs.github.com/en/authentication/managing-commit-signature-verification>
- GitHub branch protection API —
  <https://docs.github.com/en/rest/branches/branch-protection>
- Reproducible Builds project — <https://reproducible-builds.org/>
- Rust reproducible-build guide — <https://github.com/rust-lang/rust/blob/master/src/doc/rustc/src/reproducible-builds.md>
- `SOURCE_DATE_EPOCH` specification —
  <https://reproducible-builds.org/specs/source-date-epoch/>
- Sigstore — <https://www.sigstore.dev/> (keyless: <https://docs.sigstore.dev/cosign/signing/overview/>; Rekor: <https://docs.sigstore.dev/rekor/overview/>)
- Hexagonal / ports & adapters — Alistair Cockburn, 2005
- ISO 13485:2016 §7.3 (Design and development controls)
- IEC 62304:2006/AMD1:2015 (Medical device software lifecycle)
