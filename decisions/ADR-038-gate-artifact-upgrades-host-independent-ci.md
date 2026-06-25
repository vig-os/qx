# ADR-038 — Gate artifact lifecycle: vendoring, federated upgrades, host-independent CI

- Status: Proposed
- Date: 2026-06-11
- Component / area: how the gate binary reaches, lives in, upgrades
  inside, and outlives a deployed registry — and the CI architecture
  (pipeline-as-derivation) that keeps verification host-independent.
  Refines ADR-034 §2 (pinned artifact gains a *location* story),
  ADR-033 (anatomy + `[min,max]` → floors), ADR-025 (release artifacts
  gain image + recipe + closure).
- Reviewers: Lars Gerchow (required for Accepted)
- Related: ADR-017 (toolchain pinning), ADR-024/025 (repro + signed
  releases), ADR-030 (multicall `pr`, build order), ADR-033 (anatomy,
  compat range), ADR-034 (SSoT gate), ADR-037 (gate provenance in the
  stream — sibling)

## Context

Regulated retention horizons (10–15+ years) outlive release URLs,
hosts, and ABIs. The current gate template fetches the pinned `pr`
binary from the tool repo's GitHub Release at run time — correct pin
semantics (sha256 verified before exec, per ADR-034 §2), wrong
availability story: the data repo's verifiability depends on an
external URL surviving. Measured baseline (2026-06-11, fat-LTO/strip
release profile): per-binary 2.3–4.1 MB raw, **0.8–1.6 MB zstd** — so
carrying the gate in-repo costs megabytes per upgrade, not gigabytes
per decade. Separately: upgrades need a defined succession mechanism
(who validates the validator?), aging tools need a degradation story
better than a read-only cliff, and CI logic living in workflow YAML is
host-locked — "GH gone" must not break the *audit*.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Fetch-by-pin from release URL (status quo) | small repo; pin semantics already right | verifiability depends on external URL across 15 y; network on the evidence path | Kept as the `fetched` knob value for casual registries |
| Git LFS for the vendored blob | small checkout | pointers in repo, blobs on a server — silently breaks the self-containment being bought | Rejected |
| User keeps a separate fork/mirror of the tool repo as the artifact source | source custody; no blobs in the data repo | a second custody domain whose pairing with the data is maintained by *procedure*, not structure: the anchor ledger doesn't seal it, upgrades stop being one atomic PR, the evidence package becomes two repos that must be bundled/restored together, per-registry gate versions force N forks, and the evidence path regains a network/auth dependency | Rejected as the *runtime* carrier; **recommended additionally** for source custody (complementary, not alternative) |
| Nix flakeref as the gate reference | perfect identity + full build-env pinning | a pointer is not availability: resolving needs hosts/caches with no SLA; "install Nix to verify" raises the auditor bar | Rejected as carrier; **adopted as build/recipe layer** |
| **Vendored static gate + recipe + attestation in the data repo; Nix builds it; pipeline-as-derivation; image to ghcr** (chosen) | repo-alone verifiability; zero-network blocking gate; recipe diff reviewable; rebuild-in-2040 path | MBs per upgrade in history (measured: acceptable) | **Chosen** |

## Decision

### 1. The gate is vendored in the data repo (availability ≠ trust)

`.qx/gate/` carries: the **static (musl) `pr` binary**
(zstd), its `sha256`, the **attestation bundle**, the **source
tarball**, and the **Nix recipe** (flakeref + `flake.lock` snapshot +
narHash). Plain blobs — never LFS. Trust is unchanged from ADR-034 §2:
CI verifies the blob against the **CODEOWNERS-gated pin before
executing**; a PR swapping the blob without the pin fails, a pin change
is a `descriptor_change`-class edit. The blob is a cache; trust flows
pin → signed release → tool-repo attestation. Manifest knob:
`gate_artifact = vendored (regulated default) | fetched`.

**History keeps every version — by construction.** The working tree
holds only the current gate; upgrades replace it; git history retains
predecessors (rotation would mean rewriting history — forbidden by the
ADR-037 model). Historical re-validation is
`git show <upgrade-commit>:.qx/gate/…` from the repo alone.
Identity/authenticity/availability split: hash in the stream (ADR-037
gate event) = identity; attestation = authenticity; blob in history =
availability. For the 2040 horizon the **exported build closure** (full
toolchain) goes in the cold-storage bundle, not the working tree.

### 2. Upgrades: the incumbent validates its own succession

A bump PR (release-watcher bot or `pr upgrade`) carries: new blob + pin
+ attestation + source + recipe diff (`flake.lock` — the reviewable
"what changed in the build environment"). Rules:

- CI validates the transition with the **incumbent** gate, pin read
  from `main` (never from the PR branch).
- Incumbent checks: artifact verifies against a **signed tool-repo
  release**; **version monotonicity** (downgrades require elevation);
  declared engine-range covers the repo's contract.
- The **successor runs in shadow** on the same PR — advisory report of
  what it would say, catching "v(n+1) rejects this registry" before
  cutover.
- CODEOWNERS elevation; post-merge the successor governs; the boundary
  is visible in the stream (ADR-037 gate events) — revalidation scope
  is a query.

### 3. Compatibility is federated: per-op floors, not a read-only cliff

Two levels replace a single `[min,max]` cliff (refines ADR-033):

- **Metamodel parse floor (hard):** a tool that cannot parse the
  contract format refuses entirely (existing refuse/migrate rule).
- **Per-op floors (derived):** an op's floor follows **what it
  consumes** — id scheme, collection descriptor, lifecycle edges,
  render preset. Re-print of an existing part (stable audit-append +
  existing preset) keeps working from an old tool; a new mint follows
  the current descriptor, so its floor rises with it. An aging tool
  **sheds write capabilities op-by-op**; "read-only" is the limit
  point, not a mode. Honesty rule: the *enforced* boundary is
  content-validated-against-current-contract at the gate; floors are
  client preflight UX ("mint requires ≥ 2.3") plus an optional
  `min_producer_version` posture knob — producer version is a claim
  (ADR-037), never the security argument. Reads are not gateable and
  are governed by the client-of-`origin/main` rule.

### 4. CI is pipeline-as-derivation; hosts get a shim ("Dagger-like", in Nix)

- **Tool repo flake outputs:** `packages.gate` (static musl `pr`),
  `packages.runner-image` (`dockerTools.buildLayeredImage` —
  reproducible, attested), `checks.*` (hermetic validation suites —
  Nix-sandboxed, network-free by construction), `apps.ci`
  (orchestrator). One derivation tree → binary, image, closure, recipe
  cannot drift apart.
- **Release pipeline publishes the runner image to ghcr**
  (`ghcr.io/<org>/qx-runner:<tag>` + digest), alongside the
  existing binary + sha256 + (future) attestations — ADR-025's artifact
  set grows by the image and the recipe.
- **Data-repo workflows are logic-free shims**: fetch-or-use-vendored
  gate → run. The deeper suite may run inside the pinned runner image
  (same digest locally via podman and in CI — environment drift dies).
  The **blocking merge-gate stays the vendored static binary** (zero
  network, zero infra at the decision point); the rich pipeline rides
  Nix/images. Evidence path and convenience path never share
  dependencies.
- **"GH gone" matrix:** verification — host-independent forever
  (pipeline runs from the bundle); merge-block — portable to the next
  T1 host (ADR-034 tiers), since no logic lives in workflow YAML;
  past witness — durable via ADR-037 merge-sync. Nothing about the
  host's existence is load-bearing for the *evidence*.

### 5. Bootstrap: one curl-able entry, anchor-aware

`install.sh` at a stable URL (tool-repo release asset + raw fallback):
`curl -fsSL …/install.sh | bash -s -- <owner/repo> [flags]` fetches the
**release-pinned** bootstrap + templates and runs
`bootstrap-data-repo.sh`, which (extended) seeds: collections +
contract + manifest + personas genesis (ADR-036), CODEOWNERS, the gate
workflow with pin, **`anchor.yml` + `bundle.yml`** (ADR-037 ledger),
protection + **immutable-releases setting**, and prints the
qualification checklist (the adopter's IQ step — validation package
entry point). The curl entry verifies the bootstrap's sha256 against
the release before executing it (same pin discipline as the gate).

## Rationale

**Why in-repo beats a user-side fork of the tool repo:** vendoring
replaces procedure with structure. Because the gate blob lives in the
same Merkle tree as the data, the ADR-037 anchor ledger **seals the
gate binary itself** with every anchor — a separate fork would need its
own anchor/witness apparatus to reach the same assurance. Upgrades are
one atomic, CODEOWNERS-gated, revertable PR instead of a two-repo
coordinated change with no transaction. `git bundle` of one repo is the
complete evidence package by construction — it is not merely "harder to
miss pulling the second repo" during backup, it is *impossible* to back
up the data without its gate. And per-registry pinning stays honest:
two registries on different validated versions are two blobs, not N
forks to administer. The fork retains one legitimate role — *source*
custody (patch/audit/hedge-upstream) — which ADR-038 serves via the
vendored source tarball + closure-in-bundle instead.

The principle the whole design reduces to: **pointers for identity,
signatures for authenticity, copies for availability.** Nix is the best
pointer-and-recipe machine available — so it builds and describes the
artifact (and makes upgrade diffs reviewable), but the artifact the
evidence path *executes* is a vendored static copy with no resolution
step. Incumbent-validates-succession is the same pattern as sudo
editing sudoers, and shadow-running the successor converts upgrade risk
into a report. Per-op floors fall out of content-vs-contract validation
that already exists — federated degradation costs no new mechanism,
only honesty about where enforcement actually lives. Measured sizes
(0.8–1.6 MB zst/binary) close the only real objection to vendoring.

## Consequences

- Tool repo: flake gains `packages.gate` (musl) / `packages.
  runner-image` / `checks.*`; release.yml gains the ghcr push job and
  the recipe/closure artifacts; `install.sh` becomes a release asset.
- Data repos: anatomy gains `.qx/gate/` (ADR-033 note);
  workflows gain `anchor.yml`/`bundle.yml`; check workflow prefers the
  vendored gate once seeded (`fetched` remains for casual registries).
- `pr upgrade` joins the CLI surface; a release-watcher bot is optional
  sugar over the same PR shape.
- Repo growth ~3–6 MB compressed per upgrade — documented; daily clones
  may be blobless (`--filter=blob:none`); the bundle stays full.
- Adopter docs gain the qualification checklist (validation package).

## Open questions / supersession triggers

- Musl static feasibility for every gate dependency (git interaction is
  via CLI/API, not libgit2 — expected fine); fallback: small-closure
  glibc build, closure exported to the bundle.
- Whether `pr upgrade` also migrates contracts (ADR-033's deferred
  migration ADR) or only swaps artifacts — leaning: artifacts only,
  migration is its own reviewed op.
- Runner-image registry redundancy (ghcr + a mirror) — decide when a
  non-GitHub T1 host lands.
- Attestation availability on private ghcr/repos is plan-dependent —
  verified per deployment (ADR-037 §5 caveat applies here too).

## References

- ADR-024/025 — repro builds, signed releases (the trust the pin
  resolves to)
- ADR-033 — anatomy + compat range (refined to floors)
- ADR-034 — SSoT gate from pinned artifact (gains the location story)
- ADR-037 — provenance events; anchor ledger (sibling)
- `tools/bootstrap-data-repo.sh`, `tools/data-repo-templates/` —
  current seeder this extends
- nixpkgs `dockerTools`; `git bundle`; measured sizes: this session,
  `target/release/{mint,label,bind}` + zstd-19
