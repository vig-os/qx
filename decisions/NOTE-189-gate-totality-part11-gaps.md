# NOTE-189 — What the gate structurally *cannot* certify (Part 11 / ALCOA+ / IEC 62366)

- Status: Liability fence (spike #189, delta D4) — not an ADR; input to a future signing/authorization ADR.
- Date: 2026-06-01
- Related: ADR-016 (CI is the policy authority), ADR-019 (proposal sink), ADR-020 (identity+authz), ADR-023 (threat model / offline defer-signing), ADR-024 (crypto baseline / signing port).

## Purpose

The architecture's load-bearing slogan is **"validate the gate, not the tool"**: because the gate (PR + ruleset + CI re-running the same Rust validator core) re-validates every proposal's *output*, the producers (web app, OCR, bulk import, future plugins) can be down-classified out of data-corruption validation scope. That claim is **true for output integrity and dangerous if stated unqualified to an auditor.** This note fixes the boundary so nobody over-sells gate-totality later.

**The gate is an output filter, not a process witness.** It can discern *altered* records; it cannot discern an *invalid-but-well-formed* one, and it never witnessed the *human act* Part 11 cares about. Three things are therefore **outside any gate's reach** and must be controlled elsewhere.

## Gap 1 — E-signature meaning/binding (designable; SHOWSTOPPER if leaned on)

A git merge-approval is **not** a 21 CFR Part 11 signature. §11.50 requires the signed record to show signer printed name, date/time, and the **meaning** of signing (authorship/review/approval); §11.70 requires the signature **cryptographically bound** to the record; §11.200 requires a non-biometric signature use **two components** with **re-authentication at signing** (not session login), and that continuous signings each bind positively. A `qms-approver` review-click satisfies none of meaning / re-auth / two-component.

**Control:** put a **signature manifest** in the record — `{signer, printed_name, signed_at, meaning_of_signature, auth_event_ref}` — require IdP re-auth at the moment of signing, and have the gate verify the manifest is present, bound, and meaning-tagged for the action class. The schema is already forward-compatible (`AuditEntry.signatures: Vec<Signature>`, ADR-023/024). **Until this ships, do not call any approval a Part 11 e-signature.**

## Gap 2 — ALCOA+ contemporaneousness of offline edits (designable; currently recorded-but-unenforced)

The gate sees **merge-time**, not activity-time. Offline-then-merge records-then-submits-later by construction. ADR-023's defer-signing dual timestamp (`claimed_at` / `signed_at`) is the right shape — but `claimed_at` is **operator-asserted, hence back-datable**, which is exactly the pattern FDA data-integrity findings target.

**Control:** capture `claimed_at` from a **signed, trusted local-clock event** the operator cannot freely rewrite, carry it in the synced payload **covered by the signature**, surface the offline window `(signed_at − claimed_at)` explicitly, and SOP-bound acceptable windows (anomalies = deviations). Then "contemporaneous-at-claim, verified-at-merge, window-disclosed" is defensible. Disclosed residual risk is forgiven far more readily than a discovered silent gap.

## Gap 3 — Valid-but-wrong / use-error (accepted residual risk — correctly)

A typo'd-but-format-valid value (right syntax, wrong lot number) passes any syntactic gate. This is the **definition** of an IEC 62366 use-error; the gate is the wrong layer and must not pretend otherwise.

**Controls live elsewhere and must be in the ISO 14971 + IEC 62366 risk file:** (i) **capture-time use-error reduction** — constrained pick-lists over free text, scan-don't-type, confirmation of high-consequence fields, OCR cross-check; (ii) **independent human verification** for high-consequence fields (a `qms-approver` who actually verifies content, not just merges). **Consequence:** these make the front-end a *risk-control measure*, which pulls it **back** into validation scope for those controls — so the slogan must stop claiming proposers leave scope wholesale.

## The defensible reframe

> **Validate the gate *for integrity*; validate the act-of-creation surfaces *for signature / contemporaneousness / use-error*; regenerate per-configuration evidence for everything in between.**

- **Defensible to an auditor today:** tamper-evidence, attribution provenance (`source`/`verified_at`), the diff-as-authority gate as an *integrity* control, reproducible builds, and down-classifying proposers from *data-corruption* validation scope.
- **Liabilities if leaned on:** (1) calling merge-approval a Part 11 e-signature; (2) treating the gate as sufficient for contemporaneousness; (3) claiming proposers leave validation scope wholesale; (4) selling configurability ("validation-cost-collapse", delta D5) without a **per-module IQ/OQ/PQ + traceability generator** — without it, configurability makes the validated state *undefinable* and the system *less* auditable.

## Action items (own issues)

- [ ] Signature-manifest in the record + re-auth-at-signing; gate verifies presence/binding/meaning. (future signing ADR)
- [ ] Trusted-clock `claimed_at` + disclosed offline window + SOP bound.
- [ ] Risk-file the valid-but-wrong residual; add capture-time use-error controls; stop claiming the FE is out of scope.
- [ ] Per-module validation-evidence generator before any D5 configurability is sold.
