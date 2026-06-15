# Exploration — the QA/QC vertical: Quality Manual → "register a part"

- Date: 2026-06-14
- Status: exploration — NOT a commitment. Drafts **one full vertical**
  of the QMS preset family so the document spine is concrete, not
  hand-wavy. Feeds a future "QMS preset family" ADR (trigger now armed:
  [[hq-eqms-goal-committed]] — parts in production → write our own SOPs
  in-engine). Builds on `platform-vs-registry.md` (the HQ question).
- Companion: ADR-035 (collections metamodel), ADR-039 (contract engine
  / canonical form), ADR-036 (e-signature rungs), ADR-037 (audit anchor).
- Tracked for build as **GitHub issue #208** (H2 preset family).

## The point

A "full eQMS based on git" is not a new engine — it is **a contract with
a `documents` collection + a handful of governance collections, plus
three generic capabilities** the parts work already wants. To prove that
claim end-to-end we trace ONE vertical from the top of the QMS pyramid to
the single operational act this project actually performs today:
**registering a part.** Every rung is a record in a collection over the
same fabric; the only genuinely-new machinery is called out as `GAP`.

## 1. The document spine (general → specific)

The classic four-tier QMS pyramid, instantiated for *this* repo. Each row
is one record in the `documents` collection (or a leaf record collection),
governed by the same lifecycle + PR gate as a part.

| Tier | Artifact (this repo) | Governs | Engine primitive | New? |
|---|---|---|---|---|
| **L0 Policy** | `QM-001` Quality Manual + Quality Policy | the whole system; lists the processes | `documents` record, lifecycle `draft→review→approved→effective→retired` | descriptor only |
| **L1 System procedures** | `QSP-001` Document & Record Control · `QSP-002` Supplier Management · `QSP-003` CAPA · `QSP-004` Training | *how* controlled docs, suppliers, nonconformities, competence are handled | `documents` records; each `references` the policy | descriptor only |
| **L2 SOPs** | `SOP-PART-001` Registering a Part · `SOP-SUP-001` Qualifying a Company | a specific operational process | `documents` record; `references` its parent QSP | descriptor only |
| **L3 Work instructions** | `WI-PART-001` Using the tool to mint + bind a part | exact keystrokes / CLI | `documents` record (or `attachment` of a rendered card); `references` its SOP | descriptor only |
| **L4 Records (evidence)** | the **part** itself · the **PR + merge** · the **audit-stream** entry · a **training record** "read & understood SOP-PART-001" | proof the process ran | the `parts`/`companies`/`trainings` collections + the ONE audit stream (ADR-037) | descriptor only |

The spine is a DAG of `reference` edges (L4 record → SOP → QSP → policy),
which is also the **traceability matrix** an auditor asks for — derived,
not maintained by hand.

## 2. The governance collections (the QMS preset family)

Beyond `parts` / `companies` / `contacts` (the H1 contract), a QMS
deployment adds these collections — all plain descriptors:

- **`documents`** — controlled docs. Fields: `doc_id` (scheme `docnum`),
  `title`, `type` (enum: policy/QSP/SOP/WI/form), `parent` (reference →
  documents), `owner` (reference → personas), `body` (attachment: md →
  Typst-rendered pdf), `effective_date` (date), `review_period_months`
  (integer), `next_review` (date, `GAP`: scheduler-maintained),
  `supersedes` (reference → documents). Lifecycle = the controlled-doc
  states. `required_to_enter: "effective"` on `owner` + approval.
- **`personas`** — accountable persons (ADR-036). FK target for owner /
  approver / trainee. Host accounts ⊆ personas via the gate.
- **`trainings`** — read-&-understood records: `who` (→personas),
  `document` (→documents), `acknowledged_at` (timestamp),
  `signature` (`GAP`: e-sig rung E1+). Closes ISO 13485 §6.2 competence.
- **`capas`** — corrective/preventive actions: lifecycle
  `open→investigation→action→verification→closed`; `references` the
  triggering record. Pure descriptor.

None of these touch engine code. They are the second **preset family**
the exploration predicted — code-owned templates, instantiated by the
bootstrap command, extendable per adopter but not weakenable.

## 3. The leaf, fully drafted — `SOP-PART-001`

To prove the vertical is real, here is the operational SOP at the bottom,
in the form its `body` attachment would carry:

> **SOP-PART-001 — Registering a Part in the Registry** · owner: Quality ·
> parent: QSP-001 (Document & Record Control), QSP-002 (Supplier Mgmt)
>
> **1. Purpose.** Define how a part is created, identified, bound to a
> manufacturer, and accepted into the controlled registry.
>
> **2. Scope.** All parts entered via the `part-registry` tool against the
> production data repo.
>
> **3. Responsibilities.** *Originator* mints + drafts the part.
> *Approver* (CODEOWNER ⊆ personas) accepts via merge. Accountability is
> the host-witnessed merge, not local authorship (ADR-036).
>
> **4. Procedure.**
> 1. Confirm the manufacturer exists + is `qualified` in `companies`
>    (per SOP-SUP-001); if not, qualify first — a part cannot reach
>    `bound` against an unqualified company (`on_unknown: reject` +
>    lifecycle gate).
> 2. Mint a part id (`nano14`, ADR-012) — status `unbound`.
> 3. Enter `type` (required to enter `bound`), `description`,
>    `manufacturer` (→ companies), `part_number`, specs.
> 4. Attach the datasheet (pdf/png, content-addressed).
> 5. Open a PR. The `pr check` gate validates against the contract
>    effective **at the PR's commit** (commit-resolved, ADR-039 §6) and
>    classifies the diff.
> 6. Approver reviews + merges. Merge = transition `unbound→bound` and
>    the audit-stream entry; the merge commit *is* the e-record.
>
> **5. Records produced.** the part record · the PR/merge · the audit
> entry · (if a new originator) a `trainings` ack of this SOP.
>
> **6. References.** QM-001, QSP-001, QSP-002, ADR-012/035/036/039.

That SOP is *executable documentation*: every control it cites
(`required_to_enter`, `on_unknown: reject`, lifecycle gate, commit-resolved
check, merge-as-signature) is enforced by the contract + host, not by
trust. That is the whole thesis of a git-native eQMS in one page.

## 4. What a full git-native eQMS requires — the checklist

Mapping the vertical onto "have / GAP", so the remaining build is explicit:

| Requirement (ISO 13485 / Part 11 / Annex 11) | Mechanism | State |
|---|---|---|
| Document control (unique id, approval, effective date, revision) | `documents` descriptor + lifecycle + git history | **descriptor only** |
| Change control | PR-diff classifier + proposal sink + CODEOWNERS | **have** (ADR-016/035) |
| Record integrity / ALCOA+ | append-only NDJSON + ONE audit stream + anchor ledger | **have** (ADR-035/037) |
| Identity / accountability | personas collection + host gate (accounts ⊆ personas) | **have, design** (ADR-036) |
| Traceability matrix | `reference` DAG, derived | **have** (engine) |
| Periodic review / effective dating / expiry | `next_review`, training expiry | **GAP — scheduler** (cron emitting proposals) |
| Approval e-signature manifestation | rungs E1→E4 | **GAP — e-sig** (WebAuthn presence → PAdES) |
| Controlled-document rendering (header/footer, "uncontrolled when printed", watermark) | md/Typst → deterministic PDF | **GAP — Typst target** (wanted for parts CoC anyway) |
| Training / competence | `trainings` collection + ack signature | **descriptor + e-sig** |
| CAPA / nonconformity | `capas` collection + lifecycle | **descriptor only** |
| Tool validation (CSV/CSA) | ADRs + obligations + conformance + SOUP + signed releases = the dossier | **have, as export** (platform-vs-registry §2) |
| New-instance setup (IQ/OQ) | bootstrap command laying the preset family | **GAP — bootstrap** |

**Reading of the table:** the QMS is ~70% *descriptor work* (collections
we can write the moment the contract engine lands), ~30% *three generic
capabilities + bootstrap* — none parts-vs-SOP-specific, all reusable, and
the render + e-sig are already wanted for parts. Nothing here is a second
engine.

## 5. Sequence (how this rides on H1)

1. **H1 lands** (ADR-039 → crates/contract → validators → `pr check` →
   parts + companies contract → FE). The vertical's L4 (parts/companies)
   is then real in production.
2. **Dogfood trigger fires:** add the `documents` + `personas` +
   `trainings` collections to *this repo's own* contract and write
   QM-001 / QSP-001/002 / SOP-PART-001 as records. Zero engine change —
   pure descriptor + content.
3. **Build the three GAPs as generic capabilities** (Typst, scheduler,
   e-sig rung E1→E3), each justified first by a parts use-case.
4. **Bootstrap command** packages the preset family → a fresh "company
   HQ" repo comes up with the spine pre-seeded = its IQ/OQ baseline
   (ADR-038 §5).

## 6. Open questions for the QMS-preset ADR

- Document numbering scheme (`docnum`) — per-type prefix + running number
  vs nano? Auditors expect human-meaningful `SOP-PART-001`.
- Is `body` an `attachment` (rendered pdf) or a first-class markdown
  field with a render target? Leaning attachment + Typst render so the
  controlled artifact is byte-stable.
- Training enforcement: advisory (warn) vs hard gate (cannot act on a
  collection until ack on file)? Likely per-contract policy.
- Scheduler placement: GitHub cron (ADR-034 pattern) vs a `pr` subcommand
  run by Actions. Both emit proposals through the PR gate — neither
  mutates truth directly.
