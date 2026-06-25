//! End-to-end test for the `mint` CLI per foundation issue #32.
//!
//! Wires an in-memory `DryRunSink`, a fake `IdentityProvider`, and a
//! tempdir-backed `CsvGitRepository`; invokes `run_mint`; asserts on:
//!
//! - stdout summary parity (mirrors `mint.py`'s output shape)
//! - Diff submitted to the sink (N RowAdds, all ADR-012 alphabet)
//! - `audit_log.csv` round-trip via `Repository::list_audit_events`
//!   (one `RowAdd` AuditEntry per minted ID, ADR-022 empty
//!   signatures + chain_hash columns).

mod common;

use qx_cli::{run_mint, MintArgs};
use qx_domain::{ActionKind, PartFilter, PART_ID_ALPHABET, PART_ID_LEN};
use qx_storage::AuditFilter;

#[test]
fn mint_three_ids_produces_diff_and_audit_entries() {
    let (_tmp, wiring, store) = common::fresh_wiring();

    let args = MintArgs {
        count: 3,
        batch: Some("B-test-001".into()),
        dry_run: true,
        dry_run_file: None,
        local: false,
    };

    let outcome = run_mint(&args, &wiring).expect("mint succeeds");
    assert_eq!(outcome.minted.len(), 3);
    assert_eq!(outcome.batch, "B-test-001");

    // Each minted id is ADR-012 conformant.
    for id in &outcome.minted {
        assert_eq!(id.as_str().chars().count(), PART_ID_LEN);
        for c in id.as_str().chars() {
            assert!(PART_ID_ALPHABET.contains(c));
        }
    }

    // Sink received one Proposal containing 3 RowAdds.
    let proposals = store.lock().unwrap();
    assert_eq!(proposals.len(), 1);
    let p = &proposals[0];
    assert_eq!(p.diff.adds.len(), 3);
    assert!(p.diff.edits.is_empty());
    assert!(p.diff.deletes.is_empty());
    assert!(p.diff.header_changes.is_empty());
    assert_eq!(p.batch_label.as_deref(), Some("B-test-001"));

    // minted_by is populated with the operator ID (#18).
    for add in &p.diff.adds {
        let minted_by = add.fields.get("minted_by").expect("minted_by must be set");
        assert_eq!(minted_by, "test:tester", "minted_by must match operator id");
    }

    // change_classification matches Diff::classify().
    assert_eq!(p.change_classification.len(), 3);
    for a in &p.change_classification {
        assert_eq!(a.kind(), ActionKind::RowAdd);
    }

    // audit_log.csv has 3 RowAdd entries via the storage adapter.
    let entries = wiring
        .repo
        .list_audit_events(&AuditFilter::default())
        .expect("list audit events");
    assert_eq!(entries.len(), 3);
    for e in &entries {
        assert_eq!(e.action.kind(), ActionKind::RowAdd);
        // ADR-023 forward-compat columns: empty at MVP.
        assert!(e.signatures.is_empty());
        assert!(e.chain_hash.is_none());
    }
}

#[test]
fn mint_zero_count_errors() {
    let (_tmp, wiring, _store) = common::fresh_wiring();
    let args = MintArgs {
        count: 0,
        batch: None,
        dry_run: true,
        dry_run_file: None,
        local: false,
    };
    assert!(run_mint(&args, &wiring).is_err());
}

#[test]
fn mint_summary_text_matches_python_shape() {
    let (_tmp, wiring, _store) = common::fresh_wiring();
    let args = MintArgs {
        count: 2,
        batch: Some("B-fixture".into()),
        dry_run: true,
        dry_run_file: None,
        local: false,
    };
    let outcome = run_mint(&args, &wiring).unwrap();
    let s = qx_cli::render_mint_summary(&outcome, &wiring.repo_root);
    assert!(s.starts_with("minted 2 ids in batch B-fixture\n"));
    assert!(s.contains("  registry: "));
    assert!(s.contains("render labels:  label --batch B-fixture --layout horz"));
}

#[test]
fn audit_entry_roundtrips_through_storage_with_empty_signatures() {
    // Foundation issue #32 §"Forward-compat": empty Signature vec
    // must round-trip via Repository::append + list per ADR-023.
    let (_tmp, wiring, _store) = common::fresh_wiring();
    let args = MintArgs {
        count: 1,
        batch: Some("B-roundtrip".into()),
        dry_run: true,
        dry_run_file: None,
        local: false,
    };
    let outcome = run_mint(&args, &wiring).unwrap();
    let entries = wiring
        .repo
        .list_audit_events(&AuditFilter::default())
        .unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert!(e.signatures.is_empty());
    assert!(e.chain_hash.is_none());
    // The minted_id is the audit target.
    match &e.target {
        qx_domain::TargetRef::Part { id } => {
            assert_eq!(id, &outcome.minted[0]);
        }
        other => panic!("expected Part target, got {other:?}"),
    }
}

#[test]
fn mint_avoids_collisions_with_existing_registry() {
    // Pre-seed the registry with a row whose ID is in the canonical
    // alphabet; verify mint does not produce a duplicate.
    // Use a known-good 14-char ID.
    let existing_id = "K7M3PQ9RT5VAXY";
    let rows = vec![(existing_id, "unbound", "B-seed")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = MintArgs {
        count: 5,
        batch: Some("B-fresh".into()),
        dry_run: true,
        dry_run_file: None,
        local: false,
    };
    let outcome = run_mint(&args, &wiring).unwrap();
    for id in &outcome.minted {
        assert_ne!(id.as_str(), existing_id);
    }
    // Verify that listing parts (after mint) — Note: dry-run doesn't
    // actually write to registry.csv; the assertion is that the diff
    // would not collide.
    let _ = wiring.repo.list_parts(&PartFilter::default()).unwrap();
}
