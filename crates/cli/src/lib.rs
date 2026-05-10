//! `part-registry-cli` — wiring crate for the `mint`, `label`, `bind`
//! binaries per ADR-017. Adapter selection per ADR-021's
//! `PART_REGISTRY_*` env vars happens here so domain crates never
//! match on adapter strings (ADR-027 §Tier 4 drift discipline).
//!
//! Foundation scaffold — binaries print a standard "not yet
//! implemented" message and exit non-zero.

#![forbid(unsafe_code)]
