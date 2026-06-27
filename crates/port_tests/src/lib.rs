//! `qx-port-tests` — generic conformance + parity +
//! drift-detection framework per ADR-027.
//!
//! Adapter crates wire their concrete adapter into the generic
//! conformance functions from their own `tests/` directory so each
//! adapter's test binary fails independently when the contract is
//! violated.
//!
//! Foundation scaffold — function bodies are intentionally empty so
//! adapter-side `tests/conformance.rs` files can be wired today
//! without producing a flood of unrelated assertion failures while
//! the trait surfaces stabilise.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use qx_domain::{PartFilter, PartId, Proposal, ProposalStatus};
use qx_identity::IdentityProvider;
use qx_signing::SigningProvider;
use qx_storage::{AuditFilter, Repository};
use qx_transport::ProposalSink;

/// Canonical registry state for cross-adapter parity: `id -> {column ->
/// value}`. Substrate-agnostic — a CSV adapter and a relational adapter
/// both project to this.
pub type RegistryState = BTreeMap<String, BTreeMap<String, String>>;

// -------------------------------------------------------------------
// Tier 1 — trait conformance
// -------------------------------------------------------------------

/// ADR-027 §Tier 1 — generic `Repository` conformance suite.
///
/// The invariants every storage adapter must honour, asserted against a
/// seeded repo the adapter provides (ADR-018 trait surface, ADR-035 §4):
/// sort-stability, read determinism, Part roundtrip, unknown→None, and
/// the read methods answering without error.
pub fn repository_conformance<R: Repository>(repo: R) {
    // list_parts is deterministic and sorted by id (ADR-035 §4).
    let parts = repo
        .list_parts(&PartFilter::default())
        .expect("list_parts answers");
    let ids: Vec<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted, "list_parts must be sorted by id (ADR-035 §4)");
    let again = repo
        .list_parts(&PartFilter::default())
        .expect("list_parts is deterministic");
    assert_eq!(
        ids,
        again.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        "two list_parts calls must agree"
    );

    // A listed id roundtrips through get_part; an absent id is None.
    if let Some(first) = parts.first() {
        let got = repo
            .get_part(&first.id)
            .expect("get_part answers")
            .expect("a listed id must be present");
        assert_eq!(got.id, first.id, "get_part roundtrips the Part");
    }
    let absent = PartId::new("ZZZZZZZZZZZZZZ").expect("valid nano14 shape");
    assert!(
        repo.get_part(&absent).expect("get_part answers").is_none(),
        "an unknown id resolves to None, not an error"
    );

    // The audit + generic-collection read paths answer without error; an
    // undeclared collection is empty (not an error).
    repo.list_audit_events(&AuditFilter::default())
        .expect("list_audit_events answers");
    let undeclared = repo
        .list_collection("definitely-not-a-declared-collection")
        .expect("list_collection answers");
    assert!(
        undeclared.is_empty(),
        "an undeclared collection reads as empty"
    );
}

/// ADR-027 §Tier 1 — generic `SigningProvider` conformance suite.
pub fn signing_provider_conformance<S: SigningProvider>(provider: S) {
    // The declared algorithm is stable across calls — a provider that
    // reports a different SigAlgorithm each call is broken (ADR-024
    // surface). The adapter's own suite exercises sign() concretely.
    assert_eq!(
        provider.algorithm(),
        provider.algorithm(),
        "algorithm() must be stable"
    );
}

/// ADR-027 §Tier 1 — generic `IdentityProvider` conformance suite.
pub fn identity_provider_conformance<I: IdentityProvider>(provider: I) {
    // When current() resolves an operator, its id must be well-formed
    // (non-empty). Providers differ in whether they can resolve without
    // external state (a token, git config), so an Err is acceptable —
    // the adapter's own suite drives the resolved path.
    if let Ok(op) = provider.current() {
        assert!(
            !op.id.0.is_empty(),
            "a resolved operator id must be non-empty"
        );
    }
}

/// ADR-027 §Tier 1 — generic `ProposalSink` conformance suite.
///
/// Spike #189: was an empty stub, so every substrate-independence claim
/// in ADR-019/027 was unfalsified. The contract clauses asserted here:
///
/// - `submit` of a well-formed `Proposal` succeeds and returns a
///   `ProposalRef` whose `url` is non-empty (ADR-022 audit citation form)
///   and whose `adapter` names the backend (ADR-019 multi-adapter routing).
/// - `status` of a freshly-submitted ref succeeds (does not error) and
///   resolves to a known `ProposalStatus` variant.
///
/// The trait's *structural* no-self-merge property (no `merge`/`close`)
/// is enforced by the trait surface itself (ADR-019) — there is no method
/// to assert against, which is the point.
pub fn proposal_sink_conformance<T: ProposalSink>(sink: &T, sample: Proposal) {
    let pref = sink
        .submit(sample)
        .expect("ADR-019: submit must succeed for a well-formed proposal");
    assert!(
        !pref.url.is_empty(),
        "ADR-022: ProposalRef.url must be non-empty (audit citation form)"
    );
    assert!(
        !pref.adapter.is_empty(),
        "ADR-019: ProposalRef.adapter must identify the backend"
    );
    let status = sink
        .status(&pref)
        .expect("ADR-019: status(submitted ref) must succeed");
    // Any variant is contractually acceptable; the clause is that a
    // freshly-submitted ref resolves to *some* known status, exhaustively.
    match status {
        ProposalStatus::Open
        | ProposalStatus::Merged
        | ProposalStatus::Closed
        | ProposalStatus::RequiresReview
        | ProposalStatus::BlockedByPolicy { .. }
        | ProposalStatus::Errored { .. } => {}
    }
}

/// ADR-027 §Tier 3 — cross-adapter `ProposalSink` **parity**, re-scoped
/// per the spike #189 distributed-data review.
///
/// The original ADR-027 wording — "same `Proposal` → same acceptance
/// *trace*" — is unachievable across substrates whose merge models differ
/// (git line-merge vs. a relational cell-merge): conflict surfaces, merge
/// granularity, and rejection timing are intrinsically substrate-visible.
/// Asserting full-trace parity would fail the moment a second substrate is
/// real and be misread as an adapter bug.
///
/// Parity is therefore scoped to **post-merge state equivalence on
/// cleanly-applying inputs**: given the same base state and the same
/// single-proposal diff, both adapters reach the same *logical row set*
/// (empty cells normalized away), and they **agree on accept-vs-reject**.
/// Header changes, conflict representation, and error *text* are declared
/// substrate-visible and out of the parity contract.
///
/// Each proposal is applied independently from `base` (matching a gate's
/// stateless-per-submit-against-ground-truth semantics); `apply_*` close
/// over a real `ProposalSink` and return its resulting [`RegistryState`].
pub fn proposal_sink_parity<FA, FB>(
    base: &RegistryState,
    corpus: &[Proposal],
    mut apply_a: FA,
    mut apply_b: FB,
) where
    FA: FnMut(&RegistryState, &Proposal) -> Result<RegistryState, String>,
    FB: FnMut(&RegistryState, &Proposal) -> Result<RegistryState, String>,
{
    for (i, p) in corpus.iter().enumerate() {
        let a = apply_a(base, p);
        let b = apply_b(base, p);
        match (&a, &b) {
            (Ok(sa), Ok(sb)) => assert_eq!(
                normalize_state(sa),
                normalize_state(sb),
                "ADR-027 parity: adapters disagree on post-merge state for proposal {i}"
            ),
            // Agreement on rejection is a parity property; the error
            // *text* is substrate-visible and not compared.
            (Err(_), Err(_)) => {}
            _ => panic!(
                "ADR-027 parity: adapters disagree on accept/reject for \
                 proposal {i}: a={a:?} b={b:?}"
            ),
        }
    }
}

/// Drop empty-valued cells so a rectangular CSV substrate (every header
/// column present, missing ones empty) compares equal to a sparse
/// relational substrate on the *logical* row set.
fn normalize_state(s: &RegistryState) -> RegistryState {
    s.iter()
        .map(|(id, fields)| {
            let f = fields
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            (id.clone(), f)
        })
        .collect()
}

/// ADR-027 §Tier 1 — pure-function `validators` conformance hook.
///
/// Validators are pure (no trait surface to mock) so this entry
/// point exists for parity with the trait-based conformance helpers
/// above and as the future home of fixture-driven assertions that
/// run identically against native + wasm32 builds per
/// ADR-016 §"FE preflight runs the same engine".
pub fn validator_conformance() {
    // TODO(foundation Tier 1): once shared fixture rows live in
    // tests/fixtures/, exercise schema + FK + sort-stability + policy
    // engine against them here so each adapter crate doesn't roll its
    // own fixture loader.
}

/// ADR-027 §Tier 3 — codec roundtrip parity hook.
///
/// Stub for the forthcoming cross-encoder/decoder parity suite (lands
/// alongside ADR-017 step 8's A/B vs `zxing-wasm`, tracked in
/// issues #27/#33). The body is intentionally empty for the foundation
/// PR: the codec crate ships its own roundtrip tests in
/// `crates/codec/src/svg.rs::tests`, so until a second encoder or
/// decoder adapter exists there is nothing to compare against.
///
/// Signatures are pinned so adapter-pair PRs can wire in without
/// changing this surface.
pub fn codec_roundtrip_conformance<E, D>(_encode: E, _decode: D)
where
    E: Fn(&str, bool) -> Result<Vec<bool>, String>,
    D: Fn(&[u8]) -> Result<String, String>,
{
    // TODO(ADR-027 Tier 3): once a second encoder/decoder adapter
    // lands, exercise the fixed canonical corpus
    // (`K7M3PQ9RT5VAXY`, plus generated nanoid IDs from the ADR-012
    // alphabet) through both adapter pairs and assert
    // `decode(encode(p)) == p` for every payload.
}

// -------------------------------------------------------------------
// Tier 2 — forward-shape tests (ADR-027 §Tier 2)
// -------------------------------------------------------------------

/// Forward-shape: round-trip an `AuditEntry` carrying a synthetic
/// Sigstore-shaped `Signature` and a populated `chain_hash`. MVP code
/// paths never produce this, but every storage adapter must round-trip
/// it byte-for-byte so activating Sigstore later (ADR-024) is an
/// adapter swap, not a schema migration.
pub fn audit_entry_roundtrips_sigstore_shape<R: Repository>(_repo: R) {
    // TODO(foundation): build a Sigstore-shaped AuditEntry via
    // synth_sigstore_*() helpers, append, list, assert equality.
}

// -------------------------------------------------------------------
// Tier 4 — drift-detection (lint-as-test)
// -------------------------------------------------------------------

/// Placeholder regex-based source scanner per ADR-027 §Tier 4.
/// Concrete walker lands once the workspace has enough source to
/// scan meaningfully (post-step-2 PRs).
#[cfg(test)]
mod drift {
    #[test]
    fn no_hardcoded_paths_in_source() {
        // TODO(foundation): walkdir over crates/{domain,codec,
        // validators,storage,identity,transport,signing}/src; reject
        // hardcoded paths per ADR-021.
    }

    #[test]
    fn no_println_outside_cli_main() {
        // TODO(foundation): walkdir over library crates; reject
        // print!/println!/eprint!/eprintln! per ADR-022.
    }
}
