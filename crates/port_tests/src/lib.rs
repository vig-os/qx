//! `part-registry-port-tests` — generic conformance + parity +
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

use part_registry_identity::IdentityProvider;
use part_registry_signing::SigningProvider;
use part_registry_storage::Repository;
use part_registry_transport::ProposalSink;

// -------------------------------------------------------------------
// Tier 1 — trait conformance
// -------------------------------------------------------------------

/// ADR-027 §Tier 1 — generic `Repository` conformance suite.
/// Body is a placeholder; assertions land alongside ADR-018 step 4.
pub fn repository_conformance<R: Repository>(_repo: R) {
    // TODO(foundation): roundtrip Part, AuditEntry, PrintEvent.
    // TODO(foundation): error coverage per ADR-018 trait surface.
    // TODO(foundation): sort-stability invariant.
}

/// ADR-027 §Tier 1 — generic `SigningProvider` conformance suite.
pub fn signing_provider_conformance<S: SigningProvider>(_provider: S) {
    // TODO(foundation): sign a synthetic payload, confirm the
    // returned Signature variant matches the algorithm() value.
}

/// ADR-027 §Tier 1 — generic `IdentityProvider` conformance suite.
pub fn identity_provider_conformance<I: IdentityProvider>(_provider: I) {
    // TODO(foundation): current() returns a well-formed Operator;
    // refresh() is idempotent for unchanged state; verified_at
    // semantics match the adapter's documented contract.
}

/// ADR-027 §Tier 1 — generic `ProposalSink` conformance suite.
pub fn proposal_sink_conformance<T: ProposalSink>(_sink: T) {
    // TODO(foundation): submit returns a parseable ProposalRef;
    // status round-trips a known ref.
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
