# HANDOFF — deploy-config refactor (issue #153)

## Current goal
Implement P0+P1 of issue #153: unified `schema/deploy-config.json` + `schema/code-types.json` SSoT, replacing 5+ hardcoded places. Tape presets deprecated → `<value> <mm|pt|px>` unit selector + `printerDpi` from config.

## What's done (this session)
- v0.8.1 deployed: DataMatrix encode+decode, detail modal, CSV selection, error cards, version display
- Issue #153 filed with full spike + 5 agent reviews (12-factor, domain, SOLID, DX, config scope)
- Architecture decided and consolidated from reviews

## Architecture decisions
- **Separate file**: `deploy-config.json` in `schema/` (NOT in registry-contract.json)
- **Code types SSoT**: `code-types.json` array with id, displayLabel, scannerFormat, encoderFamily, ecLevel, minModuleSizeMm
- **Family+EC axis**: config says `micro_qr:M` not `micro_qr_m4` — encoder auto-selects version
- **Capacity validation**: reject impossible combos at config load time
- **Scanner**: read list append-only, decode silently, no warnings on legacy labels
- **Build-time**: Vite import alias `@deploy-config`, env var overrides for repo settings
- **Runtime override**: `window.__PART_REGISTRY_CONFIG__` for standalone app deployments
- **Backward compat**: absent config = all types allowed, all features enabled
- **Tape presets deprecated**: replaced by printerDpi config + unit selector (mm/pt/px)
- **Contract stays data schema only**: fields, statuses, ID rules — no deploy policy

## Files to create
- `schema/deploy-config.json` — default config with all settings
- `schema/deploy-config.schema.json` — JSON Schema for validation
- `schema/code-types.json` — SSoT for barcode symbology metadata
- `web/src/config/deploy-config.ts` — typed loader + validation

## Files to refactor
- `web/src/config.ts` — read from deploy-config instead of hardcodes
- `web/src/ui/scanner.ts:182` — format filter from config
- `web/src/tabs/print.ts:1035-1038` — dropdown from code-types.json
- `web/src/layouts/label-settings.ts:19` — CodeType derived from code-types
- `web/src/output/plan-opts.ts` — label settings injection
- `web/src/tabs/print.ts` — TAPE_SIZES → unit selector + printerDpi
- `crates/codec/src/qr.rs:110-115` — Rust decoder from config (P1)

## Open issues
- #153 — spike: 12-factor barcode type config (this work)
- #149 — print auto-size guard
- #148 — multi-row editing + filters
- #133 — device flow auth

## Previous phases (completed)
All FE PRs merged, milestones created, SOUP harnesses done, schema additions done, Playwright tests, CI fixes, sandbox deployed through v0.8.1.
