//! `qx-transport-table` — an in-memory, **non-CSV**
//! `ProposalSink` (spike #189, delta D2).
//!
//! ADR-019 asserts and ADR-027 is meant to *test* substrate-independence
//! of the proposal gate, but no second `ProposalSink` adapter existed —
//! so the claim had never run through the parity suite. This crate is the
//! deliberately non-CSV partner: it stores the registry as a relational
//! row map (`id -> {column: value}`), not CSV text, so the ADR-027 parity
//! suite compares *semantic outcomes* rather than byte-identical CSV.
//!
//! ## Leaks surfaced while building this (the point of the exercise)
//!
//! Implementing `ProposalSink` over a non-CSV substrate forced three
//! CSV/git assumptions in the supposedly substrate-neutral contract into
//! the open. They are documented inline and tracked on issue #189:
//!
//! 1. **`ProposalRef.url` is mandatory** (`crates/domain` — `ProposalRef`).
//!    A table backend has no URL; it must mint a synthetic `table://`
//!    scheme purely to satisfy the type. The "canonical reference" being
//!    URL-shaped is GitHub bleeding into the domain.
//! 2. **`Diff::HeaderChange.file` carries the literal `"registry.csv"`**
//!    (`crates/domain` — `HeaderChange`). A table has columns, not files.
//!    This adapter treats header changes as *substrate-visible* (per the
//!    ADR-027 parity re-scope) and does not route by `.file`.
//! 3. **Row→container routing is not in the `Diff`.** The CSV adapter
//!    decides which *file* a row lands in via `classify_row`; that
//!    knowledge lives only in `transport_github_pr`, not in the contract,
//!    so a non-CSV adapter applies every registry-shaped row to its one
//!    table. Fine here, but the contract under-specifies it.
//!
//! `submit` applies immediately (there is no external review gate in an
//! in-memory substrate), so `status` reports `Merged`. Acceptance still
//! belongs to the policy authority in any *real* backend (ADR-016); this
//! adapter is a test instrument, not a production sink.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::sync::Mutex;

use qx_domain::{Diff, Proposal, ProposalRef, ProposalStatus};
use qx_transport::{ProposalError, ProposalSink};

/// Canonical registry state: `id -> {column -> value}`. Empty-valued
/// cells are never stored — a table has no concept of an empty CSV cell.
/// This is exactly the normalization the parity comparison applies to the
/// CSV adapter's output, so the two substrates compare equal on the
/// *logical* row set.
pub type Rows = BTreeMap<String, BTreeMap<String, String>>;

/// In-memory non-CSV `ProposalSink`.
pub struct TableSink {
    rows: Mutex<Rows>,
    submitted: Mutex<u64>,
}

impl TableSink {
    /// Empty registry.
    pub fn new() -> Self {
        Self {
            rows: Mutex::new(Rows::new()),
            submitted: Mutex::new(0),
        }
    }

    /// Seed with a base registry state (the parity suite applies each
    /// proposal independently from a common base, matching the CSV
    /// adapter's stateless-per-submit-against-`main` semantics).
    pub fn with_base(base: Rows) -> Self {
        Self {
            rows: Mutex::new(base),
            submitted: Mutex::new(0),
        }
    }

    /// The applied ground-truth state.
    pub fn state(&self) -> Rows {
        self.rows.lock().expect("rows lock").clone()
    }

    fn apply(&self, diff: &Diff) -> Result<(), ProposalError> {
        let mut rows = self.rows.lock().expect("rows lock");

        // Registry never deletes (ADR-012). Refuse the same illegal diff
        // `transport_github_pr` refuses, so the two adapters agree on
        // rejection — that agreement is itself a parity property.
        if !diff.deletes.is_empty() {
            return Err(ProposalError::Rejected(
                "registry deletes are not permitted per ADR-012 — \
                 use a void edit (status -> void) instead"
                    .into(),
            ));
        }

        // header_changes: substrate-visible (leak #2). A table has no
        // file/header artifact, so this adapter ignores `.file` routing.
        // Cross-substrate parity is scoped to row state, not headers.

        // Adds: insert id -> (non-empty fields + id).
        for add in &diff.adds {
            if let Some(id) = &add.id {
                let mut fields = strip_empty(&add.fields);
                fields.insert("id".into(), id.as_str().to_owned());
                rows.insert(id.as_str().to_owned(), fields);
            }
        }

        // Edits: merge `after` by id; promote-to-add when absent, matching
        // `transport_github_pr`'s apply_diff_to_file. Empty `after` values
        // clear the column (the table's analogue of an empty CSV cell).
        for edit in &diff.edits {
            let id = edit.id.as_str().to_owned();
            let entry = rows.entry(id.clone()).or_default();
            entry.insert("id".into(), id);
            for (k, v) in &edit.after {
                if v.is_empty() {
                    entry.remove(k);
                } else {
                    entry.insert(k.clone(), v.clone());
                }
            }
        }

        Ok(())
    }
}

impl Default for TableSink {
    fn default() -> Self {
        Self::new()
    }
}

fn strip_empty(m: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    m.iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

impl ProposalSink for TableSink {
    fn submit(&self, proposal: Proposal) -> Result<ProposalRef, ProposalError> {
        self.apply(&proposal.diff)?;
        let mut n = self.submitted.lock().expect("counter lock");
        *n += 1;
        Ok(ProposalRef {
            // LEAK #1: the contract demands a URL even for a backend that
            // has none. Mint a synthetic scheme so the type is satisfiable.
            url: format!("table://local/proposal/{n}"),
            local_id: Some(format!("row-batch-{n}")),
            adapter: "table".into(),
        })
    }

    fn status(&self, _proposal_ref: &ProposalRef) -> Result<ProposalStatus, ProposalError> {
        // In-memory apply is immediate; there is no external gate to poll.
        Ok(ProposalStatus::Merged)
    }
}
