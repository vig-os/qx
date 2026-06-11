{
  description = "part-registry — Class B regulated traceability platform (per ADR-017)";

  # Per #35 Phase 3: one `nix develop` brings up the full dev environment
  # — Rust toolchain pinned to `rust-toolchain.toml`, Node 22 + npm, uv
  # (for Python parity tests + tools/), wasm-pack, wasm-bindgen-cli,
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

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
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

  outputs = { self, nixpkgs, flake-utils, rust-overlay, guardrails }:
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

            # Python parity CLIs + tools/
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
            # (sheet.py, obligations_check.py, …). Declare it an output surface so
            # guardrails' no-debug-leftovers gate stays high-signal on lib/app
            # code instead of flagging legitimate command output.
            GUARDRAILS_OUTPUT_GLOBS = "tools/*:*/tools/*";
          };

          # Appended after the guardrails banner so the project cheatsheet
          # is the last thing printed on shell entry.
          hook = ''
            echo "part-registry dev shell"
            echo "  rust:      $(rustc --version)"
            echo "  node:      $(node --version)"
            echo "  wasm-pack: $(wasm-pack --version)"
            echo ""
            echo "  cargo test --workspace        # Rust gate"
            echo "  cd web && npm ci && npm test  # FE gate"
            echo "  cd web && npm run e2e         # Playwright headless"
            echo ""
          '';
        };

        # Pipeline-as-derivation (ADR-038 §4): CI logic lives in flake
        # outputs; .github/workflows/ci.yml is a thin shim running
        # `nix flake check`. Hermetic by construction — the Nix sandbox
        # gives the obligations gate no network and a pure source tree,
        # so local `nix flake check` and CI are the same run.
        checks.obligations = pkgs.runCommand "adr-obligations" { } ''
          cd ${./.}
          ${pkgs.python3}/bin/python3 tools/obligations_check.py
          touch $out
        '';

        # Future: `nix build` for the FE bundle + the static (musl)
        # gate binary + the dockerTools runner image as derivations
        # (ADR-038 §4 end-state), so CI and release.yml consume the
        # same artifacts. release.yml stays the source of truth until
        # the gate build moves in-flake.
      });
}
