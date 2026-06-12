{
  description = "part-registry — Class B regulated traceability platform (per ADR-017)";

  # Per #35 Phase 3: one `nix develop` brings up the full dev environment
  # — Rust toolchain pinned to `rust-toolchain.toml`, Node 22 + npm, uv
  # (for the design-time font tools in tools/), wasm-pack, wasm-bindgen-cli,
  # Playwright + chromium, gh, jq, actionlint. CI can `nix develop -c
  # <cmd>` to get the same env as a contributor's machine.
  #
  # The dev shell is composed on top of the shared `guardrails` flake
  # (gerchowl/guardrails): its toolbelt (prek, gitleaks, cargo-deny,
  # cargo-mutants/-bloat/-criterion, tokei, the `guardrails` CLI + the
  # editable gate scripts) rides in, and entering the shell auto-installs
  # the pre-commit hooks defined in `.pre-commit-config.yaml` so commits
  # are gated the same way everywhere. `guardrails info` lists the gates
  # and every config knob; escape one line with `guardrails-ok`.
  #
  # Pipeline-as-derivation (ADR-038 §4): every CI gate lives in
  # `checks.<system>` below. `.github/workflows/ci.yml` is a thin matrix
  # shim that runs `nix flake check`, so `nix flake check` on a laptop
  # equals CI confidence. crane vendors the workspace Cargo.lock once,
  # the artifact is shared across fmt/clippy/test/deny; RustSec
  # advisories are pinned via the `advisory-db` input so cargo-deny
  # runs OFFLINE inside the Nix sandbox.
  #
  # Architecture matrix (which checks run on which system):
  #
  #   check               aarch64-darwin   x86_64-linux
  #   ------------------  ---------------  ------------
  #   fmt                       yes             yes
  #   clippy                    yes             yes
  #   test                      yes             yes
  #   deny                      yes             yes
  #   obligations               yes             yes
  #   nx75-drift                yes             yes
  #   guardrails-gates          yes             yes
  #   web-unit                  yes             yes
  #   wasm                      yes             yes
  #   web-e2e                    -              yes   (chromium sandbox)
  #   release-binary             -              yes   (musl-style smoke)

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    # crane = vendored cargo builds from the checked-in Cargo.lock; the
    # vendored deps artifact is computed once and reused by fmt, clippy,
    # test and deny so checks stay parallel + cacheable.
    crane.url = "github:ipetkov/crane";
    # Pinned RustSec advisory database. `cargo deny` consumes it through
    # an env var so the check runs OFFLINE in the Nix sandbox; bumps are
    # a flake.lock update.
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
    # Shared code-quality / observability / perf governance. Consumed via
    # `guardrails.lib.${system}.mkDevShell`; follows our nixpkgs/flake-utils
    # so the closure stays deduplicated.
    guardrails = {
      url = "github:gerchowl/guardrails";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane, advisory-db, guardrails }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config.allowUnfree = true; # for playwright browsers on Linux
        };

        # rust-toolchain.toml is the source of truth for the channel +
        # components + targets. rust-overlay reads it directly so a
        # bump there propagates to `nix develop` without a flake edit.
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # wasm-bindgen-cli's version must match the `wasm-bindgen`
        # crate version pinned in workspace `Cargo.toml` (currently
        # 0.2.121 per Foundation #33 + #54). nixpkgs nightly tracks
        # the latest; pin by hash if the unstable channel ever drifts.
        wasmBindgenCli = pkgs.wasm-bindgen-cli;

        # Workspace builds with the same pinned toolchain the dev shell uses.
        rustPlatformPinned = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        # The obligations gate binary (crates/devtools — Rust port of the
        # retired tools/obligations_check.py, ADR-017 step 9). Built from the
        # checked-in Cargo.lock so the derivation is hermetic; the vendor step
        # fetches the whole workspace lock once and is cached thereafter.
        devtools = rustPlatformPinned.buildRustPackage {
          pname = "part-registry-devtools";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          buildAndTestSubdir = "crates/devtools";
          doCheck = false;
        };

        # crane wired to the pinned rust-toolchain — overrideToolchain
        # ensures the cargo/rustc used by every crane derivation matches
        # rust-toolchain.toml (incl. wasm32-unknown-unknown target).
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Source filter — keep .rs, Cargo.{toml,lock}, plus extras the
        # workspace tests read at runtime (schema JSON, fixtures, the
        # decisions tree obligations-check expects to find). Operates on
        # path STRINGS so lib.hasInfix substring checks work.
        src = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let
              p = toString path;
              base = baseNameOf p;
            in
              (craneLib.filterCargoSources path type)
              || (pkgs.lib.hasInfix "/schema/" p)
              || (pkgs.lib.hasInfix "/decisions/" p)
              || (pkgs.lib.hasInfix "/labels/" p)
              || (pkgs.lib.hasInfix "/web/test-fixtures/" p)
              || (pkgs.lib.hasSuffix ".toml" base)
              || (pkgs.lib.hasSuffix ".lock" base);
        };

        commonArgs = {
          inherit src;
          # The workspace root Cargo.toml is virtual (no `[package]`),
          # so crane can't infer a name. Pin it explicitly so the
          # derivation names read cleanly + the placeholder warning
          # stays out of CI logs.
          pname = "part-registry-workspace";
          version = "0.0.0";
          strictDeps = true;
          # Workspace tests exercise feature-gated code (the cli `serve`
          # fixture we missed in a prior pass). Build vendored deps with
          # every feature on so the cargoArtifacts cache stays warm for
          # every downstream check.
          # --exclude part-registry-desktop: the Tauri shell drags the
          # gtk/webkit native closure into every Linux check (glib-sys
          # build scripts fail without it) for a ~100-line dispatch
          # wrapper. It is checked by its own lighter `desktop-check`
          # below instead of taxing the shared deps artifact.
          cargoExtraArgs = "--workspace --exclude part-registry-desktop --all-features";
          # Native deps a few transitive crates need to LINK during the
          # vendor build (openssl-sys via reqwest, pkg-config consumers).
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];
        };

        # Vendored Cargo.lock artifact — computed ONCE, then fed into
        # fmt/clippy/test/deny via `cargoArtifacts`. Granular checks
        # below all reuse this so they parallelise without re-vendoring.
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        # Prebuilt wasm-bindgen JS+wasm bundle for the FE. crane drives
        # the wasm32 cargo build with vendored deps from cargoArtifacts,
        # and wasm-bindgen runs in a post-install hook so the output is
        # the exact tree `web/src/wasm/` expects. Shared by web-unit +
        # web-e2e so they don't reach back through cargo at test time.
        wasmArtifact = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "part-registry-wasm";
          version = "0.1.0";
          # crane injects --release itself; repeating it errors
          cargoExtraArgs = "--target wasm32-unknown-unknown -p part-registry-wasm";
          # The default `cargo install` step at the end of buildPackage
          # has nothing to install for a cdylib — skip it; the wasm
          # artifact lands in $cargoBuildLog's target/ dir which we
          # consume below.
          doNotPostBuildInstallCargoBinaries = true;
          doNotLinkInheritedArtifacts = true;
          doCheck = false;
          nativeBuildInputs = (commonArgs.nativeBuildInputs or [ ]) ++ [ wasmBindgenCli ];
          installPhaseCommand = ''
            mkdir -p $out
            wasm-bindgen --target web \
              --out-dir $out \
              --out-name part_registry_wasm \
              target/wasm32-unknown-unknown/release/part_registry_wasm.wasm
          '';
        });

        # web/ source filtered to the FE-only inputs npm + vitest +
        # playwright need. Drops the cached node_modules and dist tree
        # that may sit in a contributor's working copy.
        webSrc = pkgs.lib.cleanSourceWith {
          src = ./web;
          filter = path: type:
            let p = toString path; in
              !(pkgs.lib.hasInfix "/node_modules/" p)
              && !(pkgs.lib.hasInfix "/dist/" p)
              && !(pkgs.lib.hasSuffix ".tsbuildinfo" p);
        };

        # FE node_modules — buildNpmPackage installs from package-lock.json
        # via a fetched FOD; npmDepsHash is the lockfile's content hash.
        # Update by setting to "" once, then pasting the value the build
        # error reports back.
        webNodeModules = pkgs.buildNpmPackage {
          pname = "part-registry-web-node-modules";
          version = "0.0.1";
          src = webSrc;
          npmDepsHash = "sha256-45u6extUUqQJoLrcWWuS9qvMNNTKLX++Bh63HRfYBlY=";
          dontNpmBuild = true;
          installPhase = ''
            mkdir -p $out
            cp -r node_modules $out/
          '';
        };

      in {
        # mkDevShell layers the guardrails toolbelt + auto-hook-install
        # under our project tools (`extra`), the shell `name`, the
        # Playwright env vars (`env`), and our cheatsheet (`hook`).
        devShells.default = guardrails.lib.${system}.mkDevShell {
          inherit pkgs;
          name = "part-registry-dev";

          extra = with pkgs; [
            # Rust workspace
            rustToolchain
            wasmBindgenCli
            wasm-pack

            # FE
            nodejs_22

            # uv stays for the remaining design-time Python font tools
            # (tools/bake_glyph_font.py + tools/font_editor_gen.py); it
            # leaves with their Rust port. Operational Python is gone
            # (ADR-017 step 9).
            uv

            # CI / repo tooling
            gh
            jq
            actionlint
            shellcheck

            # Playwright + chromium for the FE e2e suite. Linux pulls
            # the bundled browsers via nixpkgs; macOS users get
            # Chromium for free (system Chrome works too — set
            # PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH if you want that).
            playwright-driver.browsers
          ];

          # Playwright's node bindings expect to find browsers next to
          # the driver. nixpkgs ships them at a fixed store path that
          # we surface via these env vars so `npx playwright test`
          # works without network access.
          env = {
            PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
            PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";

            # `tools/` holds CLI scripts whose job is to write to stdout/stderr
            # (bake_glyph_font.py, font_editor_gen.py, …), and crates/devtools
            # is the obligations gate binary whose report IS its output.
            # Declare both as output surfaces so guardrails' no-debug-leftovers
            # gate stays high-signal on lib/app code instead of flagging
            # legitimate command output.
            GUARDRAILS_OUTPUT_GLOBS = "tools/*:*/tools/*:crates/devtools/*:*/crates/devtools/*";
          };

          # Appended after the guardrails banner so the project cheatsheet
          # is the last thing printed on shell entry.
          hook = ''
            echo "part-registry dev shell"
            echo "  rust:      $(rustc --version)"
            echo "  node:      $(node --version)"
            echo "  wasm-pack: $(wasm-pack --version)"

            # Single source of truth for the Playwright version is THIS
            # flake's nixpkgs (which also pins the chromium build). The
            # npm `@playwright/test` runner must match it exactly or it
            # looks for a browser revision the Nix store doesn't have
            # ("Executable doesn't exist …chromium_headless_shell-NNNN").
            # node_modules can drift (npm install bumps it); enforce the
            # pin here so local e2e always finds the Nix browsers.
            pwVer="${pkgs.playwright-driver.version}"
            if [ -d web/node_modules ]; then
              pwInstalled=$(node -p "require('./web/node_modules/@playwright/test/package.json').version" 2>/dev/null || echo none)
              if [ "$pwInstalled" != "$pwVer" ]; then
                echo "  playwright drift: node_modules=$pwInstalled, nix=$pwVer — pinning…"
                ( cd web && npm install --no-audit --no-fund --save-exact "@playwright/test@$pwVer" >/dev/null 2>&1 ) \
                  && echo "  @playwright/test pinned to $pwVer (matches Nix chromium)" \
                  || echo "  pin failed — run: (cd web && npm i -E @playwright/test@$pwVer)"
              else
                echo "  playwright: $pwVer (npm and nix in sync)"
              fi
            fi
            echo ""
            echo "  cargo test --workspace        # Rust gate"
            echo "  cd web && npm ci && npm test  # FE gate"
            echo "  cd web && npm run e2e         # Playwright headless (Nix chromium)"
            echo ""
          '';
        };

        # =====================================================================
        # checks.<system> — every CI gate lives here. ci.yml runs
        # `nix flake check`; local laptops run the same set. Linux-only
        # checks are attached via lib.optionalAttrs at the bottom.
        # =====================================================================
        checks = {
          # rustfmt — fast, no compile.
          fmt = craneLib.cargoFmt {
            inherit src;
          };

          # clippy — full workspace, every feature, every target,
          # warnings are errors (matches the old rust.yml).
          clippy = craneLib.cargoClippy (commonArgs // {
            inherit cargoArtifacts;
            # workspace/exclude/features come from commonArgs — repeating
            # them here makes cargo reject the duplicate flags
            cargoClippyExtraArgs = "--all-targets -- -D warnings";
          });

          # cargo test — every feature ON so feature-gated corners
          # (the cli `serve` router fixture we missed once) compile +
          # run in CI.
          test = craneLib.cargoTest (commonArgs // {
            inherit cargoArtifacts;
            # workspace/exclude/features inherited from commonArgs
            cargoTestExtraArgs = "";
          });

          # cargo-deny — licenses + RustSec advisories. The advisory-db
          # input pins the RustSec tree so the check runs OFFLINE inside
          # the Nix sandbox (no GitHub clone at gate time).
          deny = craneLib.cargoDeny (commonArgs // {
            inherit advisory-db;
            # cargo-deny takes its own flags: crane must not forward the
            # workspace/feature args, and crane already injects `check`
            # — only the WHICH list goes through cargoDenyChecks.
            cargoExtraArgs = "";
            cargoDenyChecks = "bans licenses sources advisories";
          });

          # ADR obligations gate — the Rust devtools binary against
          # decisions/obligations.toml.
          obligations = pkgs.runCommand "adr-obligations" { } ''
            cd ${./.}
            ${devtools}/bin/obligations-check
            touch $out
          '';

          # nx75 anchor font drift — the design-time baker checks that
          # crates/codec/src/glyph_font.rs is in sync with both masters
          # in design/. Baker is stdlib-only (no third-party deps), so
          # python3 from nixpkgs runs it without uv inside the sandbox.
          nx75-drift = pkgs.runCommand "nx75-drift" {
            nativeBuildInputs = [ pkgs.python3 ];
          } ''
            cd ${./.}
            python3 tools/bake_glyph_font.py --check
            touch $out
          '';

          # guardrails gates — only the SUBSET safe to run whole-tree.
          # prek runs the full set on STAGED files at commit time, and
          # sweeping the entire working tree from a Nix check turns
          # the lint-style gates into a noise wall against legitimate
          # prose comments that no PR actually authored. We keep them
          # at commit-time only:
          #
          #   commit-time only (prek):
          #     no-commented-code    — high false-positive on
          #                            descriptive prose comments
          #     no-debug-leftovers   — false-positive on tracing-style
          #                            output the tree-wide
          #                            GUARDRAILS_OUTPUT_GLOBS misses
          #     no-hardcoded         — needs per-line context the gate
          #                            tunes via prek's staged diff
          #
          #   tree-wide safe (run here):
          #     no-fake-impl         — matches well-defined sentinels
          #                            (todo!() / unimplemented!() / etc.)
          #
          # `no-conflict-markers` would also belong in the tree-wide
          # set, but the pinned guardrails rev does not ship it yet —
          # add it here on the next `nix flake update` of guardrails.
          guardrails-gates = pkgs.runCommand "guardrails-gates" {
            nativeBuildInputs = [ guardrails.packages.${system}.gates ];
          } ''
            cd ${./.}
            rs_files=$(find crates desktop -name '*.rs' -not -path '*/target/*' 2>/dev/null || true)
            ts_files=$(find web/src -type f \( -name '*.ts' -o -name '*.tsx' -o -name '*.js' \) 2>/dev/null || true)
            all_files="$rs_files $ts_files"
            if [ -n "$all_files" ]; then
              echo "$all_files" | xargs -n 50 guardrails-no-fake-impl
            fi
            touch $out
          '';

          # Wasm build — `cargo build --target wasm32-unknown-unknown
          # -p part-registry-wasm`. Exists as a derivation in its own
          # right so other checks (web-unit, pages.yml) can reuse the
          # bindgen output without re-building.
          wasm = wasmArtifact;

          # FE unit tests via vitest. node_modules come from the
          # fixed-output buildNpmPackage above so the check is hermetic,
          # and the wasm bundle is dropped in from `wasmArtifact` so
          # vitest does NOT shell out to cargo / wasm-bindgen at test
          # time.
          # Mirror the repo layout under $TMPDIR/repo so vitest.config's
          # `../schema/registry-contract.json` aliases resolve.
          web-unit = pkgs.runCommand "web-unit" {
            nativeBuildInputs = [ pkgs.nodejs_22 ];
          } ''
            mkdir -p $TMPDIR/repo/web $TMPDIR/repo/schema
            cp -r ${webSrc}/. $TMPDIR/repo/web/
            cp -r ${./schema}/. $TMPDIR/repo/schema/
            chmod -R u+w $TMPDIR/repo
            cd $TMPDIR/repo/web
            # Hard-link the FOD-fetched node_modules so npm scripts find
            # every dep without touching the network.
            cp -r ${webNodeModules}/node_modules ./node_modules
            chmod -R u+w ./node_modules
            # Drop the prebuilt wasm bundle where build:wasm would have
            # written it; vitest's setupFile loads it synchronously.
            mkdir -p src/wasm
            cp ${wasmArtifact}/* src/wasm/
            # Run vitest directly — skip the `test` npm script because
            # that re-invokes build:wasm via cargo.
            ./node_modules/.bin/vitest run
            touch $out
          '';
        }
        # Linux-only checks. Darwin's Nix sandbox cannot run chromium
        # reliably (no namespaces; the playwright bundle expects Linux
        # glibc). The release-binary smoke build mirrors the musl-style
        # target that release.yml ships.
        // pkgs.lib.optionalAttrs (system == "x86_64-linux") {
          # Playwright e2e — the production Vite build hits chromium
          # headless. Linux-only.
          web-e2e = pkgs.runCommand "web-e2e" {
            nativeBuildInputs = [ pkgs.nodejs_22 pkgs.playwright-driver.browsers ];
            PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
            PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
          } ''
            mkdir -p $TMPDIR/repo/web $TMPDIR/repo/schema
            cp -r ${webSrc}/. $TMPDIR/repo/web/
            cp -r ${./schema}/. $TMPDIR/repo/schema/
            chmod -R u+w $TMPDIR/repo
            cd $TMPDIR/repo/web
            cp -r ${webNodeModules}/node_modules ./node_modules
            chmod -R u+w ./node_modules
            mkdir -p src/wasm
            cp ${wasmArtifact}/* src/wasm/
            ./node_modules/.bin/vite build
            ./node_modules/.bin/playwright test
            touch $out
          '';

          # Release-binary smoke — the lean `pr` build release.yml
          # ships as the data-repo gate. Catches link-time regressions
          # in the binary path the install script pins.
          release-binary = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
            pname = "pr";
            cargoExtraArgs = "--release -p part-registry-cli --bin pr";
            doCheck = false;
          });
        };

        # Future: `nix build` for the FE bundle + the static (musl)
        # gate binary + the dockerTools runner image as derivations
        # (ADR-038 §4 end-state), so CI and release.yml consume the
        # same artifacts. release.yml stays the source of truth until
        # the gate build moves in-flake.
      });
}
