//! End-to-end test for the `bind` CLI per foundation issue #32.
//!
//! Wires a tempdir-backed `CsvGitRepository` seeded with one
//! `unbound` part; invokes `run_bind`; asserts on:
//!
//! - prefix resolution (8-char accepted; full 14-char accepted)
//! - prefix collision is detected and reported
//! - `--void` produces a `RowVoid` diff + audit entry
//! - `--rebind` allows re-binding a bound row
//! - audit entry round-trips with empty `Signature` vec

mod common;

use part_registry_cli::{run_bind, BindArgs};
use part_registry_domain::ActionKind;
use part_registry_storage::AuditFilter;

const ID_A: &str = "K7M3PQ9RT5VAXY";
const ID_B: &str = "K7M3PQABCDEFGH"; // shares 8-char prefix K7M3PQAB? No: ID_A=K7M3PQ9R, ID_B=K7M3PQAB
const ID_C: &str = "ABCDEFGHJKMNPQ";

fn bind_args(id: &str) -> BindArgs {
    BindArgs {
        id: id.into(),
        type_: None,
        description: None,
        vendor: None,
        part_number: None,
        location: None,
        notes: None,
        rebind: false,
        void: false,
        dry_run: true,
        dry_run_file: None,
    }
}

#[test]
fn bind_unbound_part_via_full_id_succeeds() {
    let rows = vec![(ID_A, "unbound", "B-test")];
    let (_tmp, wiring, store) = common::seeded_wiring(&rows);

    let mut args = bind_args(ID_A);
    args.type_ = Some("PT100".into());
    args.vendor = Some("TC Direct".into());
    args.location = Some("loop A".into());

    let out = run_bind(&args, &wiring).expect("bind succeeds");
    assert_eq!(out.id.as_str(), ID_A);
    assert!(!out.voided);
    assert_eq!(out.fields.get("status").map(String::as_str), Some("bound"));
    assert_eq!(out.fields.get("type").map(String::as_str), Some("PT100"));
    assert_eq!(
        out.fields.get("vendor").map(String::as_str),
        Some("TC Direct")
    );

    // Diff contains exactly one edit.
    let proposals = store.lock().unwrap();
    assert_eq!(proposals.len(), 1);
    let p = &proposals[0];
    assert_eq!(p.diff.edits.len(), 1);
    assert!(p.diff.adds.is_empty());

    // change_classification = RowBind (status unbound -> bound).
    assert_eq!(p.change_classification.len(), 1);
    assert_eq!(p.change_classification[0].kind(), ActionKind::RowBind);

    // Audit log has one RowBind entry with empty signatures.
    let entries = wiring
        .repo
        .list_audit_events(&AuditFilter::default())
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action.kind(), ActionKind::RowBind);
    assert!(entries[0].signatures.is_empty());
    assert!(entries[0].chain_hash.is_none());
}

#[test]
fn bind_via_8_char_prefix_succeeds() {
    let rows = vec![(ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let prefix = &ID_A[..8];
    let mut args = bind_args(prefix);
    args.type_ = Some("PT100".into());
    let out = run_bind(&args, &wiring).expect("bind by prefix succeeds");
    assert_eq!(out.id.as_str(), ID_A);
}

#[test]
fn bind_via_prefix_with_dash_normalizes() {
    let rows = vec![(ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    // Insert a dash in the middle of the 8-char prefix.
    let dashed = format!("{}-{}", &ID_A[..4], &ID_A[4..8]);
    let mut args = bind_args(&dashed);
    args.type_ = Some("PT100".into());
    let out = run_bind(&args, &wiring).unwrap();
    assert_eq!(out.id.as_str(), ID_A);
}

#[test]
fn bind_prefix_collision_is_ambiguous() {
    let rows = vec![
        (ID_A, "unbound", "B-test"), // K7M3PQ9R...
        (ID_B, "unbound", "B-test"), // K7M3PQAB...
    ];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    // Shared 6-char prefix is too short (< HUMAN_LENGTH=8) — bind
    // rejects on length, not ambiguity. Use a longer shared run.
    // Actually ID_A and ID_B share K7M3PQ as their 6-char prefix;
    // the 8-char prefix `K7M3PQ9R` is unique to ID_A so we need
    // a different fixture pair for the collision test.
    //
    // Construct a synthetic collision: two IDs sharing 8+ chars.
    drop(wiring);
    let id_a = "K7M3PQ9RT5VAXY";
    let id_collision = "K7M3PQ9RZZZZZZ";
    let rows = vec![
        (id_a, "unbound", "B-test"),
        (id_collision, "unbound", "B-test"),
    ];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let mut args = bind_args("K7M3PQ9R"); // 8-char shared
    args.type_ = Some("PT100".into());
    let err = run_bind(&args, &wiring).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("ambiguous prefix"),
        "expected ambiguity message, got: {msg}"
    );
}

#[test]
fn bind_void_sets_status_void_and_audit_void() {
    let rows = vec![(ID_A, "unbound", "B-test")];
    let (_tmp, wiring, store) = common::seeded_wiring(&rows);
    let mut args = bind_args(ID_A);
    args.void = true;

    let out = run_bind(&args, &wiring).unwrap();
    assert!(out.voided);

    let proposals = store.lock().unwrap();
    assert_eq!(proposals.len(), 1);
    let p = &proposals[0];
    assert_eq!(p.diff.edits.len(), 1);
    // Classification should be RowVoid (status -> void).
    assert_eq!(p.change_classification[0].kind(), ActionKind::RowVoid);

    let entries = wiring
        .repo
        .list_audit_events(&AuditFilter::default())
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].action.kind(), ActionKind::RowVoid);
}

#[test]
fn bind_rebind_required_for_bound_row() {
    let rows = vec![(ID_C, "bound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let mut args = bind_args(ID_C);
    args.type_ = Some("PT200".into());

    let err = run_bind(&args, &wiring).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("--rebind"), "expected --rebind hint: {msg}");

    // With --rebind it succeeds.
    args.rebind = true;
    let out = run_bind(&args, &wiring).unwrap();
    assert_eq!(out.id.as_str(), ID_C);
}

#[test]
fn bind_void_status_cannot_be_bound() {
    let rows = vec![(ID_C, "void", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let mut args = bind_args(ID_C);
    args.type_ = Some("PT200".into());
    let err = run_bind(&args, &wiring).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("voided"));
}

#[test]
fn bind_short_query_errors() {
    let rows = vec![(ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = bind_args("K7M3"); // 4 chars — below HUMAN_LENGTH=8
    let err = run_bind(&args, &wiring).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("query too short"));
}

#[test]
fn bind_unknown_id_errors() {
    let rows = vec![(ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = bind_args("ZZZZZZZZZZZZZZ");
    let err = run_bind(&args, &wiring).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("no match"));
}
