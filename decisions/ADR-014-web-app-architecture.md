# ADR-014 — Web app architecture: extension interfaces, SSOT, plugin model

- Status: Superseded by ADR-030
- Date: 2026-05-08
- Component / area: `web/` static SPA. Implements the FE half of
  ADR-013.
- Builds on: ADR-013.

## Context

ADR-013 sketched the parts-registry web app at the deployment level —
GH Pages + WASM DuckDB + PR-driven binds. This ADR locks the
*internal* architecture: how the SPA is organized so that **adding a
new tab, label layout, or plugin doesn't require modifying the core**.

The constraint set for the architecture:

- **Extensibility** — three things need to grow over time without
  touching unrelated code: tabs (Lookup, Print, Bind today; Settings,
  Stats, Mint later), label layouts (vert, horz, flag today; circular
  tags, multi-up sheets later), and plugins (Error report today;
  Telemetry, Keyboard shortcuts, Print history later).
- **Single source of truth** — three classes of data must live in
  exactly one place: configuration constants (URLs, alphabet, tape
  sizes), registry row schema, and validation rules. Drift here is
  the failure mode the ADR-013 shared-validators decision is meant to
  prevent.
- **Don't repeat yourself** — the SVG layout logic is the most
  obvious DRY hazard: it exists in both `label.py` (Python, tooling)
  and `web/src/layouts/` (TypeScript, FE). The eventual SSOT is
  Pyodide-loaded `label.py`; the spike accepts the dual-implementation
  debt explicitly and tracks it.

## Alternatives considered

| Option | Pros | Cons | Verdict |
|---|---|---|---|
| **No framework, no extension model** — single `index.ts`, all logic inline | Zero ceremony, fastest to write | New tabs / layouts / plugins require editing the core; growth becomes painful by tab #4 | Rejected |
| **React or Svelte component tree, no plugin system** | Familiar, rich ecosystem | Components alone don't enforce the extension boundaries we want; growth still couples shared code; +50–100 KB bundle for a static-data app | Rejected: framework cost without the architectural property we want |
| **Plain TypeScript + Tab/Layout/Plugin interfaces + registries** (this ADR) | Each extension point has a small interface and a single registration call. Core never imports a concrete tab/layout/plugin. Bundle small. | Some hand-rolled DOM helpers (`src/ui/dom.ts`) instead of a library | **Chosen** |
| **Pyodide for everything, including FE rendering** (label.py + validators run in browser) | Eliminates the TS-port drift risk completely; one canonical implementation in Python | ~6 MB cold load just for Pyodide runtime; segno's pure-Python QR encoder works but adds more bytes; complex async startup | Rejected for the spike, retained as the long-term direction for **validators and label rendering** specifically (per ADR-013). The shell, tabs, and plugin system stay TypeScript. |

## Decision

The `web/` SPA is plain TypeScript built with Vite. The architecture
has three extension points, each defined by a small interface in
`src/core/types.ts`:

```ts
interface Tab {
  readonly id: string;
  readonly label: string;
  mount(container, ctx): void | Promise<void>;
  unmount?(): void;
}

interface Layout {
  readonly id: string;
  readonly label: string;
  measure(opts): { widthMm, heightMm };
  renderSvg(canonical, opts): string;
  optionFields?(): LayoutOptionField[];  // form options the Print tab exposes
}

interface Plugin {
  readonly id: string;
  install(host, ctx): void;
  uninstall?(): void;
}
```

Each extension point has a registry (`src/tabs/index.ts`,
`src/layouts/index.ts`, `src/plugins/index.ts`) that holds the
registered instances. Adding a new one is one file + one registry
line; the core's `main.ts` never changes.

### Single sources of truth

| What | Where | Consumers |
|---|---|---|
| Repo slug, registry URL, ID alphabet/length/regex, QR border, tape sizes, default size | `src/config.ts` | All tabs, all plugins, layouts (alphabet check) |
| Registry row shape + field metadata (label, editable, meaningful-from-status) | `src/registry/schema.ts` | Lookup detail view, Bind form, Print row resolution, future validators |
| Registry data access (load, query, find-by-id, batches) | `src/registry/registry.ts` | All tabs depend on the `Registry` interface, never on `fetch` |
| Layout SVG primitives (svgWrap, qrBlock, textBlock) | `src/layouts/svg.ts` | Every layout (DRY) |

### Plugin host surface

Plugins receive a `PluginHost` object with the smallest API needed for
useful integrations:

```ts
interface PluginHost {
  addToolbarButton(spec): () => void;   // returns an uninstaller
  toast(message, kind?: "info" | "error"): void;
}
```

Future hooks added on demand (e.g. tab-change observer, registry
mutation observer, modal launcher). Interface Segregation: plugins
get only what they need; growing the surface is additive.

### Print pipeline

The Print tab opens a child window with one `@page` per label
(matching the printer's auto-cut behavior on continuous DK tape) and
calls `window.print()` from the child. The page CSS sets
`size: <w>mm <h>mm; margin: 0` so the OS print dialog receives the
correct physical dimensions; the user picks the Brother in the
dialog. AirPrint discovery on Wi-Fi means iOS / iPadOS / macOS work
without any extra software (per ADR-013's printer integration
section).

### Error report plugin

The first plugin demonstrates the model end-to-end:

1. Adds a "🐞 Report" toolbar button.
2. On click: `html2canvas-pro` captures the page → `Blob` → clipboard
   (`navigator.clipboard.write([new ClipboardItem(...)])`) where
   supported.
3. Opens a new tab to `https://github.com/<repo>/issues/new` with
   query-string `title=Bug:&body=<environment+description>&labels=bug`.
4. The user pastes the screenshot from clipboard into the issue body.

No GitHub OAuth token needed for the spike — the prefilled-URL path
is anonymous from the SPA's perspective; the user authenticates with
GitHub for the actual issue creation. A future enhancement would use
an OAuth token to attach the screenshot directly via the GitHub REST
upload API.

## Rationale

**Three small interfaces beat one big component tree.** A React /
Svelte tree could implement the same UI but doesn't enforce the
extension boundary the way separate registries do. With registries
the answer to "where does layout `X` live?" is *always* "in
`src/layouts/X.ts`, registered in `src/layouts/index.ts`" — no
hunting for which parent component imports it. The cost of running
without a framework on a static-data app of this size (single-digit
KB of UI logic) is negligible; the hand-rolled `dom.ts` is ~50 lines.

**Plugins, not "settings".** The Error Report feature could be a flag
on the main shell. Making it a `Plugin` instead pays off the moment
a second non-tab feature shows up — telemetry, print history,
keyboard shortcut registry. They all want the same primitives
(toolbar slot, toast surface) and have the same lifecycle (install at
boot, optional uninstall). Defining the interface up front is cheap;
backporting it later is not.

**Schemas as TypeScript types AND data.** `FIELDS` in `schema.ts` is
a `readonly FieldDef[]` — a runtime data structure that the form
generator and table renderer iterate. This is what makes "add a new
column" mean editing one file: the editable bind form, the lookup
detail dl, and the (future) Mint admin form all build their fields
from this array.

**TS port of `label.py` is a known debt, not a hidden one.** The
`web/README.md` and `src/layouts/index.ts` both call it out. The
correct fix (Pyodide-load the Python module) is documented in
ADR-013; this ADR notes the spike's choice and the trigger to switch
(when a layout change fails the roundtrip suite *because* of FE-CLI
divergence, or when the FE adds enough layouts that maintaining two
copies is more work than the Pyodide cold-start cost).

## Consequences

- **Adding a tab**, layout, or plugin is one file + one registry line
  + zero core changes. This is an explicit invariant — if a future PR
  edits `main.ts` to "support" a new tab, that's a smell to push
  back on.
- **Bundle stays small** because the core doesn't ship framework
  weight. Production: ~45 KB for the core + lazy-loaded chunks for
  `@zxing/browser` (camera scan, lookup tab) and `html2canvas-pro`
  (error-report plugin). Pyodide is *not* part of phase-1 spike;
  loading it is the trigger that breaks the bundle-size invariant
  intentionally.
- **Validators are not yet implemented.** ADR-013 specifies them as
  a single Python module run via Pyodide in the FE and natively in
  CI. Spike status: the FE does ad-hoc input checks (regex on ID,
  required-field on bind form) but does not run the cross-row
  rules (sort stability, uniqueness, status-transition). Tracked as
  a sub-task of phase 2 issue #1.
- **Bind submission is a stub.** The queue is real and persists in
  `localStorage`; the "submit batch" button logs the queued rows and
  alerts the user. Implementing the real GitHub OAuth device flow +
  PR creation is a phase 2 work item, not part of this ADR.
- **Lookup edit is a planned addition** (issue #1 sub-task): inline
  edit on the lookup detail, queue → submit-as-PR, *reusing* the
  bind queue and submission infrastructure (DRY — both bind and
  edit produce the same kind of CSV diff, differing only in starting
  state). The shared queue model means the shared validator pipeline
  catches both kinds of change with the same code.

## Open questions / supersession triggers

- **Pyodide migration trigger.** Define operationally: when do we
  switch from TS-ported layouts to Pyodide-loaded `label.py`? Two
  candidate triggers: (a) any layout-change PR that requires editing
  *both* sides, or (b) a roundtrip-test failure traced to FE-CLI
  divergence. Either trips the migration; until then, the
  inline-comment warnings in the layout files are the discipline.
- **Validator load order.** When validators land via Pyodide, do they
  run on every keystroke, debounced, or only on submit? Sketch:
  per-row schema check on blur; cross-row checks on submit. Defer to
  the issue #1 sub-task that builds them.
- **Auth model for write paths.** OAuth device flow is sketched in
  ADR-013 but not implemented. The token storage decision (sessionStorage
  vs IndexedDB vs none-keep-prompting) wants its own ADR when phase
  2 starts.

## References

- ADR-012 — Part identification scheme.
- ADR-013 — Parts registry web app deployment architecture.
- `web/src/core/types.ts` — interface definitions.
- `web/README.md` — drift-risk note on TS-port of `label.py`.
