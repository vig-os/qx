//! Meta-schema gate (ADR-039 §3 — M-A.1). Every canonical contract that
//! ships in the repo — the `qx init` preset AND the example fixture — must
//! validate against the hand-mirrored `schema/contract.schema.json` AND
//! parse via the Rust parser (the SSOT), so the editor tooling the schema
//! exists to serve can't silently drift from the engine. Runs in the
//! existing `test` flake check — no new derivation.

use std::path::PathBuf;

fn schema_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../schema")
        .join(rel)
}

fn load(rel: &str) -> serde_json::Value {
    let p = schema_path(rel);
    let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

/// Every canonical contract shipped in the repo: the example fixture and
/// the `qx init` preset. The meta-schema gate must hold for all of them.
const SHIPPED_CONTRACTS: &[&str] = &[
    "contract.example.json",
    "presets/company.contract.json",
    "presets/personas.contract.json",
];

#[test]
fn shipped_contracts_validate_against_meta_schema() {
    let meta = load("contract.schema.json");
    let compiled = jsonschema::JSONSchema::compile(&meta)
        .expect("contract.schema.json must itself be a valid JSON Schema");
    for rel in SHIPPED_CONTRACTS {
        let doc = load(rel);
        // Collect owned messages immediately so the borrowing error-iterator
        // is consumed before `compiled`/`doc` go out of scope.
        let msgs: Vec<String> = match compiled.validate(&doc) {
            Ok(()) => Vec::new(),
            Err(errors) => errors.map(|e| e.to_string()).collect(),
        };
        assert!(
            msgs.is_empty(),
            "{rel} fails contract.schema.json (schema drift?):\n  {}",
            msgs.join("\n  ")
        );
    }
}

#[test]
fn shipped_contracts_parse_via_rust_ssot() {
    // SSOT parity: the Rust authority accepts the same docs the meta-schema
    // does. Divergence here = FE (schema) and gate (Rust) disagreeing on
    // validity — the SSOT split class S1 closed for the `closed` facet.
    for rel in SHIPPED_CONTRACTS {
        let bytes = std::fs::read(schema_path(rel)).unwrap();
        qx_contract::Contract::from_bytes(&bytes)
            .unwrap_or_else(|e| panic!("{rel} must parse via the Rust SSOT: {e}"));
    }
}
