//! `part-registry-wasm` — `wasm-bindgen` façade over `codec`,
//! `validators`, and the policy engine per ADR-017 strangler-fig
//! step 8. Consumed by `web/src/` once the inline TS encoder
//! (`web/src/layouts/qrcode-generator.ts`) is retired.
//!
//! ## Observability (ADR-022)
//!
//! `wasm_init()` is the WASM-side entry point that mints a
//! [`RequestId`] for one FE-initiated user action and opens a tracing
//! span so any subsequent emits inside the WASM module inherit it via
//! `tracing` span context. The actual subscriber install on wasm32 is
//! a no-op today: the `tracing-subscriber` `fmt` layer assumes
//! `std::io::stderr`/`std::io::stdout` which behave differently in
//! browsers, and a proper `tracing-web` shim is out of scope for the
//! foundation (issues #32/#33 land the FE-WASM bridge). The FE
//! conducts its own browser-side logging today and propagates the
//! `request_id` returned here through the ProposalSink payload
//! (ADR-019) per ADR-022 §"request_id propagation".
//!
//! Foundation scaffold; bodies are placeholders so the WASM target
//! compiles cleanly under `cargo build --target wasm32-unknown-unknown`.

#![forbid(unsafe_code)]

use part_registry_domain::RequestId;
use wasm_bindgen::prelude::*;

/// Mint a fresh UUIDv7 request id for an FE-initiated action.
///
/// Per ADR-022 §"request_id propagation" the FE generates one ID per
/// user-action root (click, scan, open-proposal) and attaches it to
/// the proposal payload + telemetry. The string is returned in
/// hyphenated lowercase form ready to embed in PR body / fetch
/// headers.
#[wasm_bindgen]
pub fn wasm_request_id_new() -> String {
    RequestId::new().to_string()
}

/// One-shot init for the WASM façade. Today a no-op: see module docs
/// for why a `tracing-web` shim is deferred. Idempotent; safe to call
/// from any JS-side entry point.
#[wasm_bindgen]
pub fn wasm_init() {
    // intentionally empty — production-side wiring deferred to #32/#33
}

#[wasm_bindgen]
pub fn render_label(_canonical: &str, _layout: &str, _size_mm: f64, _format: &str) -> String {
    // ADR-017 step 8 — wires through `part_registry_codec::render_label`.
    String::from("<svg><!-- foundation scaffold; not yet implemented --></svg>")
}

#[wasm_bindgen]
pub fn validate_diff(_diff_json: &str) -> JsValue {
    // ADR-017 step 8 — wires through validators::validate_*.
    JsValue::from_str("foundation scaffold; not yet implemented")
}

#[wasm_bindgen]
pub fn classify_diff(_diff_json: &str) -> JsValue {
    // ADR-017 step 8 — wires through validators::classify_diff
    // for FE preflight per ADR-019.
    JsValue::from_str("foundation scaffold; not yet implemented")
}
