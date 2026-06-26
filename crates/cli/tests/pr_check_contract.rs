//! End-to-end tests for the contract-driven `qx check` path (ADR-039,
//! task #20). Drives the compiled `pr` binary against a temp data repo
//! holding a canonical `.qx/contract.json` + `collections/
//! *.jsonl`, exercising:
//!
//! - structural mode (no `--base`): every record validated against its
//!   collection descriptor; a malformed record fails the gate.
//! - reference FK integrity across collections.
//! - commit-resolved effective-dating (`--base`): only new/changed
//!   records are re-validated; an untouched (even invalid) record is not
//!   re-litigated.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;

const TWO_COLLECTION_CONTRACT: &str = r#"{
  "format_version": 1,
  "collections": [
    { "name": "parts",
      "id": { "scheme": "nano14", "default": true, "mintable": true },
      "lifecycle": { "statuses": ["unbound","bound","void"],
        "transitions": { "unbound": ["bound","void"], "bound": ["void"], "void": [] },
        "initial": "unbound" },
      "fields": [
        { "key": "type", "type": "string", "label": "Type", "required_to_enter": "bound" },
        { "key": "torque", "type": "decimal", "label": "Torque", "precision": 4, "scale": 2, "min": 0 },
        { "key": "manufacturer", "type": "reference", "label": "Manufacturer",
          "collection": "companies", "on_unknown": "reject" }
      ] },
    { "name": "companies",
      "id": { "scheme": "nano14", "default": false, "mintable": true },
      "fields": [ { "key": "label", "type": "string", "label": "Name", "required": true } ] }
  ]
}"#;

fn write_repo(dir: &Path, contract: &str, parts: &str, companies: &str) {
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::create_dir_all(dir.join("collections")).unwrap();
    fs::write(dir.join(".qx/contract.json"), contract).unwrap();
    fs::write(dir.join("collections/parts.jsonl"), parts).unwrap();
    fs::write(dir.join("collections/companies.jsonl"), companies).unwrap();
}

fn pr_check(dir: &Path, base: Option<&str>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_qx"));
    cmd.arg("check").arg("--path").arg(dir);
    if let Some(b) = base {
        cmd.arg("--base").arg(b);
    }
    cmd.current_dir(dir).output().expect("spawn pr")
}

fn git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_init(dir: &Path) {
    git(&["init", "--initial-branch=main"], dir);
    git(&["config", "user.email", "test@example.invalid"], dir);
    git(&["config", "user.name", "Test"], dir);
    git(&["config", "commit.gpgsign", "false"], dir);
}

#[test]
fn structural_mode_accepts_a_clean_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(
        dir,
        TWO_COLLECTION_CONTRACT,
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"COMP0001\"}\n",
        "{\"id\":\"COMP0001\",\"label\":\"Acme\"}\n",
    );
    let out = pr_check(dir, None);
    assert!(
        out.status.success(),
        "expected OK, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn structural_mode_rejects_malformed_record() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    // Missing `type` at status bound (required_to_enter) AND torque scale
    // 3 > declared 2 — two independent errors.
    write_repo(
        dir,
        TWO_COLLECTION_CONTRACT,
        "{\"id\":\"PART0002\",\"status\":\"bound\",\"torque\":\"9.999\",\"manufacturer\":\"COMP0001\"}\n",
        "{\"id\":\"COMP0001\",\"label\":\"Acme\"}\n",
    );
    let out = pr_check(dir, None);
    assert!(!out.status.success(), "expected failure");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("type"), "want a `type` error, got:\n{err}");
    assert!(err.contains("scale"), "want a scale error, got:\n{err}");
}

#[test]
fn reference_fk_violation_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    // manufacturer references a company id that does not exist.
    write_repo(
        dir,
        TWO_COLLECTION_CONTRACT,
        "{\"id\":\"PART0003\",\"status\":\"bound\",\"type\":\"nut\",\"torque\":\"2.00\",\"manufacturer\":\"GHOST999\"}\n",
        "{\"id\":\"COMP0001\",\"label\":\"Acme\"}\n",
    );
    let out = pr_check(dir, None);
    assert!(!out.status.success(), "expected FK failure");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("manufacturer"), "want FK error, got:\n{err}");
}

#[test]
fn invalid_contract_fails_the_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    // reference field with no target collection — structurally invalid.
    let bad = r#"{ "format_version": 1, "collections": [
        { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true },
          "fields": [ { "key": "v", "type": "reference", "label": "V" } ] } ] }"#;
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::write(dir.join(".qx/contract.json"), bad).unwrap();
    let out = pr_check(dir, None);
    assert!(!out.status.success(), "expected invalid-contract failure");
    assert!(String::from_utf8_lossy(&out.stderr).contains("contract invalid"));
}

#[test]
fn effective_dating_skips_untouched_invalid_record() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_init(dir);

    // BASE commit: one clean record + one ALREADY-INVALID record (missing
    // `type` at bound). In reality this is history that was qualified
    // under an earlier contract; the point is we must not re-litigate it.
    let companies = "{\"id\":\"COMP0001\",\"label\":\"Acme\"}\n";
    let base_parts = concat!(
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"COMP0001\"}\n",
        "{\"id\":\"PARTBAD0\",\"status\":\"bound\",\"torque\":\"1.00\",\"manufacturer\":\"COMP0001\"}\n",
    );
    write_repo(dir, TWO_COLLECTION_CONTRACT, base_parts, companies);
    git(&["add", "-A"], dir);
    git(&["commit", "-m", "base"], dir);
    let base_sha = {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // HEAD: leave PARTBAD0 untouched, ADD a new clean record.
    let head_parts = concat!(
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"COMP0001\"}\n",
        "{\"id\":\"PARTBAD0\",\"status\":\"bound\",\"torque\":\"1.00\",\"manufacturer\":\"COMP0001\"}\n",
        "{\"id\":\"PART0002\",\"status\":\"bound\",\"type\":\"nut\",\"torque\":\"2.00\",\"manufacturer\":\"COMP0001\"}\n",
    );
    fs::write(dir.join("collections/parts.jsonl"), head_parts).unwrap();

    // With --base: only PART0002 (new) is in scope → PARTBAD0 is skipped
    // → the gate PASSES (effective-dating, ADR-039 §6).
    let out = pr_check(dir, Some(&base_sha));
    assert!(
        out.status.success(),
        "effective-dating should skip the untouched invalid record; got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Without --base: structural mode re-validates ALL records → the old
    // invalid PARTBAD0 is caught → FAIL. This is the contrast that proves
    // the --base path is actually filtering.
    let out = pr_check(dir, None);
    assert!(
        !out.status.success(),
        "structural mode must catch the invalid record"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("PARTBAD0"));
}

#[test]
fn contract_tightening_revalidates_untouched_records() {
    // The effective-dating leak (ADR-039 §6, M-A.1 S5): a CONTRACT
    // tightening can invalidate an UNTOUCHED record without that record
    // appearing in the diff. The gate must re-validate every record of a
    // collection whose descriptor changed, even untouched ones.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_init(dir);

    let companies = "{\"id\":\"COMP0001\",\"label\":\"Acme\"}\n";
    let parts =
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"COMP0001\"}\n";
    write_repo(dir, TWO_COLLECTION_CONTRACT, parts, companies);
    git(&["add", "-A"], dir);
    git(&["commit", "-m", "base"], dir);
    let base_sha = {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // HEAD: tighten the `parts` descriptor (add a required `grade`) but
    // leave PART0001 UNTOUCHED — it now violates the new requirement.
    let tightened = TWO_COLLECTION_CONTRACT.replace(
        r#"{ "key": "type", "type": "string", "label": "Type", "required_to_enter": "bound" },"#,
        concat!(
            r#"{ "key": "type", "type": "string", "label": "Type", "required_to_enter": "bound" },"#,
            "\n        ",
            r#"{ "key": "grade", "type": "string", "label": "Grade", "required": true },"#,
        ),
    );
    assert_ne!(
        tightened, TWO_COLLECTION_CONTRACT,
        "replace must alter the contract"
    );
    fs::write(dir.join(".qx/contract.json"), &tightened).unwrap();

    // Pre-S5 this skipped PART0001 (untouched) and PASSED — the leak. Now
    // the reshaped `parts` collection forces re-validation → FAIL.
    let out = pr_check(dir, Some(&base_sha));
    assert!(
        !out.status.success(),
        "a contract tightening must re-validate untouched records; gate wrongly passed.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("PART0001"),
        "expected PART0001 to be flagged by the tightened contract"
    );
}

#[test]
fn effective_dating_catches_a_changed_record() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_init(dir);

    let companies = "{\"id\":\"COMP0001\",\"label\":\"Acme\"}\n";
    let base_parts =
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"COMP0001\"}\n";
    write_repo(dir, TWO_COLLECTION_CONTRACT, base_parts, companies);
    git(&["add", "-A"], dir);
    git(&["commit", "-m", "base"], dir);
    let base_sha = {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // HEAD: EDIT PART0001 into an invalid state (torque scale 3 > 2).
    let head_parts =
        "{\"id\":\"PART0001\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.555\",\"manufacturer\":\"COMP0001\"}\n";
    fs::write(dir.join("collections/parts.jsonl"), head_parts).unwrap();

    // The changed record IS in scope → the gate catches it even with --base.
    let out = pr_check(dir, Some(&base_sha));
    assert!(
        !out.status.success(),
        "a changed record must be re-validated"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("PART0001"));
}

#[test]
fn cyclic_component_graph_fails_the_gate() {
    // ADR-035 §1a component-graph-integrity: a self-referential `acyclic`
    // relation whose records form a cycle is rejected by `qx check`.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let contract = r#"{ "format_version": 1, "collections": [
        { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true },
          "open_properties": true,
          "relations": [ { "name": "components", "target": "parts", "acyclic": true, "kind": "many-many" } ] } ] }"#;
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::create_dir_all(dir.join("collections")).unwrap();
    fs::write(dir.join(".qx/contract.json"), contract).unwrap();
    // P1 → P2 → P1.
    fs::write(
        dir.join("collections/parts.jsonl"),
        "{\"id\":\"P1\",\"components\":[\"P2\"]}\n{\"id\":\"P2\",\"components\":[\"P1\"]}\n",
    )
    .unwrap();

    let out = pr_check(dir, None);
    assert!(
        !out.status.success(),
        "a cyclic component graph must fail the gate"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cycle"),
        "expected a cycle error, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
