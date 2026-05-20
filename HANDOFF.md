# HANDOFF — part-registry full implementation

## Goal
Work through all open issues, deploy to test repo, write comprehensive tests.

## What's done
- **Phase 0**: All 5 FE PRs merged (#62-68). Issues #6, #7, #10, #23 closed.
- **Phase 1**: 6 milestones created, all issues assigned.
- **Phase 2 (partial)**: #38, #44, #52, #53 implemented and merged (PRs #69-72 + #70). CI fixes for bind.ts type mismatch (PR #73) and pages.yml wasm-bindgen (PR #74) merged.

## What's in flight
- **Agent (SOUP harnesses)**: Working on #45 + #46 in worktree. SOUP H1 (ID alphabet/collision) and H2 (QR roundtrip/golden).
- **Agent (Schema + CLI)**: Working on #18 + #56 in worktree. Schema additions (minted_by/bound_by/last_edited) + CLI void-notes parity.

## What's next
- **#43** (coverage binary): Depends on #44 (done) + #45/#46 (in flight). Implement after harnesses land.
- **Phase 4** (#14, #20, #5, #1): Web app features — always-on decoder, scanner multi-pick, proposal broker.
- **Phase 5** (#15, #11): Print/layout — flag options, matrix studio.
- **Phase 6**: Comprehensive Playwright test suite (SSoT-driven from registry-contract.json).
- **Phase 7**: CI portability + SSoT enforcement tests.
- **Phase 8**: Deploy to exo-pet/exopet-registry-sandbox.
- **Phase 9**: Housekeeping (#16, #17), defer #13/#19.

## Key files
- `schema/registry-contract.json` — SSoT
- `crates/domain/src/lib.rs` — Part struct, PartId
- `crates/validators/src/lib.rs` — REGISTRY_HEADER
- `crates/config/src/lib.rs` — typed adapter enums (StorageAdapterChoice etc.)
- `crates/observability/src/lib.rs` — OperatorGuard
- `web/src/tabs/bind.ts` — fixed type mismatch
- `.github/workflows/pages.yml` — fixed wasm-bindgen install

## Plan file
`/home/larsgerchow/.claude/plans/tidy-toasting-popcorn.md`
