# ADR-025 — Distribution integrity (signed releases, SRI, reproducible builds, future Cosign)

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — release packaging and end-user
  delivery for the Rust CLI binaries, the WASM module, and any
  future Tauri / mobile / embedded artifact
- Reviewers: Lars Gerchow
- Related: ADR-017 (Rust core + reproducible build mandate), ADR-023
  (threat model + MVP crypto scope), ADR-024 (signing port + signed
  commits + signed tags), ADR-027 (drift-detection enforces
  reproducibility invariants)

## Context

ADR-024 covers the **per-action / per-commit** crypto baseline:
signed git commits, branch protection, signed git tags, and
reproducible Rust builds. It does not cover the **distribution
side** — how the artifacts that operators actually run get from a
signed source commit to a verified binary on the operator's
machine, browser, or future device.

Distribution integrity is a distinct concern with a distinct threat
model surface:

- **Operator pulls a binary from a release page** — does the binary
  match what the source commit produced?
- **Browser loads the FE WASM module** — has the served bundle been
  tampered with between the source commit and the browser?
- **Self-update path (future)** — does the running binary verify
  the new artifact before swapping?

ADR-023's adversaries-in-scope (external attacker, insider with
repo write, compromised CI) all touch this surface differently:

- External attacker tampering with the FE bundle in transit →
  Subresource Integrity (SRI) defends
- Insider modifying a release artifact post-build → signed tags +
  reproducible-build verification defends
- Compromised CI runner producing a tampered binary → multi-host
  reproducible-build matrix defends (one CI runner cannot forge
  a hash that two independent hosts both accept)

The MVP scope here mirrors ADR-023's posture: ship the controls
git + GitHub already provide, design the data shape so the deferred
controls (Cosign-signed artifacts, transparency-log inclusion for
distribution) bolt on later without redesign.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **No distribution-side controls** — operators trust the GH release page | zero work | indefensible against tampered-release scenario; no defense for FE WASM in transit; not auditor-defensible at the regulatory tier | Rejected |
| **Signed git tags only**, no per-artifact signature, no SRI | trivial; relies on git's content-addressing for source integrity | source integrity ≠ artifact integrity; a tampered build of a signed commit still verifies as "signed" by tag association | Rejected — necessary but not sufficient |
| **MVP: signed git tags + reproducible build verification (multi-host CI) + SRI on FE WASM bundle + signed release artifacts via signed tag annotation**, with Cosign / Sigstore artifact signing reserved as a future bolt-on | covers the named threats with no new infrastructure beyond what ADR-024 + Vite already provide; ~1 day of CI work | long-term verifiability of binary releases tied to GitHub remaining authoritative (same gap ADR-024 accepts for commits) | **Chosen** |
| **Sigstore-everywhere from day one** (Cosign-signed artifacts, Rekor-anchored release proofs, SLSA provenance) | maximum rigor; matches the OSS ecosystem pattern | requires Sigstore infrastructure (per ADR-023 deferred); ~2 weeks setup + ongoing ops; over-builds for current consequence tier | Rejected for MVP — preserved as the deferred upgrade path |

## Decision

The MVP distribution-integrity stack is:

1. **Reproducible Rust builds** (already in ADR-024 §4) verified by
   a **two-host CI matrix** on every release tag. CI builds the
   tagged commit on `ubuntu-latest` and `macos-latest` with the
   pinned toolchain; if the artifact hashes diverge, the release is
   blocked. Hash records (SHA-256 of every artifact) are written to
   the release notes and to a per-tag `dist-hashes.json` checked
   into the data repo's `releases/` directory.

2. **Signed git tags** (ADR-024 §3) carry the artifact hashes in the
   tag annotation body. Verifying the tag signature transitively
   verifies the artifact hash binding. This is the MVP's substitute
   for per-artifact Cosign signatures.

3. **WASM bundle Subresource Integrity (SRI)**. The FE's `index.html`
   loads `crates/wasm/`'s built artifact via a `<script>` /
   `<link>` tag with `integrity="sha384-..." crossorigin="anonymous"`.
   Vite's `vite-plugin-wasm` + a small post-build step compute the
   SRI hash from the same artifact CI verified, and inject it into
   the HTML at build time. The browser refuses to execute a WASM
   bundle whose hash does not match.

4. **CLI self-update is out of scope for the MVP.** Operators install
   via `cargo install qx-cli` (against crates.io once
   published, or against the git tag) or download a pre-built
   release binary and verify against the `dist-hashes.json` and the
   signed tag manually. A future ADR (or amendment) introduces
   automatic self-update once the deferred Cosign artifact-signing
   activates.

5. **No third-party CDN for FE assets.** The WASM bundle is served
   from the same origin as the rest of the FE (GitHub Pages today;
   self-hosted in future). Eliminates an entire class of supply-chain
   attacks at the cost of losing free CDN caching — acceptable for
   the project's scale.

## Forward-compatibility

When ADR-023 trigger T2 fires (auditor request) or T6 (consequence
tier escalates), the following bolt on:

- **Cosign-signed artifacts**: each release artifact (CLI binary,
  WASM bundle, Tauri app, mobile package) gets a Cosign signature
  alongside its hash. The `dist-hashes.json` schema reserves a
  `cosign_signature: Option<String>` field today (empty in MVP);
  future activation populates it.
- **Sigstore Rekor inclusion proof** for each artifact signature,
  same shape as the per-row signing path in ADR-024's deferred
  state. The `dist-hashes.json` reserves `rekor_proof: Option<...>`.
- **SLSA provenance attestation** (level 3+) generated by CI and
  attached to the release. Reserved field `slsa_provenance:
  Option<String>` in `dist-hashes.json`.
- **Self-update path** in the CLI: download artifact → verify
  Cosign signature against trust root → verify hash matches
  `dist-hashes.json` → swap binary atomically. The swap mechanism
  (`cargo-update`-style or a custom updater) is an implementation
  detail of the future adapter.

## Rationale

**Why a two-host reproducible-build matrix is the load-bearing
defense against compromised CI.** A single CI runner can be
subverted to produce any artifact and call it "the build of commit
X." Two independent runners producing byte-identical artifacts for
the same commit makes that subversion require *both* runners to be
compromised simultaneously and identically — a much harder attack
than compromising one. Three-host matrices are stronger but the
incremental gain past two is small for the regulatory tier this
project targets; revisit on T6.

**Why hashes in the signed tag annotation, not detached signature
files.** Tag annotations are part of the git object DAG; tampering
with the annotation breaks the tag signature. A detached signature
file in `releases/` would be one extra file to remember to verify;
embedding hashes in the tag annotation makes "verify the signature
of the tag" sufficient.

**Why SRI on the WASM bundle but not on the rest of the FE.** The
WASM bundle is the load-bearing security boundary — it carries the
codec, validators, signing trait dispatch, identity provider client.
The rest of the FE (HTML/CSS/light TS shell) is much less
sensitive; SRI on every asset adds bundle-rebuild churn for
marginal protection. WASM gets the rigor; the shell gets standard
TLS.

**Why no CLI self-update in MVP.** Self-update introduces a runtime
trust decision (the running binary must decide whether to trust
the new artifact). Without Cosign-signed artifacts (deferred), the
self-update trust decision would need to fall back to "trust
GitHub's release page," which is the same trust anchor as a manual
download. Manual download keeps the trust decision with the
operator, where it belongs in the MVP. Automation lands with
Cosign activation.

## Consequences

- **CI release workflow grows**: the existing `.github/workflows/`
  set adds a `release.yml` that runs on tag push. Two-host matrix
  build, hash comparison, tag-annotation injection of hashes,
  `dist-hashes.json` PR to the data repo. Estimate ~1 day to
  build, including the hash-injection automation.
- **FE build pipeline change**: `vite-plugin-wasm` configured to
  emit an SRI hash in the build output; a small post-build
  script (~30 lines) writes the hash into the served HTML. The
  WASM bundle URL becomes immutable per release (content-addressed
  via the hash in the SRI attribute), so cache headers can be
  permanent.
- **Release-tag discipline**: tags are exclusively `v<semver>`,
  signed (per ADR-024), and annotated with the artifact hash
  block. CI rejects unsigned or differently-named tags.
- **Operator install instructions**: `crates/README.md` (and any
  future installation docs) must include both the install command
  AND the verification command. Documented as a release-checklist
  item.
- **`dist-hashes.json` schema is forward-compatible**: today's
  schema accepts the future `cosign_signature`, `rekor_proof`,
  `slsa_provenance` fields as optional. Storage adapters and the
  release workflow round-trip them blindly so MVP-era release
  records remain valid after the schema activates.
- **No third-party CDN**: FE assets served from GH Pages (or
  self-hosted). The "could we put WASM on a CDN for caching" lever
  is intentionally not pulled.
- **No self-update in CLI**: the CLI binary will refuse to
  self-update if such a request is made; the only way to upgrade
  is a fresh install + manual hash verification. Documented in
  the user manual.

## Open questions / supersession triggers

- Whether `dist-hashes.json` belongs in the data repo or the code
  repo. Argument for data repo: it's the audit artefact; auditors
  consult the data repo for everything else. Argument for code
  repo: it's tied to source releases. Decision deferred to release
  workflow implementation; methodology accepts either.
- Whether the two CI hosts should be `ubuntu-latest` +
  `macos-latest` (different OS, similar toolchain) or two
  geographically-distinct GH Actions runners on the same OS
  (different physical infrastructure, identical toolchain).
  Cross-OS reveals platform-dependent build leaks; same-OS
  reveals supply-chain leaks. Both are useful; doing both is a
  three-host matrix.
- Whether to publish to `crates.io` for `cargo install` access.
  Pros: one-line install; integrates with the Rust ecosystem's
  trust chain. Cons: crates.io has no signature mechanism today
  (Sigstore integration is RFC'd but not shipped); we'd be relying
  on crates.io's TLS + GitHub linkage. Defer to ADR-023 trigger
  T2 alongside Cosign.
- Whether SLSA Level 3 is achievable with GH Actions today.
  Currently yes via `slsa-framework/slsa-github-generator`.
  Decision to claim SLSA-3 deferred until ADR-023 trigger T2 (auditor
  request). MVP records what's necessary for SLSA-3 retroactively
  (hashes, build env, provenance) but doesn't claim the level.
- Whether tampering with `dist-hashes.json` in the data repo is a
  threat scenario. ADR-023 lists "insider with repo write" in
  scope; this is exactly that. Defense: `dist-hashes.json` is
  only ever written by CI on tag push; the data-repo branch
  protection rejects human-authored modifications to that file
  via a CODEOWNERS-style rule. To be implemented alongside the
  release workflow.

## References

- [ADR-017 — Rust core + ports/adapters](ADR-017-rust-core-ports-adapters.md)
  §"Reproducible-build discipline"
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
  §"MVP crypto scope — fixed", trigger T2 (auditor request) and T6
  (consequence escalation)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
  §3 (signed tags), §4 (reproducible builds)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
  §"Tier 4 — drift-detection" (reproducibility invariants enforced
  as lint-tests)
- Subresource Integrity — <https://www.w3.org/TR/SRI/>
- Sigstore Cosign — <https://docs.sigstore.dev/cosign/overview/>
- SLSA — <https://slsa.dev/spec/v1.0/>
- Reproducible Builds — <https://reproducible-builds.org/>
