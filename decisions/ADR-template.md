# ADR-NNN — Title

- Status: Proposed | Accepted | Research | Deferred | Rejected | Superseded by ADR-XXX
- Date: YYYY-MM-DD
- Component / area: short identifier (e.g. `sdmd_v2 cooling loop`)
- Reviewers: name, name (required for Status: Accepted)
- Trigger conditions: only if Status is Deferred — list the conditions that re-open this decision
- Supersedes: ADR-XXX (only if applicable)
- Superseded by: ADR-XXX (only if applicable)

## Context

What problem are we solving. What constraints apply. What was the
starting set of options. What earlier decisions or measurements feed
into this one.

Cite sources inline (datasheet section, manual page, calc file path).

## Alternatives considered

Mandatory section. Every option that got serious consideration, with
the reason it was rejected. Table form is preferred:

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| Option A | … | … | Chosen / Rejected: reason |
| Option B | … | … | Rejected: reason |

If only one option was considered, justify why no alternatives were
evaluated (e.g. single-source vendor constraint, regulatory mandate).

## Decision

The chosen option, stated as a single sentence. Include vendor links,
part numbers, configuration values as needed.

## Rationale

Why this option won. Cite sources for any quantitative or regulatory
claim. Reference the relevant calculation file, measurement, or
standard clause.

## Consequences

What this commits the project to:

- Process discipline (assembly steps, calibration intervals, operator
  training).
- Compatibility constraints with adjacent subsystems.
- Lead time / sourcing implications.
- Downstream ADR triggers (what changes would force a revisit).

## Corrections

Only if applicable. Record wrong assumptions caught after first draft
in this format:

> **YYYY-MM-DD:** original text claimed X. Source Y showed the correct
> value is Z. Decision/numbers updated above; this entry preserves the
> error for audit.

Do not delete. Multiple corrections stack chronologically.

## Open questions / supersession triggers

What would invalidate this decision. Examples: a measurement that fails
a model, a vendor end-of-life, a scope expansion that pushes a
software item from Class A to Class B.

## References

- Datasheet / manual / paper title — section, page
- `path/to/calc_file.py`
- `explorations/<topic>.md`
- Standard clause: ISO XXXXX §N.N
