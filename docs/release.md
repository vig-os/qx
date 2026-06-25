# Release bundle contract

Per [ADR-019](../decisions/ADR-019-proposal-sink-port.md) +
[ADR-025](../decisions/ADR-025-distribution-integrity.md) +
[#35](https://github.com/MorePET/part-registry/issues/35).

The `release.yml` workflow tags every push on `v*` and publishes a
**frontend bundle** as a GitHub Release asset. Each data repo's
`pages.yml` consumes that bundle to deploy a per-registry FE site,
baking its own `VITE_DATA_REPO` at build time.

## What's in the bundle

```
frontend-bundle-<tag>.tar.gz
├── BUNDLE_METADATA.json    # { tag, commit, code_repo, built_at }
├── schema/
│   └── registry-contract.json
└── web/
    ├── package.json
    ├── package-lock.json
    ├── tsconfig.json
    ├── vite.config.ts
    ├── vitest.config.ts
    ├── index.html
    ├── src/
    │   ├── main.ts
    │   ├── config.ts
    │   ├── ... (full source)
    │   └── wasm/
    │       ├── qx_wasm.js
    │       ├── qx_wasm_bg.wasm
    │       └── qx_wasm.d.ts
    ├── test-fixtures/
    └── README.md
```

Excluded: `node_modules/`, `dist/`, `*.tsbuildinfo`, `.DS_Store`.

Also published alongside: `sha256sums-<tag>.txt`.

## Why source-plus-prebuilt-WASM

Two reasons:

1. **`VITE_DATA_REPO` is baked at build time.** Vite resolves
   `import.meta.env.VITE_DATA_REPO` during `npm run build`, so a
   pre-built `dist/` is locked to one data repo. Shipping source lets
   each data repo bake its own value.
2. **No Rust toolchain on the consumer.** The Rust→WASM step is the
   slow part (cargo + wasm-bindgen-cli install + compile). Pre-running
   it here means the consumer's `pages.yml` only needs Node.

## Consumer recipe — data-repo `pages.yml`

```yaml
name: Deploy registry FE to GitHub Pages
on:
  push:
    branches: [main]
  workflow_dispatch:
    inputs:
      bundle_tag:
        description: "Code-repo release tag (e.g. v0.1.0)"
        required: false
        default: latest

permissions:
  contents: read
  pages: write
  id-token: write

env:
  CODE_REPO: MorePET/part-registry
  DATA_REPO_SLUG: ${{ github.repository }}      # e.g. exo-pet/exopet-registry-sandbox
  BUNDLE_TAG: ${{ inputs.bundle_tag || 'latest' }}

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - name: Download release bundle
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          if [ "${BUNDLE_TAG}" = "latest" ]; then
            gh release download --repo "${CODE_REPO}" --pattern "frontend-bundle-*.tar.gz"
            gh release download --repo "${CODE_REPO}" --pattern "sha256sums-*.txt"
          else
            gh release download "${BUNDLE_TAG}" --repo "${CODE_REPO}" --pattern "frontend-bundle-*.tar.gz"
            gh release download "${BUNDLE_TAG}" --repo "${CODE_REPO}" --pattern "sha256sums-*.txt"
          fi

      - name: Verify SHA-256
        run: sha256sum --check sha256sums-*.txt

      - name: Extract bundle
        run: tar xzf frontend-bundle-*.tar.gz

      - uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: "npm"
          cache-dependency-path: web/package-lock.json

      # See code-repo `.github/workflows/playwright.yml` for the
      # `npm install` vs `npm ci` rationale.
      - name: npm install
        working-directory: web
        run: npm install --no-audit --no-fund

      # `build:fe` — vite + tsc only. Skip `build:wasm` because the
      # bundle already ships prebuilt wasm-bindgen artifacts.
      - name: npm run build:fe
        working-directory: web
        env:
          VITE_DATA_REPO: ${{ env.DATA_REPO_SLUG }}
          VITE_BASE: /${{ github.event.repository.name }}/
        run: npm run build:fe

      - uses: actions/configure-pages@v5
      - uses: actions/upload-pages-artifact@v3
        with:
          path: web/dist

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

Phase 3 lands this verbatim into both data repos via the bootstrap
script.

## Tagging a release

```bash
# Bump version, commit, tag, push tag.
git tag v0.1.0
git push origin v0.1.0
```

`release.yml` triggers on the tag push, runs the full test suite, then
builds and uploads the bundle. Manual re-runs via `workflow_dispatch`
re-upload assets with `gh release upload --clobber` — idempotent.

## Integrity verification

`sha256sums-<tag>.txt` is signed by GitHub's release-asset upload (TLS
to the GitHub API). Consumers verify with `sha256sum --check` before
extracting. Future hardening (ADR-025 §"Future Cosign"): sign the
bundle with a Sigstore certificate so downstream consumers can verify
provenance without trusting GitHub.
