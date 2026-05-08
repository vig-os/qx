# Decision log — part-registry

Append-only chronological record of decisions for the parts registry.
Newest entries first.

## 2026-05-08 — Print event log (CLI side)

**Context:** issue #12 — a printed-but-unbound label is a real
artifact, but `status` (per ADR-012) is the *logical* unbound/bound/void
relationship and cannot represent multiplicity (a sticker can be
reprinted). Audit traceability needs an event log, not a status
promotion.

**Outcomes:** ADR-015 (Print event log), Status: Proposed. New
`print_log.csv` at the repo root with the schema
`id,printed_at,printed_by,layout,size_mm,extra,copies,output_mode,batch_label`.
`label.py` grows `--log` / `--no-log` (default on), `--operator`
(default `$USER`), `--output-mode` (default `dk-continuous-auto-cut`).
After every successful render of all SVGs the script appends one row
per ID and re-sorts by `printed_at` for stable diffs. `extra` is a
JSON-encoded string of layout-specific options (`{}` for vert/horz,
`{"cableOd":N}` for flag).

**Process notes:** the FE wiring (queue print events into the same
PR pipeline as bind diffs) and the validator wiring (FK to
registry.csv, sort-stability, header equality) are explicitly out of
scope for this CLI-only change — separate follow-up work tracked in
the web app and validators issues. The CLI prints a stderr warning
on local FK miss but still logs; CI is the source of truth for
orphan events.

**References:** ADR-015, issue #12, `label.py`, `print_log.csv`.

## 2026-05-08 — Web app spike (architecture + Lookup/Print/Bind tabs + Error Report plugin)

**Context:** ADR-013 specified the phase 2 web app deployment shape;
user requested a working spike (`web/` directory in this repo) the
same day to validate the architecture and start running labels through
the print path.

**Outcomes:** ADR-014 (web app architecture: extension interfaces,
SSOT, plugin model), Status: Proposed. Working SPA at `web/` with
Vite + TypeScript build, deployed to GitHub Pages via the
`.github/workflows/pages.yml` action on every push to `main` that
touches `web/**` or `registry.csv`.

**Process notes:** the architecture commits to three small interfaces
— `Tab`, `Layout`, `Plugin` — each with its own registry. Adding a
new extension is one file + one registry line + zero core changes.
This is an explicit invariant captured in ADR-014.

Three SSOTs locked:

1. `src/config.ts` — repo slug, registry URL, ID alphabet/length/regex,
   QR border, tape sizes (`pt-N` for P-touch, `dk-N` for QL DK rolls),
   default size.
2. `src/registry/schema.ts` — registry row shape + field metadata
   (`FIELDS` array with `label`, `editable`, `meaningfulFrom`).
   Lookup detail view, Bind form, future validators all read from
   here.
3. `src/registry/registry.ts` — sole `Registry` interface; data layer
   abstracts CSV-from-raw.githubusercontent.com today, will be
   DuckDB-WASM later. Tabs depend on the interface, never on `fetch`.

A drift risk was acknowledged and explicitly traded: the SVG layout
renderers in `web/src/layouts/` are a TypeScript port of `label.py`.
The proper SSOT (Pyodide-loaded `label.py` so FE and CLI run literally
the same code) is the long-term direction per ADR-013 but was deferred
for spike speed. The migration trigger is captured in ADR-014: any
layout-change PR that requires editing both sides, or a roundtrip-test
failure traced to FE-CLI divergence.

The Error Report plugin demonstrates the plugin model end-to-end:
`html2canvas-pro` snapshot → clipboard write → opens prefilled GitHub
issue URL with environment and description, no OAuth token required.

The Bind tab is fully scaffolded with a real localStorage queue but
the GitHub-API submission path is stubbed — the user clicks "submit
batch" and gets an alert showing the queued rows. Implementing the
real OAuth device flow + REST API batch PR creation is a sub-task of
issue #1.

User added a follow-up: Lookup tab should also expose inline edit
that funnels through the same bind-queue infrastructure (DRY — the
queue knows about row diffs, doesn't care whether the diff originated
in a bind or an edit). Filed as a sub-task of issue #1; ADR-014
references it in Consequences.

**References:** ADR-012, ADR-013, ADR-014, `web/`, issue #1.

## 2026-05-08 — Repository extracted from MorePET/exopet

**Context:** ADR-012 (Part identification) and ADR-013 (Parts registry
web app) were drafted in `MorePET/exopet/system-design/parts/` during a
single design session on 2026-05-08. ADR-013 identified "when phase 2
work begins" as the trigger to extract; user moved the extraction
forward to bootstrap the registry as a standalone, public, share-able
artifact and to start labeling parts the same day.

**Outcomes:** new repo `MorePET/part-registry` (public). Files
relocated:

- `system-design/parts/{mint,label,bind,test_labels}.py`,
  `registry.csv`, `examples/` → repo root
- `system-design/decisions/{ADR-012,ADR-013}-*.md` → `decisions/`
- `system-design/decisions/{METHODOLOGY,ADR-template}.md` →
  `decisions/` (audit framework carried over)

The original ADR-012 and ADR-013 files are the canonical source going
forward in this repo. The `MorePET/exopet` decisions index has been
updated to add an "externally hosted ADRs" section pointing readers
here. ADR numbering continues from 014 onward in this repo; the 001-011
ADRs are exopet-specific hardware decisions and stay there.

History was *not* preserved via `git filter-repo` / `git subtree split`
— the parts code was new on the same day, history was minimal, and the
urgency (lab needs to print labels today) outweighed the audit benefit
of preserved history. The exopet-side LOG entry from 2026-05-08
remains as the historical record of how the design evolved.

The repo starts public to remove paid-plan dependencies for GH Pages
deployment (per ADR-013) and to bootstrap quickly. Plan is to move
private once the registry contains operational data — though ADR-013's
argument that the registry data is generally non-sensitive (hardware
IDs + locations, not vendor pricing) means public may end up being the
steady state.

**Process notes:** the GitHub issue tracking phase 2 implementation
work was filed on `MorePET/exopet#13` before extraction; transferred
to `MorePET/part-registry` as part of this move so the work item
lives with its target repo.

**References:**
[`MorePET/exopet/system-design/decisions/LOG.md`](https://github.com/MorePET/exopet/blob/main/system-design/decisions/LOG.md)
(entries from 2026-05-08 documenting the original design session);
ADR-012; ADR-013.
