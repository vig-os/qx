//! End-to-end tests for `qx init` (ADR-039) — scaffolding a deployable
//! company data repo. Drives the compiled `pr` binary and proves the
//! scaffold passes its OWN `qx check` gate (the deployment is valid out of
//! the box and stays valid as real records are added).

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn pr(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qx"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn pr")
}

#[test]
fn init_scaffolds_a_repo_that_passes_its_own_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    let out = pr(&["init", "--path", "."], dir);
    assert!(
        out.status.success(),
        "qx init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The scaffold exists.
    assert!(dir.join(".qx/contract.json").exists());
    assert!(dir.join("collections/parts.jsonl").exists());
    assert!(dir.join("collections/companies.jsonl").exists());
    assert!(dir.join("collections/contacts.jsonl").exists());
    assert!(dir.join(".github/workflows/check.yml").exists());
    assert!(dir.join("README.md").exists());

    // An empty scaffold is a VALID repo — it passes its own gate.
    let out = pr(&["check", "--path", "."], dir);
    assert!(
        out.status.success(),
        "fresh scaffold should pass qx check: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn init_then_add_records_still_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(pr(&["init", "--path", "."], dir).status.success());

    // Add a qualified company and a part bound to it.
    fs::write(
        dir.join("collections/companies.jsonl"),
        "{\"id\":\"COMP0001\",\"status\":\"active\",\"label\":\"Acme\",\"role\":\"manufacturer\",\"qualification\":\"qualified\"}\n",
    )
    .unwrap();
    fs::write(
        dir.join("collections/parts.jsonl"),
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"M3 bolt\",\"manufacturer\":\"COMP0001\"}\n",
    )
    .unwrap();

    let out = pr(&["check", "--path", "."], dir);
    assert!(
        out.status.success(),
        "real records on the scaffold should pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn init_then_add_invalid_record_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(pr(&["init", "--path", "."], dir).status.success());

    // A part bound to a company that does not exist → FK violation.
    fs::write(
        dir.join("collections/parts.jsonl"),
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"manufacturer\":\"GHOST\"}\n",
    )
    .unwrap();

    let out = pr(&["check", "--path", "."], dir);
    assert!(!out.status.success(), "FK violation must fail the gate");
    assert!(String::from_utf8_lossy(&out.stderr).contains("manufacturer"));
}

#[test]
fn init_refuses_to_overwrite_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    assert!(pr(&["init", "--path", "."], dir).status.success());

    // Second init without --force is refused.
    let out = pr(&["init", "--path", "."], dir);
    assert!(!out.status.success(), "second init should refuse");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));

    // With --force it succeeds.
    let out = pr(&["init", "--path", ".", "--force"], dir);
    assert!(out.status.success(), "init --force should overwrite");
}
