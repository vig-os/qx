# docs/

Operator-facing documentation for qx.

| File | For | Length |
|---|---|---|
| [`operator-cheatsheet.md`](operator-cheatsheet.md) | Lab-floor reference. One A4 page, dense, pin-near-the-printer format. | ~750 words |
| [`quickstart.md`](quickstart.md) | New-operator narrative walkthrough. PWA install → first scan → first bind → first print → where to find help. | ~1850 words |
| [`registry-anatomy.md`](registry-anatomy.md) | Admin reference. Every file in a deployed registry repo — purpose, governing contract, how it's enforced/checked, and who administers it. | ~900 words |

For the system overview and CLI reference, see the top-level
[`README.md`](../README.md). For design rationale, see
[`../decisions/`](../decisions/).

## Scope

These are **markdown** docs that render on github.com today. The
in-app docs route — inline help icons, dismissable quickstart panel,
printer-friendly cheatsheet route inside the SPA — is tracked
separately in [issue #8](https://github.com/MorePET/part-registry/issues/8).
When that lands, this folder becomes its source.

## Conventions

- Plain English, short sentences. Operators here are technical (it's
  a research lab) but the docs assume no prior qx
  knowledge.
- `> Note:` callouts only for the genuinely surprising things — not
  for routine information.
- No screenshots yet. The UI is still moving; screenshots will be
  added once it stabilizes (per issue #8).
- Links use **relative paths** so they work in github.com's web view,
  in the cloned repo, and in any future static-site renderer.

## Updating

Docs are part of the registry. Every change is a PR, every fix is a
commit. If something here is wrong, unclear, or missing — file a PR
or an issue.
