//! The conformance corpus runner — NATIVE arm of the ADR-039 §4 parity
//! triplet (native Rust / wasm / FE). Loads `conformance/contract.json` +
//! `conformance/cases.json` and drives every case through the SSOT
//! [`validate_record`]. Each case's `expect` is the COMPLETE issue set:
//! an unmatched expectation OR an unexpected issue fails the case.
//!
//! The same corpus is the fixture for the wasm + FE runners, so passing
//! here is one third of the cross-surface guarantee; the wasm-clean CI
//! build (flake `wasm-clean` check) proves the same code compiles to
//! wasm32, and the FE runner (with the FE migration) closes the triangle.

#![allow(clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};

use part_registry_contract::Contract;
use part_registry_validators::record::{validate_record, RecordContext, Severity};
use serde_json::Value;

fn conformance_dir() -> String {
    format!("{}/../../conformance", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn conformance_contract_is_valid() {
    let bytes =
        std::fs::read(format!("{}/contract.json", conformance_dir())).expect("read contract");
    Contract::from_bytes(&bytes).expect("conformance contract must be structurally valid");
}

#[test]
fn conformance_corpus_native_runner() {
    let dir = conformance_dir();
    let contract = Contract::from_bytes(&std::fs::read(format!("{dir}/contract.json")).unwrap())
        .expect("conformance contract valid");
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/cases.json")).unwrap())
            .expect("cases.json parses");
    let cases = doc["cases"].as_array().expect("cases is an array");
    assert!(!cases.is_empty(), "corpus must not be empty");

    let mut failures: Vec<String> = Vec::new();
    for case in cases {
        let name = case["name"].as_str().expect("case name");
        let coll_name = case["collection"].as_str().expect("case collection");
        let collection = contract
            .collection(coll_name)
            .unwrap_or_else(|| panic!("case `{name}`: unknown collection `{coll_name}`"));
        let status = case.get("status").and_then(Value::as_str);

        let mut universe: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        if let Some(known) = case.get("known_ids").and_then(Value::as_object) {
            for (coll, ids) in known {
                let set = ids
                    .as_array()
                    .expect("known_ids values are arrays")
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
                universe.insert(coll.clone(), set);
            }
        }
        let ctx = RecordContext::new(universe);
        let record = case["record"]
            .as_object()
            .expect("case record is an object");

        let issues = validate_record(collection, record, status, &ctx);

        // Match each expectation to exactly one issue; nothing left over.
        let mut remaining: Vec<&_> = issues.iter().collect();
        let mut unmatched: Vec<String> = Vec::new();
        for e in case["expect"].as_array().expect("expect is an array") {
            let ep = e["path"].as_str().unwrap();
            let es = e["severity"].as_str().unwrap();
            let ec = e["contains"].as_str().unwrap();
            let sev = match es {
                "error" => Severity::Error,
                "warn" => Severity::Warn,
                other => panic!("case `{name}`: bad severity `{other}`"),
            };
            match remaining
                .iter()
                .position(|i| i.path == ep && i.severity == sev && i.message.contains(ec))
            {
                Some(pos) => {
                    remaining.remove(pos);
                }
                None => unmatched.push(format!("{ep} / {es} / contains \"{ec}\"")),
            }
        }

        if !unmatched.is_empty() || !remaining.is_empty() {
            let extra: Vec<String> = remaining
                .iter()
                .map(|i| format!("{} / {:?} / \"{}\"", i.path, i.severity, i.message))
                .collect();
            failures.push(format!(
                "case `{name}`:\n  unmatched expectations: {unmatched:?}\n  unexpected issues: {extra:?}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "conformance corpus failures ({} of {} cases):\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
