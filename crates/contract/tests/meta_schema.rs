//! Meta-schema gate (ADR-039 §3 — M-A.1 S2). The hand-mirrored
//! `schema/contract.schema.json` and the shipped `contract.example.json`
//! must agree with each other AND with the Rust parser (the SSOT), so the
//! editor tooling the schema exists to serve can't silently drift from the
//! engine. Runs in the existing `test` flake check — no new derivation.

use std::path::PathBuf;

fn load(name: &str) -> serde_json::Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../schema")
        .join(name);
    let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

#[test]
fn example_validates_against_meta_schema() {
    let meta = load("contract.schema.json");
    let example = load("contract.example.json");
    let compiled = jsonschema::JSONSchema::compile(&meta)
        .expect("contract.schema.json must itself be a valid JSON Schema");
    // Collect owned messages immediately so the borrowing error-iterator is
    // fully consumed before `compiled`/`example` go out of scope.
    let msgs: Vec<String> = match compiled.validate(&example) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.map(|e| e.to_string()).collect(),
    };
    assert!(
        msgs.is_empty(),
        "contract.example.json fails contract.schema.json (schema drift?):\n  {}",
        msgs.join("\n  ")
    );
}

#[test]
fn example_parses_via_rust_ssot() {
    // Example ↔ Rust parser parity: the authority accepts the same doc the
    // meta-schema does. If these two diverge, the FE (schema) and the gate
    // (Rust) would disagree about validity — exactly the SSOT split S1 fixed.
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema/contract.example.json");
    let bytes = std::fs::read(&p).unwrap();
    qx_contract::Contract::from_bytes(&bytes)
        .expect("contract.example.json must parse via the Rust SSOT");
}
