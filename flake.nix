{
  description = "part-registry — Class B regulated traceability platform (per ADR-017)";

  # Per #35 Phase 3: one `nix develop` brings up the full dev environment
  # — Rust toolchain pinned to `rust-toolchain.toml`, Node 22 + npm, uv
  # (for Python parity tests + tools/), wasm-pack, wasm-bindgen-cli,
  # Playwright + chromium, gh, jq, actionlint. CI can `nix develop -c
  # <cmd>` to get the same env as a contributor's machine.

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
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
        devShells.default = pkgs.mkShell {
          name = "part-registry-dev";

          buildInputs = with pkgs; [
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
          PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
          PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";

          shellHook = ''
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
                echo "  ⚠ playwright drift: node_modules=$pwInstalled, nix=$pwVer — pinning…"
                ( cd web && npm install --no-audit --no-fund --save-exact "@playwright/test@$pwVer" >/dev/null 2>&1 ) \
                  && echo "  ✓ @playwright/test pinned to $pwVer (matches Nix chromium)" \
                  || echo "  ✗ pin failed — run: (cd web && npm i -E @playwright/test@$pwVer)"
              else
                echo "  playwright: $pwVer (npm ↔ nix in sync)"
              fi
            fi
            echo ""
            echo "  cargo test --workspace        # Rust gate"
            echo "  cd web && npm ci && npm test  # FE gate"
            echo "  cd web && npm run e2e         # Playwright headless (Nix chromium)"
            echo ""
          '';
        };

        # Future: `nix build` for the FE bundle as a derivation, so CI
        # can consume the same artifact `release.yml` produces. Not in
        # Phase 3 scope — release.yml stays the source of truth for now.
      });
}
