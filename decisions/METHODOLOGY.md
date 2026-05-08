# Decision-Process Methodology

This page is the playbook for how decisions get made and recorded in the
EXOPET project. It is short on purpose: the goal is one place an auditor
or a new engineer can read in five minutes and then understand every
record in this folder.

## Purpose

The decisions/ folder supports four distinct external readers, all of
whom must be able to follow the trail:

- **QMS Design Controls (ISO 13485 §7.3)** — design inputs, design
  outputs, design review records, and design transfer evidence.
- **Risk management (ISO 14971)** — record of hazard analysis, control
  selection, residual-risk acceptance.
- **Software lifecycle (IEC 62304)** — software safety classification
  rationale and architectural decisions that drive (or relax) class.
- **Notified Body / FDA review** — a third party must be able to
  reconstruct *why* a choice was made, *what* alternatives were rejected,
  and *whether* a wrong assumption was caught and corrected.

If a decision matters for any of those four, it gets an ADR.

## Record types

Three artifact types live under `system-design/`:

| Type | Folder | Purpose |
|---|---|---|
| **ADR** | `decisions/ADR-NNN-*.md` | A single decision with its alternatives, rationale, and consequences. One ADR = one commitment (or one explicit non-commitment, see Status). |
| **Exploration** | `decisions/explorations/*.md` | Architectural research and option surveys. NOT a commitment. Feeds into ADRs but is preserved separately so the working-out is auditable. |
| **Log entry** | `decisions/LOG.md` | Append-only chronological process trail. One entry per decision-making session, capturing what was decided, what was explored, and what corrections were caught in flight. |

## Status taxonomy

Every ADR carries exactly one of six Status values. These are the only
allowed values, with these definitions:

- **Accepted** — in force, governs current work. Engineering builds
  against this.
- **Proposed** — recommended, awaiting team review. Not yet binding.
  Anyone implementing against a Proposed ADR does so at their own
  schedule risk.
- **Research** — architectural exploration. NOT a commitment. The ADR
  exists to capture the shape of a future decision and the constraints
  it must satisfy, so the work is not lost. Do not implement against
  Research ADRs.
- **Deferred** — postponed with explicit trigger conditions documented.
  The ADR must list the conditions that would re-open the decision
  (e.g. "WCET measurement on Linux-PREEMPT_RT exceeds 200 µs", "Class
  B/C scope expands beyond X").
- **Rejected** — considered and decided against, with reason. Kept on
  file so the alternative does not get re-proposed without new
  information.
- **Superseded by ADR-NNN** — replaced by a newer ADR. The pointer is
  mandatory; never silently delete a superseded ADR.

## Required content per ADR

Every ADR must contain these sections, in this order:

1. **Context** — what problem is being solved, what constraints apply.
2. **Alternatives Considered** — every option that got serious thought,
   with the reason for rejection. A table is the preferred form. An ADR
   that lists only the chosen option fails audit.
3. **Decision** — the chosen option, stated as a single sentence (plus
   parts/vendors/links as needed).
4. **Rationale** — *why* this option won, with citations.
5. **Consequences** — what this commits us to: process discipline, lead
   times, compatibility constraints, downstream ADR triggers.

Optional sections, used when applicable:

- **Corrections** — wrong assumptions caught after first draft. See the
  Correction protocol below. Preserve, do not delete.
- **Open questions / supersession triggers** — what would invalidate
  this ADR.
- **References** — datasheets, manuals, papers, agent reports.

## Evidence requirements

Any claim with regulatory or engineering weight must be cited:

- **Datasheets and manuals** — cite the document and the section/page
  (e.g. "LAUDA L 100 manual §4.2", "PETsys readout-chain overview p. 1").
- **Quantitative claims** — point to the calculation file or measurement
  record (e.g. `cooling_calc.py`, a notebook, a test report).
- **Regulatory claims** — cite the standard clause (e.g. "ISO 14971
  §5.4", "IEC 62304 §5.3"), not just the standard name.
- **Agent reports / explorations** — cite the file path under
  `explorations/` so the chain of reasoning is traceable.

A claim with no citation is a flag for the next reviewer.

## Correction protocol

When a wrong assumption is caught after an ADR is drafted, **document
the correction visibly inside the ADR**. Do not silently revise the
text.

The standard form is a `## Corrections` section near the bottom that
records:

- what was originally believed,
- when and how the error was caught,
- what the corrected value/decision is,
- the source that overrode the original.

This is the single most important habit for audit defensibility. A
Notified Body review that finds a quietly-rewritten ADR loses trust in
every other ADR in the folder. A review that finds an ADR with a
"Corrections" section reading "initial estimate of 12 W/module was
wrong; PETsys datasheet p. 1 gives 3.6 W; total revised from 144 W to
43.6 W" gains trust.

The same protocol applies to Log entries: corrections caught during a
session go into the **Process notes** field for that session.

## Status transition rules

- **Research → Proposed** — only when the option set has narrowed to a
  single recommendation and the open questions are answered.
- **Proposed → Accepted** — requires named reviewers in the frontmatter
  and a date.
- **Deferred → Accepted** — only after the documented trigger conditions
  are met; the ADR must record what evidence triggered activation.
- **Any → Superseded by ADR-NNN** — only when a successor ADR exists.
  The successor must back-reference (`Supersedes: ADR-NNN`) and the old
  ADR's Status line must be updated. Do not delete.
- **Any → Rejected** — keeps the record. Useful when an ADR is drafted
  and then the team picks a different path; the rejected ADR is the
  evidence the alternative was considered.

## Audit principles

The folder is structured around four principles, in priority order:

1. **Traceable** — every decision has a paper trail from input to
   commitment. README → LOG → ADR → cited source.
2. **Alternatives enumerated** — an ADR with no alternatives section
   is incomplete. The auditor must see what was rejected and why.
3. **Self-correcting** — wrong assumptions are caught and recorded
   visibly, not erased. The Corrections protocol is non-negotiable.
4. **Forward-thinking work distinct from current commitments** —
   Research and Deferred ADRs and the explorations/ folder exist so
   architectural thinking gets captured without being mistaken for a
   commitment. An auditor reading an Accepted ADR knows engineering is
   building against it; an auditor reading a Research ADR knows it is
   not yet binding.
