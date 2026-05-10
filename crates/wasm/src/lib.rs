//! `part-registry-wasm` — `wasm-bindgen` façade over `codec`,
//! `validators`, and the policy engine per ADR-017 strangler-fig
//! step 8. Consumed by `web/src/` once the inline TS encoder
//! (`web/src/layouts/qrcode-generator.ts`) is retired.
//!
//! Foundation scaffold; bodies are placeholders so the WASM target
//! compiles cleanly under `cargo build --target wasm32-unknown-unknown`.

#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;

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
