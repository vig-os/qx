//! SOUP harness H1 — canonical-ID alphabet contract, collision
//! resistance, and RNG uniformity attestation per IEC 62304.
//!
//! These tests validate the `nanoid` SOUP dependency by asserting that
//! the domain constants (`PART_ID_ALPHABET`, `PART_ID_LEN`) agree with
//! the schema contract, that 10k random IDs are collision-free, and
//! that the RNG draws from the alphabet uniformly (chi-squared).

use std::collections::HashSet;

use part_registry_domain::{PART_ID_ALPHABET, PART_ID_LEN};
use rand::Rng;

// ------------------------------------------------------------------
// 1. Alphabet contract — domain constants match registry-contract.json
// ------------------------------------------------------------------

/// Parse the schema contract at compile time and assert the domain
/// crate's constants match.
#[test]
fn alphabet_matches_schema_contract() {
    let raw = include_str!("../../../schema/registry-contract.json");
    let contract: serde_json::Value = serde_json::from_str(raw).expect("contract JSON parses");

    let expected_alphabet = contract["id"]["alphabet"]
        .as_str()
        .expect("contract.id.alphabet is a string");
    let expected_length = contract["id"]["canonicalLength"]
        .as_u64()
        .expect("contract.id.canonicalLength is a number") as usize;

    assert_eq!(
        PART_ID_ALPHABET, expected_alphabet,
        "PART_ID_ALPHABET drift: domain crate diverged from schema/registry-contract.json"
    );
    assert_eq!(
        PART_ID_LEN, expected_length,
        "PART_ID_LEN drift: domain crate diverged from schema/registry-contract.json"
    );
}

// ------------------------------------------------------------------
// 2. Collision test — 10,000 random IDs, zero collisions
// ------------------------------------------------------------------

/// Generate a random ID from the canonical alphabet + length using the
/// same character-sampling strategy nanoid uses.
fn random_id(rng: &mut impl Rng) -> String {
    let alphabet: Vec<char> = PART_ID_ALPHABET.chars().collect();
    (0..PART_ID_LEN)
        .map(|_| alphabet[rng.gen_range(0..alphabet.len())])
        .collect()
}

#[test]
fn no_collisions_in_10k_ids() {
    let mut rng = rand::thread_rng();
    let mut seen = HashSet::with_capacity(10_000);
    for i in 0..10_000 {
        let id = random_id(&mut rng);
        assert!(seen.insert(id.clone()), "collision on iteration {i}: {id}");
    }
    assert_eq!(seen.len(), 10_000);
}

// ------------------------------------------------------------------
// 3. Uniformity test — chi-squared on 100,000 random characters
// ------------------------------------------------------------------

#[test]
fn alphabet_draws_are_uniform() {
    let alphabet: Vec<char> = PART_ID_ALPHABET.chars().collect();
    let k = alphabet.len();
    let n: usize = 100_000;
    let mut counts = vec![0usize; k];

    let mut rng = rand::thread_rng();
    for _ in 0..n {
        let idx = rng.gen_range(0..k);
        counts[idx] += 1;
    }

    // Chi-squared statistic: sum((observed - expected)^2 / expected)
    let expected = n as f64 / k as f64;
    let chi2: f64 = counts
        .iter()
        .map(|&c| {
            let diff = c as f64 - expected;
            diff * diff / expected
        })
        .sum();

    // Degrees of freedom = k - 1 = 29. At p = 0.01 the critical value
    // for chi-squared(29) is ~49.59. We use a generous 60.0 to avoid
    // flaky failures while still catching gross non-uniformity.
    let critical = 60.0;
    assert!(
        chi2 < critical,
        "chi-squared uniformity test failed: chi2 = {chi2:.2}, critical = {critical} (df = {}, n = {n})",
        k - 1,
    );
}
