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
| typecheck (TS strict) | `npm run typecheck` |
| tests (vitest + testing-library) | `npm test -- --run` |
| production build | `npm run build` |

Env knobs (build-time, Vite):

- `VITE_TRANSPORT` = `mock` (default) | `http` | `wasm`
- `VITE_API_BASE` — base URL for `http`; defaults to same origin
  (POSTs to `{base}/api/dispatch`)

## The transport contract

```ts
type Transport = (req: Request) => Promise<Response>;
```

`Request`/`Response` (in `src/protocol/`) mirror the Rust `crates/app`
serde enums exactly: requests are internally tagged with `"op"`
(`Resolve | List | Count | Describe | Create | Edit | Transition |
Whoami`), responses are the `{ok: true, data} | {ok: false, error:
{kind, message}}` envelope with `kind ∈ NotFound | Validation |
Unsupported | Auth | Backend | BadRequest`. Transports never throw for
domain failures — those come back in the envelope; a throw means the
transport itself is broken or misconfigured.

Implementations (`src/transport/`):

- **`mockTransport(fixtures?)`** — real in-memory
  filter/sort/page/count/describe/resolve/create/edit/transition over a
  fixture entity store (not canned responses). Default backend in dev
  and the backbone of the tests. Ships with a small parts fixture set
  (`fixtures.ts`).
- **`httpTransport(baseUrl)`** — `POST {base}/api/dispatch`, JSON.
  Accepts any well-formed envelope regardless of HTTP status; anything
  else maps to a `Backend` error.
- **`wasmTransport()`** — documented integration point; throws
  `"wasm transport: crates/wasm dispatch not built yet — see ADR-030 §3"`
  until the wasm-bindgen façade over `app::dispatch` exists. The
  intended wiring is sketched in `src/transport/wasm.ts`.
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
the grid, `#/<id>` resolves an entity. Two routes don't justify a
dependency, and hash routing keeps GitHub Pages deploys config-free.

## Deliberately absent (ported later, per ADR-030)

Worthwhile features of the old `web/` arrive as thin renders over the
protocol once the Rust side is in place — they are **not** missing by
accident:

- **camera scan / lookup-by-QR** (Tauri mobile gets the native scan
  plugin)
- **label print pipeline** (layout/size/copies — print events fold into
  the audit spine, ADR-035 §0)
- **bind queue / bulk bind flow**
- mutation UI generally (`Create`/`Edit`/`Transition` are implemented
  in the protocol layer and mock, exercised by tests, but have no
  screens yet)
- Tauri `invoke` transport; WASM transport wiring (see above)
- auth/identity surfaces (`Whoami` is plumbed but unused)

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
