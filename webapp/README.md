# webapp — the webview shell (ADR-030 §3)

The new Vite + React + TypeScript + Tailwind SPA over the app-layer
command protocol. One build artifact, three deploy targets (serverless
WASM, browser → `pr serve`, Tauri webview), selected by a transport
injected at build/runtime — the UI never knows which. Replaces the old
`web/` (ADR-014 model, retired).

## Run

From the repo root (the dev shell provides Node 22):

```sh
nix develop
cd webapp
npm install
```

| Task | Command |
|---|---|
| dev server, in-memory mock backend (default) | `npm run dev` |
| dev server against `pr serve` | `VITE_TRANSPORT=http VITE_API_BASE=http://localhost:8080 npm run dev` |
| build the wasm pkg (serverless transport) | `npm run build:wasm` |
| typecheck (TS strict) | `npm run typecheck` |
| tests (vitest + testing-library) | `npm test -- --run` |
| production build | `npm run build` |

Env knobs (build-time, Vite):

- `VITE_TRANSPORT` = `mock` (default) | `http` | `wasm`
- `VITE_API_BASE` — base URL for `http`; defaults to same origin
  (POSTs to `{base}/api/dispatch`)
- `VITE_DATA_URL` — registry snapshot URL the `wasm` transport fetches
  (required for `wasm`)
- `VITE_DATA_FORMAT` — snapshot format for `wasm`: `csv` (default) |
  `jsonl`
- `VITE_REGISTRY_NAME` — registry display name for `wasm`; defaults to
  the data URL

## The transport contract

```ts
type Transport = (req: Request) => Promise<Response>;
```

`Request`/`Response` (in `src/protocol/`) mirror the Rust `crates/app`
serde enums exactly: requests are internally tagged with `"op"`
(`Resolve | List | Count | Describe | Create | Edit | Transition |
Print | Export | PollProposal | Whoami`), responses are the
`{ok: true, data} | {ok: false, error: {kind, message}}` envelope with
`kind ∈ NotFound | Ambiguous | Validation | Unsupported | Auth |
Backend | BadRequest`. Transports never throw for domain failures —
those come back in the envelope; a throw means the transport itself is
broken or misconfigured.

Implementations (`src/transport/`):

- **`mockTransport(fixtures?)`** — a faithful in-memory double of the
  engine's dispatch (resolve normalization + prefix matching, the
  `Unsupported` collection guard, lifecycle rules, mint-then-bind,
  Print/Export/PollProposal/Whoami) over a fixture entity store (not
  canned responses). Default backend in dev and the backbone of the
  tests. Ships with a small parts fixture set (`fixtures.ts`). Its
  Print renders a placeholder SVG (rect + id text); real label
  rendering is the Rust codec's.
- **`httpTransport(baseUrl)`** — `POST {base}/api/dispatch`, JSON.
  Accepts any well-formed envelope regardless of HTTP status; anything
  else maps to a `Backend` error.
- **`wasmTransport(env?)`** — the serverless deploy: imports the
  wasm-pack pkg (`src/wasm-pkg/`, gitignored — built by
  `npm run build:wasm`), fetches the snapshot from `VITE_DATA_URL`,
  opens it via `registry_open`, then round-trips Request/Response JSON
  through `registry_dispatch`. The pkg is loaded via
  `import.meta.glob`, so typecheck and `vite build` succeed without it
  and its absence surfaces as a clear runtime error pointing at
  `npm run build:wasm`. Honest capabilities (crates/wasm dispatch.rs):
  reads are fully served; mutations answer `Auth` until an operator is
  set and `Backend` (OAuth + PR wiring pending) at the proposal sink.
- A Tauri `invoke` transport joins the same seam later (ADR-030 §3).

## Descriptor-driven UI (ADR-035 §0)

Descriptors from `Describe` carry **all** display metadata; components
hardcode **zero** field names or labels. Grid columns, detail rows,
status filters, the count strip, entity titles (`render.label_fields`)
and the registry name are all generated from the `Describe` payload.
The dividing line, applied throughout:

- **Domain strings** (collection names, field keys/labels, status
  tokens) — always from descriptors/entity data.
- **Micro-core keys** (`id`, `status`, `created_at`,
  `transitioned_at`) — protocol identifiers every entity has; rendered
  as their literal key, never given an invented display label.
- **UI chrome** (pager arrows, filter placeholder, error text) — the
  shell's own, allowed.

Routing is a deliberately tiny hash router (`src/router.ts`): `#/` is
the grid, `#/print` the print page, `#/bind` the bind queue, `#/<id>`
resolves an entity. A handful of routes don't justify a dependency,
and hash routing keeps GitHub Pages deploys config-free.

## Pages beyond the grid

- **`#/print`** — select entities (pasted ids or the shared filter
  grammar), set options (layout/size/chars/micro/copies — values from
  the protocol vocabularies), dispatch `Print`, preview the returned
  SVGs, and print via a child window with one margin-0 `@page` per
  label (continuous-roll model, ported from the old `dk-continuous`
  output mode). Die-cut sheet packing is deliberately out of scope.
- **`#/bind`** — look up an id (`Resolve`), fill the descriptor's
  editable fields, queue locally (`localStorage`,
  `webapp.bind-queue`), submit as sequential
  `Transition{to: "bound", fields}`. Failed items stay queued with the
  protocol error shown verbatim — on the wasm transport that is the
  honest `Auth` (no operator) or `Backend` (OAuth + PR wiring pending)
  answer.

## Deliberately absent (ported later, per ADR-030)

Worthwhile features of the old `web/` arrive as thin renders over the
protocol once the Rust side is in place — they are **not** missing by
accident:

- **camera scan / lookup-by-QR** (Tauri mobile gets the native scan
  plugin)
- **die-cut label sheets** (the print page targets continuous tape
  only)
- mint/edit/void screens (`Create`/`Edit`/`Transition{void}` are
  implemented in the protocol layer and mock, exercised by tests, but
  have no screens yet)
- Tauri `invoke` transport
- auth/identity surfaces (`Whoami` is plumbed; no sign-in UI yet, so
  the wasm transport never calls `registry_set_operator`)

## Protocol notes

The Rust engine (`crates/app/src/engine.rs`) is authoritative; the
mock mirrors its response shapes:

- Mutations are **proposals** (ADR-019): the registry does not change
  until the proposal lands, so mutation responses carry a
  `ProposalRef` (`{url, local_id, adapter}`), never the updated
  entity. Authoritative shapes (typed in `src/protocol/types.ts`):
  - `Create` → `{minted: ["<id>", …], created_at: "<RFC3339>", proposal}`
  - `Edit` → `{id, proposal}`
  - `Transition` → `{id, to, proposal}`

  The mock applies the mutation to its in-memory store immediately
  (dev convenience — the real engine defers application to
  proposal-merge) but returns these shapes with a fake
  `{url: "mock://proposal/<n>", local_id: "<n>", adapter: "mock"}` ref.
- `Describe{collection}` returns the same `{name, collections}`
  envelope narrowed to that collection (matches the engine); unknown
  collection → `NotFound`.
- Mock-local choices still pending engine alignment: unknown
  collection on other ops → `BadRequest` (engine: `Unsupported`);
  `filter.fields` is exact match per key (engine: case-insensitive
  substring); `Whoami` returns a placeholder and the UI does not
  consume it.
