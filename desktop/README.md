# desktop — the Tauri v2 shell (ADR-030 §3)

The desktop member of the **Webview** bundle family: the same Vite +
React webapp (`webapp/`), loaded into a Tauri v2 window, with its
transport calling the Rust app layer **in-process** — `invoke
("dispatch")` lands on the one Tauri command in
`src-tauri/src/lib.rs`, which runs `qx_app::dispatch`
directly. No HTTP hop, no second server.

Wiring matches `qx serve` (`crates/cli/src/bin/qx.rs`): adapters come
from `Wiring::from_config` over the `PART_REGISTRY_*` env config
(ADR-021). The live GitHub PR sink is used when a token resolves
(`PART_REGISTRY__TRANSPORT__GITHUB_TOKEN`, `GITHUB_TOKEN`, or
`GH_TOKEN`); otherwise mutations are captured as dry-run JSON on
stdout and reads still work token-free.

## Dev

Two ways to iterate on the UI:

- **Against `qx serve` (no Tauri in the loop)** — fastest for pure UI
  work; the http transport speaks the identical protocol:

  ```sh
  cargo run -p qx-cli --features serve --bin qx -- serve
  cd webapp && VITE_TRANSPORT=http VITE_API_BASE=http://localhost:8470 npm run dev
  ```

- **Real desktop shell** — start the Vite dev server with the tauri
  transport, then launch the Tauri dev window (it loads `devUrl`,
  `http://localhost:5173`):

  ```sh
  cd webapp && VITE_TRANSPORT=tauri npm run dev
  ```

  and in a second terminal (needs `tauri-cli`, e.g.
  `cargo install tauri-cli --version '^2'`):

  ```sh
  cd desktop/src-tauri && cargo tauri dev
  ```

  Without `tauri-cli`, `cargo run -p qx-desktop` also works
  in debug — the debug build points at `devUrl`, so the Vite dev
  server must be running.

## Build

```sh
cd webapp && VITE_TRANSPORT=tauri npm run build   # produces webapp/dist
cd desktop/src-tauri && cargo tauri build         # release binary
```

The release build embeds `webapp/dist` at compile time, so the webapp
build must run first. `cargo check -p qx-desktop` is the CI
gate for this crate; full bundling needs platform tooling that CI does
not require.

## Deferred

- **Bundling + icons** — `bundle.active` is `false` in
  `tauri.conf.json`. The committed `icons/icon.png` is a minimal
  generated 128×128 RGBA tile — just enough for Tauri's codegen, which
  requires it for the default window icon. To enable installers,
  generate the real multi-size set from project artwork
  (`cargo tauri icon path/to/source.png`), list it under `bundle.icon`,
  and flip `bundle.active` to `true`. Until then `cargo tauri build`
  produces the plain binary only.
- **Mobile (Tauri v2 iOS/Android)** — the lib entry point is already
  split out of `main.rs` for it, but no mobile targets, signing, or
  per-platform config are wired.
- **Updater / code signing / notarization** — release-channel
  concerns, out of scope for the scaffold.
- **Capability hardening** — `capabilities/default.json` grants
  `core:default` only; revisit when the UI starts using plugins
  (dialog, shell, deep links, …).
