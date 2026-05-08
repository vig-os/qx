# Decision log — part-registry

Append-only chronological record of decisions for the parts registry.
Newest entries first.

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
