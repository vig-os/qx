//! `qx-app` — the application layer per ADR-030/035.
//!
//! One serde command protocol ([`Request`] → [`Response`]) over a
//! generic collection engine; every shell (CLI, TUI, serve, MCP,
//! web/WASM, Tauri) calls [`dispatch`] and depends on this crate only —
//! never on adapter crates (ADR-030 architectural invariant).
//!
//! - [`protocol`] — the wire types (one parameterized op family).
//! - [`entity`] — the ADR-035 micro-core render of stored records.
//! - [`preset`] — code-owned collection descriptors (the regulated
//!   floor; `Describe` output).
//! - [`engine`] — [`AppContext`] + [`dispatch`] + the handlers.

#![forbid(unsafe_code)]

pub mod engine;
pub mod entity;
pub mod preset;
pub mod protocol;

pub use engine::{dispatch, mint_part_id, AppContext, HUMAN_PREFIX_LEN};
pub use entity::{part_to_entity, Entity};
pub use preset::{parts_descriptor, registry_descriptor, CollectionDescriptor, RegistryDescriptor};
pub use protocol::{
    ErrorBody, ErrorKind, Filter, PaddingSpec, Page, PrintOptions, Request, Response, Selection,
    Sort, SortDir,
};

#[cfg(test)]
mod tests;
