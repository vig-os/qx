# ADR-021 — Configuration model (12-factor)

- Status: Accepted
- Date: 2026-05-10
- Component / area: cross-cutting — defines the single read site for
  every deploy-varying value in the system; selects every adapter named
  by ADR-018 / ADR-019 / ADR-020 / ADR-024
- Reviewers: Lars Gerchow
- Related: ADR-013 (Parts registry web app — fixes the data substrate
  whose path is now configurable), ADR-017 (Rust core + ports/adapters
  — names `crates/config/`), ADR-018 (Storage port — adapter selected
  here), ADR-019 (Proposal sink port — adapter selected here),
  ADR-020 (Identity & authorization port — adapter selected here),
  ADR-022 (Observability — log level / sink configured here),
  ADR-023 (Threat model — `verified_at` window, signing toggles
  configured here), ADR-024 (Crypto baseline — signing adapter
  selected here), ADR-027 (Port conformance — drift test forbids
  hardcoded paths in domain/codec/validators)

## Context

The Python tooling and the TypeScript FE both hardcode every
deploy-varying value they touch. Concretely:

- `label.py:33-36` hardcodes `PARTS_DIR`, `REGISTRY`, `PRINT_LOG`,
  `LABELS_DIR` as paths derived from the script's own location. A
  second deploy on the same machine (e.g. a staging clone of the data
  repo) requires editing the script.
- `label.py:64` hardcodes `DEFAULT_SIZE_MM = 11.0` as a module
  constant. An operator who prefers a different default tape per site
  cannot change it without editing source.
- `label.py:81` hardcodes `FONT_FAMILY = "Consolas, monospace"`. A
  Linux lab without Consolas installed gets fallback rendering with
  no operator-visible knob.
- `web/src/config.ts:8` hardcodes `REPO_SLUG = "MorePET/part-registry"`
  and derives the registry URL from it. The repo split required by
  ADR-019 (code repo OSS, data repo closed) cannot land without
  changing this file in source and rebuilding the FE bundle.
- Operator identity comes from `--operator $USER` (Python) or nothing
  (FE). ADR-020 replaces this with an `IdentityProvider` adapter, but
  *which* adapter to load on a given deploy is itself a configuration
  question this ADR must answer.

ADR-017 commits the project to a Rust workspace with a
ports-and-adapters shape. ADR-018, ADR-019, ADR-020, and ADR-024 each
define a port with multiple credible adapter implementations. The
question this ADR answers is: **how does a running binary know which
adapter to instantiate, and where do the deploy-specific values
(repo URLs, file paths, IdP endpoints, signing keys) live?**

Three constraints bear on the answer:

1. **One binary, many deploys.** The Rust `mint`/`label`/`bind`
   binaries (and the WASM bundle) must be byte-identical across
   developer-laptop, lab-machine, CI, and future Tauri/embedded
   deploys. Reproducible-build discipline (ADR-024 §4) requires this.
   Compile-time configuration is therefore unavailable as a primary
   mechanism: every deploy-varying value must be runtime-readable.
2. **Audit-grade single read site.** ADR-027 includes a drift-detection
   conformance test that forbids hardcoded paths and URLs in
   `domain/`, `codec/`, `validators/`, `storage/`, `identity/`,
   `transport/`, and `signing/` source trees. Configuration must have
   exactly one read site (`crates/config/`) and hand typed values to
   the rest of the system. Scattered `std::env::var(...)` calls fail
   the drift test.
3. **Heterogeneous deploy targets.** A configuration model that works
   only for the CLI fails the WASM/FE deploy. A model that works only
   for the FE fails the embedded/handheld future deploy named in
   ADR-017's portability matrix. The model must accommodate at least
   four target shapes: native CLI, browser WASM, future
   Tauri/desktop, future embedded.

The 12-factor methodology answers exactly these constraints.
Principle III ("Store config in the environment") prescribes
environment variables as the substrate; principle IV ("Treat backing
services as attached resources") prescribes that databases, message
queues, identity providers, and storage be addressable via
configuration without code change. Both principles map directly onto
the ports/adapters shape ADR-017 chose.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **Status quo: hardcoded paths in source + scattered `os.getenv()` / `import.meta.env` calls** | Works today; zero up-front cost | Violates 12-factor III; no per-deploy configuration without source edit; no parse-at-boundary discipline (each call site re-parses); fails ADR-027's drift test | Rejected — every adapter named in ADR-018/019/020/024 needs a selection mechanism, and hardcoding forecloses every alternative the port architecture is designed to enable |
| **`clap`-based CLI args only** | Mature; well-understood; type-safe | Solves only the CLI surface; FE/WASM has no `argv`; embedded targets often have no `argv`; CLI args do not unify across processes | Rejected as the *only* mechanism — `clap` remains useful for per-invocation overrides on top of env (e.g. `--size 11` overriding the configured default), but it cannot be the substrate |
| **Compile-time configuration (cargo features per deploy)** | Smallest runtime; no parser | Violates "one binary, many deploys"; rebuild required for every config change; defeats reproducible-build discipline (each deploy produces a different binary hash); no path to operator-changeable values | Rejected — the cost falls on every deploy event, forever |
| **`figment`-based 12-factor: defaults file shipped in binary + env overrides + per-deploy override file, all parsed once at startup into a typed `Config` struct** | Matches 12-factor III + IV exactly; one read site enforces ADR-027 drift property; typed values flow to the rest of the system; supports CLI, WASM, and future targets via the same schema; layered precedence (defaults < deploy file < env < CLI flags) is explicit | Up-front work to define the schema; one new dependency (`figment` ≈ 30 KB compiled) | **Chosen** |
| **`config` crate (rust-cli/config-rs)** | Same shape as figment; mature | Heavier dependency footprint (~100 KB compiled); less Rust-idiomatic API; figment's serde-first model maps more cleanly to typed `Config` | Rejected in favour of figment — both would work; figment is the lighter pick for the same property |
| **Vault / consul / etcd / aws-parameter-store** | Centralised secret management; rotation; audit | Massively over-scoped for a sub-million-row CSV registry; introduces a network dependency for CLI tools that today work on an offline laptop; each deploy target (CLI, WASM, embedded) needs its own client | Rejected — wrong scale; revisit only if a multi-tenant SaaS deploy emerges (re-open trigger documented below) |

## Decision

Configuration follows **12-factor principle III** (config in
environment) and **principle IV** (backing services as attached
resources). Concretely:

1. A single Rust crate `crates/config/` reads and validates all
   configuration at process startup (parse-at-boundary), and hands a
   typed `Config` value to the rest of the system.
2. Every adapter selection (storage, proposal sink, identity,
   signing) is named via env vars and resolved through `Config`.
3. Defaults ship in `crates/config/defaults.toml`, embedded into the
   binary via `include_str!`. Env vars override defaults. An optional
   per-deploy file (`~/.config/part-registry/config.toml` for CLI,
   build-time `import.meta.env.PART_REGISTRY_*` for FE) overrides
   defaults but is itself overridden by env vars.
4. No file under `crates/{domain,codec,validators,storage,identity,transport,signing}/`
   may contain a hardcoded path, repo URL, IdP endpoint, font family,
   or service URL. ADR-027's drift-detection conformance test
   enforces this with a static `grep`-style check over the workspace.
5. The substrate is `figment`. Layered precedence: built-in defaults
   < deploy file < environment variables < per-invocation CLI flags
   (where applicable).

## Config crate shape

```rust
// crates/config/src/lib.rs
use std::path::PathBuf;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub repo: RepoConfig,
    pub storage: StorageConfig,
    pub identity: IdentityConfig,
    pub transport: TransportConfig,
    pub signing: SigningConfig,
    pub label: LabelDefaults,
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Deserialize)]
pub struct RepoConfig {
    /// env: PART_REGISTRY_DATA_REPO_URL — closed data repo
    /// (e.g. git@github.com:eXoma/exopet-registry.git)
    pub data_repo_url: String,
    /// env: PART_REGISTRY_CODE_REPO_URL — OSS code repo
    /// (e.g. https://github.com/MorePET/part-registry)
    pub code_repo_url: String,
    /// env: PART_REGISTRY_DATA_LOCAL_PATH — local clone path
    pub local_clone_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum StorageAdapterChoice {
    /// env: PART_REGISTRY_STORAGE=csv_git
    CsvGit,
    /// env: PART_REGISTRY_STORAGE=sqlite,
    ///      PART_REGISTRY_STORAGE_SQLITE_PATH=/var/lib/...
    Sqlite { path: PathBuf },
    /// env: PART_REGISTRY_STORAGE=duckdb,
    ///      PART_REGISTRY_STORAGE_DUCKDB_PATH=/var/lib/...
    DuckDb { path: PathBuf },
}

#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub adapter: StorageAdapterChoice,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum IdentityAdapterChoice {
    /// env: PART_REGISTRY_IDENTITY=git_config
    GitConfig,
    /// env: PART_REGISTRY_IDENTITY=github_oauth,
    ///      PART_REGISTRY_IDENTITY_GITHUB_CLIENT_ID=...
    GithubOauth { client_id: String },
    /// env: PART_REGISTRY_IDENTITY=oidc,
    ///      PART_REGISTRY_IDENTITY_OIDC_ISSUER=...
    Oidc { issuer_url: String, client_id: String },
    /// env: PART_REGISTRY_IDENTITY=env_user — DEV/TEST ONLY
    EnvUser,
}

#[derive(Debug, Deserialize)]
pub struct IdentityConfig {
    pub adapter: IdentityAdapterChoice,
    /// env: PART_REGISTRY_IDENTITY_VERIFIED_AT_WINDOW_SECS
    /// — re-attestation window, see ADR-023
    pub verified_at_window_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum TransportAdapterChoice {
    /// env: PART_REGISTRY_TRANSPORT=github_pr,
    ///      PART_REGISTRY_TRANSPORT_GITHUB_TOKEN=...
    GithubPr,
    /// env: PART_REGISTRY_TRANSPORT=local_branch
    LocalBranch,
    /// env: PART_REGISTRY_TRANSPORT=deposit_folder,
    ///      PART_REGISTRY_TRANSPORT_DEPOSIT_PATH=...
    DepositFolder { path: PathBuf },
}

#[derive(Debug, Deserialize)]
pub struct TransportConfig {
    pub adapter: TransportAdapterChoice,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum SigningAdapterChoice {
    /// env: PART_REGISTRY_SIGNING=git_commit
    GitCommit,
    /// env: PART_REGISTRY_SIGNING=sigstore — see ADR-024 future
    Sigstore { fulcio_url: String, rekor_url: String },
    /// env: PART_REGISTRY_SIGNING=none — DEV/TEST ONLY
    None,
}

#[derive(Debug, Deserialize)]
pub struct SigningConfig {
    pub adapter: SigningAdapterChoice,
}

#[derive(Debug, Deserialize)]
pub struct LabelDefaults {
    /// env: PART_REGISTRY_LABEL_DEFAULT_SIZE_MM (default 11.0)
    pub default_size_mm: f64,
    /// env: PART_REGISTRY_LABEL_FONT_FAMILY
    /// (default "Consolas, monospace")
    pub font_family: String,
    /// env: PART_REGISTRY_LABEL_LABELS_DIR
    pub labels_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
pub struct ObservabilityConfig {
    /// env: PART_REGISTRY_LOG_LEVEL (default "info")
    pub log_level: String,
    /// env: PART_REGISTRY_AUDIT_LOG_PATH
    pub audit_log_path: PathBuf,
}

impl Config {
    /// Parse defaults + env. Used by every CLI binary at startup.
    pub fn from_env() -> Result<Self, ConfigError> {
        use figment::{Figment, providers::{Format, Toml, Env}};
        Figment::new()
            .merge(Toml::string(include_str!("../defaults.toml")))
            .merge(Toml::file(deploy_file_path()).nested())
            .merge(Env::prefixed("PART_REGISTRY_").split("_"))
            .extract()
            .map_err(ConfigError::from)
    }

    /// Test-only: parse defaults + an explicit override map.
    pub fn from_defaults_with_overrides(
        overrides: &[(&str, &str)],
    ) -> Result<Self, ConfigError> { /* ... */ }
}
```

### Per-target schema

| Target | Defaults source | Override mechanism | Notes |
|---|---|---|---|
| Native CLI (`mint`, `label`, `bind`) | `crates/config/defaults.toml` (compiled in via `include_str!`) | `PART_REGISTRY_*` env vars; optional `~/.config/part-registry/config.toml`; per-invocation `clap` flags | CLI flags override env which overrides file which overrides defaults |
| Browser WASM (FE) | `crates/config/defaults.toml` (compiled into the WASM bundle) | Build-time `import.meta.env.PART_REGISTRY_*` injected by Vite at `npm run build`; runtime `/config.json` endpoint fetched at app startup for adapter selection | Vite `define` plugin substitutes build-time vars; runtime fetch covers values that vary per FE deploy without rebuild |
| Future Tauri / desktop | `crates/config/defaults.toml` (bundled in the app) | OS env vars; user config file at the OS-conventional path (`~/Library/Application Support/part-registry/config.toml` on macOS, `%APPDATA%\part-registry\config.toml` on Windows, `~/.config/part-registry/config.toml` on Linux) | Same schema as CLI; deploy file location differs per OS |
| Future embedded (handheld scanner) | `crates/config/defaults.toml` (bundled) | Env at boot from device-provisioning; no user-writable config file by default | Schema is the same; provisioning tool writes env at first boot |

## Migration audit

Every currently-hardcoded value, its source location, and its target
env var:

| Source | Current value | Target env var | Notes |
|---|---|---|---|
| `label.py:33` | `PARTS_DIR = Path(__file__).resolve().parent` | `PART_REGISTRY_DATA_LOCAL_PATH` | Removes script-location coupling |
| `label.py:34` | `REGISTRY = PARTS_DIR / "registry.csv"` | derived from `PART_REGISTRY_DATA_LOCAL_PATH` (filename fixed by ADR-013 schema) | The filename `registry.csv` is part of the data-repo schema, not config |
| `label.py:35` | `PRINT_LOG = PARTS_DIR / "print_log.csv"` | derived from `PART_REGISTRY_DATA_LOCAL_PATH` | Same — filename fixed by ADR-015 |
| `label.py:36` | `LABELS_DIR = PARTS_DIR / "labels"` | `PART_REGISTRY_LABEL_LABELS_DIR` | Operators may want to direct rendered SVGs to a shared share |
| `label.py:52-62` | `TAPE_SIZES` dict | `PART_REGISTRY_LABEL_TAPE_SIZES` (TOML table override; defaults retained) | Defaults stay in `defaults.toml`; env override is a TOML-encoded string for site-specific tape inventories |
| `label.py:64` | `DEFAULT_SIZE_MM = 11.0` | `PART_REGISTRY_LABEL_DEFAULT_SIZE_MM` | Per-site default tape height |
| `label.py:67-70` | `QR_BORDER_STANDARD = 4`, `QR_BORDER_MICRO = 2` | **NOT configurable** — ISO 18004 mandates these values | Stays a constant in `crates/codec/`; comment cites ISO 18004 §6.3.7 |
| `label.py:74-78` | `FORMATS` dict (4/4, 4/4/4, 5/5/4) | **NOT configurable for MVP** — fixed per ADR-012 ID scheme | Could become config later if customer-specific row layouts emerge; currently constants in `crates/codec/` |
| `label.py:81` | `FONT_FAMILY = "Consolas, monospace"` | `PART_REGISTRY_LABEL_FONT_FAMILY` | Operators may have different fonts available; SVG rendering must accept any |
| `web/src/config.ts:8` | `REPO_SLUG = "MorePET/part-registry"` | `PART_REGISTRY_CODE_REPO_URL` (build-time via Vite) | This is the *code* repo; per ADR-019 the data repo is separate |
| `web/src/config.ts:9` | `DEFAULT_BRANCH = "main"` | `PART_REGISTRY_DATA_REPO_BRANCH` (build-time via Vite) | Default `main` retained in `defaults.toml` |
| `web/src/config.ts:13-14` | `REGISTRY_URL` derived from `REPO_SLUG` | `PART_REGISTRY_DATA_REPO_URL` (build-time) — derive raw-content URL in adapter, not in config | Adapter-specific URL composition lives in the storage adapter, not config |
| `web/src/config.ts:17` | `ISSUE_NEW_URL` derived from `REPO_SLUG` | derived from `PART_REGISTRY_CODE_REPO_URL` in `transport_github_pr` adapter | Same — composition in adapter |
| `web/src/config.ts:24` | `QR_BORDER_MODULES = 4` | **NOT configurable** — see above | Constant in `crates/codec/` |
| `web/src/config.ts:29-39` | `TAPE_SIZES` (duplicate of Python) | shared via WASM-exported config; same env var as CLI | Eliminates the dual-source duplication |
| `web/src/config.ts:41` | `DEFAULT_SIZE_MM = 11.0` | shared via WASM-exported config; same env var as CLI | Same |
| Python CLI: `--operator $USER` (env fallback) | `os.environ["USER"]` | `PART_REGISTRY_IDENTITY=env_user` (DEV/TEST adapter) for the same behaviour; production deploys configure `git_config`, `github_oauth`, or `oidc` per ADR-020 | Identity is now a typed adapter, not a string |

The migration lands as part of ADR-017's strangler-fig step 3
(`domain` + `config`). Each step that follows (storage, identity,
transport, signing) consumes the typed `Config` and stops reading
env directly.

## Rationale

**Why 12-factor III + IV.** Both principles are written for exactly
the constraints this project has: one source tree, multiple deploys,
backing services that vary per deploy. Principle III prescribes env
as the substrate because env is the only mechanism present in every
target shape (CLI shell, container orchestrator, systemd unit, Vite
build, embedded provisioning). Principle IV prescribes that swapping
backing services (here: storage / identity / transport / signing
adapters) be a config change, not a code change — which is exactly
what the ports/adapters shape ADR-017 chose makes possible.

**Why `figment` over `config-rs`.** Both work. Figment's serde-first
API maps more cleanly to the typed `Config` struct above; its
provider model (`Toml::string(...)`, `Env::prefixed(...)`) composes
in the layered-precedence order the 12-factor pattern requires;
its compiled size (~30 KB) is one-third of `config-rs` (~100 KB).
Neither is a load-bearing choice — if `figment` becomes
unmaintained the adapter surface in `crates/config/` is small enough
to swap (a single `from_env()` function).

**Why parse-at-boundary.** Scattered `std::env::var(...)` calls in
domain code mean every call site re-parses, re-validates, and
re-handles the missing-var case. Parse-at-boundary collapses those N
re-parses to one, gives the type checker the chance to enforce that
downstream code receives valid values, and makes the config schema
auditable as a single struct definition. ADR-027's drift-detection
conformance test needs exactly this property: it greps for `env::var`
calls in `domain/`/`codec/`/`validators/` source and rejects any
match.

**Why a defaults file embedded in the binary.** Two properties:
(1) every binary is self-contained and works with no external file
on first run — useful for `cargo install part-registry` and for
embedded targets with no filesystem; (2) the defaults are visible
in source review, not buried in a deploy step. The deploy file at
`~/.config/part-registry/config.toml` is optional; if absent, env
overrides defaults directly.

**Why `PART_REGISTRY_*` as the env prefix.** Avoids collision with
unrelated tooling (`USER`, `HOME`, `GITHUB_TOKEN` are all already
overloaded). Long enough to be unambiguous in `env | grep`. Matches
the cargo crate name and project repo slug.

**Why CLI flags override env.** Per-invocation overrides (e.g.
`label --size 8` overriding `PART_REGISTRY_LABEL_DEFAULT_SIZE_MM=11`)
are a UX requirement preserved from the current Python CLI; users
expect `--flag` to "just work" without unsetting an env var. Figment
expresses this as an additional `Serialized::defaults(cli_args)`
provider merged last.

## Consequences

- **One read site.** Every adapter selection, every deploy-varying
  value, and every operator default flows through `crates/config/`.
  No file in `crates/{domain,codec,validators,storage,identity,transport,signing}/`
  may call `std::env::var` directly. ADR-027's drift-detection
  conformance test enforces this with a CI grep.
- **Reproducible builds.** The same source commit produces a
  byte-identical binary across machines because every deploy-varying
  value is read at runtime, not compiled in. Required by ADR-024 §4.
- **Schema-as-code.** The `Config` struct above IS the configuration
  schema. There is no separate JSON Schema, no separate documentation
  page that drifts from the implementation. New env vars require
  adding a field, which fails CI until `defaults.toml` and the
  `from_env` parser cover it.
- **Operator-visible knobs.** The values currently hardcoded in
  `label.py` (default size, font family, labels output dir) become
  operator-changeable without source edit. A lab without Consolas
  installed runs `export PART_REGISTRY_LABEL_FONT_FAMILY="DejaVu
  Sans Mono, monospace"` and the next `label` invocation uses it.
- **Repo split landable.** `web/src/config.ts:8`'s hardcoded
  `MorePET/part-registry` slug becomes a build-time injected value;
  the OSS code repo and the closed data repo can be operated as
  separate repos without source edit.
- **Identity adapter is now a config selection, not a code path.**
  Switching from `--operator $USER` (`identity_env_user`) to GitHub
  OAuth (`identity_github_oauth`) or generic OIDC (`identity_oidc`)
  is a single env-var change at deploy time. ADR-020's adapter set
  is selectable without rebuild.
- **WASM/FE config story is two-layer.** Build-time vars (Vite-injected
  `import.meta.env.PART_REGISTRY_*`) cover values fixed at FE bundle
  build time (e.g. code repo URL). Runtime `/config.json` fetched at
  app startup covers values that may vary per FE deploy without
  rebuild (e.g. identity adapter selection if the same FE bundle is
  reused across staging and production). Both flow through the WASM
  module's exported `Config::from_runtime(json)` constructor.
- **Test discipline.** Every `cargo test` run sets explicit env vars
  via the `Config::from_defaults_with_overrides` helper rather than
  reading the host's environment. CI runs with `env -i
  PART_REGISTRY_*=...` to prevent host env leakage.
- **Audit trail for config changes.** Production deploys check
  `~/.config/part-registry/config.toml` (or the per-OS equivalent)
  into version control under the deploy-team's repo, not the data
  repo. ADR-022's audit trail records the resolved config snapshot
  at process startup so an auditor can reconstruct which adapter
  produced a given audit entry.
- **One new dependency.** `figment` (≈30 KB compiled) joins the
  workspace dependency set. Reviewed for licensing (MIT/Apache-2.0
  dual) and supply-chain (active maintenance, used by Rocket).
- **Deletion of duplicates.** `web/src/config.ts`'s `TAPE_SIZES` /
  `DEFAULT_SIZE_MM` duplicates of the Python constants are removed
  in ADR-017 strangler-fig step 8 when the WASM module exports the
  same values to the FE.

## Open questions / supersession triggers

- Whether the FE's runtime `/config.json` endpoint should be served
  by the same web server hosting the FE (current GH Pages model has
  no server-side beyond static file hosting — solution: ship
  `config.json` as a static file in the deploy bundle, edited per
  deploy-environment via a CI step). Resolves at strangler-fig
  step 8; no ADR change needed if the static-file solution holds.
- Whether `defaults.toml` should be schema-validated at compile time.
  Possible via a `build.rs` that parses it through the same `Config`
  struct, failing the build if defaults are invalid. Deferred until
  the schema stabilises; until then, the runtime parse error on
  first invocation is acceptable feedback.
- Whether per-tenant configuration is needed (a single binary serving
  multiple deploys with different repos). Not in MVP scope. Re-opens
  if a SaaS deploy emerges. The `Config` struct shape supports
  multi-tenancy via `Vec<TenantConfig>` without changing the
  read-site discipline.
- Whether secret values (e.g. `PART_REGISTRY_TRANSPORT_GITHUB_TOKEN`)
  should be read from a secret manager rather than env. Out of MVP
  scope; env is the 12-factor-prescribed substrate and is acceptable
  for the deploy targets in scope (developer laptops, lab machines,
  CI). Re-opens if a multi-tenant SaaS or enterprise deploy
  surfaces a compliance requirement (e.g. SOC 2, HIPAA-BAA) that
  forbids plaintext env-var secrets.
- Whether `clap`-based CLI arg parsing should be a fifth figment
  provider (currently shown as conceptually merged last) or remain
  CLI-only with explicit override calls. Implementation detail
  deferred to step-3 PR review.

## References

- 12-factor.net principle III — Config: <https://12factor.net/config>
- 12-factor.net principle IV — Backing services:
  <https://12factor.net/backing-services>
- `figment` — <https://docs.rs/figment/>
- `config-rs` — <https://docs.rs/config/>
- [ADR-013 — Parts registry web app](ADR-013-parts-registry-web-app.md)
- [ADR-017 — Rust core, ports/adapters, multi-target deploy](ADR-017-rust-core-ports-adapters.md)
- [ADR-018 — Storage as a port](ADR-018-storage-port.md)
- [ADR-019 — Proposal sink as a port](ADR-019-proposal-sink-port.md)
- [ADR-020 — Identity & authorization as a port](ADR-020-identity-authorization-port.md)
- [ADR-022 — Observability: tracing + audit trail](ADR-022-observability-tracing-audit.md)
- [ADR-023 — Threat model + crypto-MVP scope](ADR-023-threat-model-and-crypto-mvp-scope.md)
- [ADR-024 — Cryptographic baseline (MVP)](ADR-024-crypto-baseline-mvp.md)
- [ADR-027 — Port conformance + forward-compatibility tests](ADR-027-port-conformance-tests.md)
- ISO 18004 §6.3.7 — QR Code quiet-zone requirement (cited for the
  non-configurable `QR_BORDER_*` constants)
- Source files audited for hardcoded values:
  `label.py:33-81`, `web/src/config.ts:1-42`

## Corrections

### 2026-05-15 — Env-var nested-key separator is double-underscore

The "Config crate shape" section above (lines 136-167) specifies
single-underscore composite env-var names (e.g.
`PART_REGISTRY_STORAGE_SQLITE_PATH`). The implementation in PR #37
uses **double-underscore** (`PART_REGISTRY__STORAGE__SQLITE_PATH`) as
the nested-key separator. This is necessary because field names like
`default_size_mm` contain underscores; a single-underscore separator
would make `PART_REGISTRY_LABEL_DEFAULT_SIZE_MM` ambiguous between
`label.default_size_mm` and `label.default.size.mm`.

The double-underscore convention is documented in
`crates/config/src/lib.rs` (§"Env var convention") and implemented via
the `ENV_SEPARATOR` constant. The prefix remains `PART_REGISTRY_`
(with a trailing single underscore); the first `__` after stripping
the prefix marks the section boundary.

Per the project's METHODOLOGY correction protocol this note is
appended rather than silently revising the body above, so the original
rationale and the corrected convention are both visible in review.

### 2026-05-15 — Adapter selection as flat enum, not tagged enum

The "Config crate shape" section specifies `StorageAdapterChoice` as a
`#[serde(tag = "kind")]` enum carrying associated data inside variants
(e.g. `Sqlite { path: PathBuf }`). The implementation uses flat
`#[serde(rename_all = "snake_case")]` enums for adapter selection
(e.g. `StorageAdapterChoice::CsvGit`) with associated config fields as
sibling `Option<T>` fields on the parent struct (e.g.
`StorageConfig::sqlite_path`).

This is pragmatic for figment env-var binding: figment's `Env` provider
cannot populate a `#[serde(tag = "kind")]` enum from a single env var.
The flat enum preserves the type-level exhaustiveness guarantee (a
`match` on the enum covers all known adapters) while keeping env-var
override ergonomics. Adapter+field consistency (e.g. "if sqlite then
`sqlite_path` must be set") is validated at startup in the adapter
constructor, not at deserialization time.

Applies to `StorageAdapterChoice`, `IdentityAdapterChoice`,
`TransportAdapterChoice`, and `SigningAdapterChoice`.
