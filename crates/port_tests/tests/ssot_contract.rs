//! SSoT enforcement: verify Rust constants match `schema/registry-contract.json`.
//!
//! This test ensures the Rust domain and validators crates stay in
//! sync with the shared contract JSON that also drives the FE, Python
//! tooling, and Playwright tests. A drift here means a schema change
//! was made in one surface but not the other.

use part_registry_domain::{PART_ID_ALPHABET, PART_ID_LEN};
use part_registry_validators::REGISTRY_HEADER;

/// Minimal contract shape — only the fields we need to assert against.
#[derive(serde::Deserialize)]
struct Contract {
    id: IdContract,
    fields: Vec<FieldContract>,
}

#[derive(serde::Deserialize)]
struct IdContract {
    alphabet: String,
    #[serde(rename = "canonicalLength")]
    canonical_length: usize,
}

#[derive(serde::Deserialize)]
struct FieldContract {
    key: String,
}

fn load_contract() -> Contract {
    let json = include_str!("../../../schema/registry-contract.json");
    serde_json::from_str(json).expect("registry-contract.json must parse")
}

#[test]
fn part_id_alphabet_matches_contract() {
    let contract = load_contract();
    assert_eq!(
        PART_ID_ALPHABET, contract.id.alphabet,
        "PART_ID_ALPHABET in crates/domain must match contract.id.alphabet"
    );
}

#[test]
fn part_id_len_matches_contract() {
    let contract = load_contract();
    assert_eq!(
        PART_ID_LEN, contract.id.canonical_length,
        "PART_ID_LEN in crates/domain must match contract.id.canonicalLength"
    );
}

#[test]
fn registry_header_matches_contract_fields_in_order() {
    let contract = load_contract();
    let contract_keys: Vec<&str> = contract.fields.iter().map(|f| f.key.as_str()).collect();

    assert_eq!(
        REGISTRY_HEADER.len(),
        contract_keys.len(),
        "REGISTRY_HEADER column count must match contract.fields count"
    );

    for (i, (rust_key, json_key)) in REGISTRY_HEADER.iter().zip(contract_keys.iter()).enumerate() {
        assert_eq!(
            rust_key, json_key,
            "REGISTRY_HEADER[{i}] = {rust_key:?} but contract.fields[{i}].key = {json_key:?}"
        );
    }
}
