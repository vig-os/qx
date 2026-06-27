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

fn qx_verify(dir: &Path, anchors: bool) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_qx"));
    cmd.arg("verify").arg("--path").arg(dir);
    if anchors {
        cmd.arg("--anchors");
    }
    cmd.current_dir(dir).output().expect("spawn verify")
}

#[test]
fn verify_passes_clean_clone_and_catches_tampering_offline() {
    // ADR-037 §5: `qx verify` checks a clone offline — contract/FK,
    // content-hash + checkpoints, personas — with no base and no network.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(
        dir,
        TWO_COLLECTION_CONTRACT,
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
        "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n",
    );
    fs::write(dir.join("audit_log.jsonl"), "{\"a\":1}\n{\"b\":2}\n").unwrap();
    Command::new(env!("CARGO_BIN_EXE_qx"))
        .args(["checkpoint", "--path"])
        .arg(dir)
        .output()
        .unwrap();

    // Clean clone verifies.
    let out = qx_verify(dir, false);
    assert!(
        out.status.success(),
        "clean clone must verify, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("verify: OK"));

    // --anchors reports the reserved checks (does not silently pass over).
    let out = qx_verify(dir, true);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("anchor-ledger"),
        "expected the reserved-anchors notice"
    );

    // Tamper a checkpoint-pinned audit line → verify fails offline.
    fs::write(dir.join("audit_log.jsonl"), "{\"a\":999}\n{\"b\":2}\n").unwrap();
    let out = qx_verify(dir, false);
    assert!(!out.status.success(), "tampered clone must fail verify");
    assert!(String::from_utf8_lossy(&out.stderr).contains("digest mismatch"));
}

#[test]
fn checkpoint_then_check_verifies_and_catches_tampering() {
    // ADR-037 §1: `qx checkpoint` pins the stream; a later `qx check`
    // verifies it, and tampering a pinned line is caught (base-free).
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    let log = "{\"seq\":1,\"v\":\"a\"}\n{\"seq\":2,\"v\":\"b\"}\n";
    fs::write(dir.join("audit_log.jsonl"), log).unwrap();

    // Write a checkpoint.
    let out = Command::new(env!("CARGO_BIN_EXE_qx"))
        .args(["checkpoint", "--path"])
        .arg(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "checkpoint must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("audit_checkpoints.jsonl").exists());

    // Clean check passes (the checkpoint verifies).
    assert!(
        pr_check(dir, None).status.success(),
        "checkpointed clean repo must pass"
    );

    // Tamper a pinned line → check fails on the checkpoint digest.
    fs::write(
        dir.join("audit_log.jsonl"),
        "{\"seq\":1,\"v\":\"TAMPERED\"}\n{\"seq\":2,\"v\":\"b\"}\n",
    )
    .unwrap();
    let out = pr_check(dir, None);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "tampered stream must fail: {stderr}");
    assert!(
        stderr.contains("checkpoint") && stderr.contains("digest mismatch"),
        "expected a checkpoint digest violation, got:\n{stderr}"
    );
}

#[test]
fn content_hash_mismatch_fails_the_gate() {
    // ADR-037 §1: an entry whose recorded content_hash does not match its
    // body is in-line tamper evidence — caught with no base.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    // A complete AuditEntry JSON line with a bogus content_hash.
    let bad = "{\"request_id\":\"00000000-0000-0000-0000-000000000001\",\"timestamp\":[1970,1,0,0,0,0,0,0,0],\"actor\":{\"id\":\"github:x\",\"display_name\":\"X\",\"source\":{\"kind\":\"git_config\"},\"verified_at\":null,\"claims\":{},\"pubkey\":null},\"action\":{\"kind\":\"add\",\"row\":{}},\"target\":{\"kind\":\"part\",\"id\":\"PART2223AAAAAA\"},\"before\":null,\"after\":null,\"extra\":{},\"content_hash\":\"sha256:deadbeef\"}\n";
    fs::write(dir.join("audit_log.jsonl"), bad).unwrap();
    let out = pr_check(dir, None);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "bad content_hash must fail: {stderr}"
    );
    assert!(
        stderr.contains("content_hash does not match"),
        "expected a content_hash violation, got:\n{stderr}"
    );
}

#[test]
fn registries_command_lists_the_workspace() {
    // ADR-033 §5: `qx registries` lists the operator-workspace registries
    // and marks the default.
    let tmp = tempfile::tempdir().unwrap();
    let ws = tmp.path().join("registries.toml");
    fs::write(
        &ws,
        "default = \"acme\"\n\n[registries.acme]\nlocator = \"github:acme/parts\"\nidentity = \"persona:alice\"\n\n[registries.local]\nlocator = \"/tmp/dev\"\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_qx"))
        .args(["registries", "--path"])
        .arg(&ws)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "registries must succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("acme (default)"), "got:\n{stdout}");
    assert!(stdout.contains("github:acme/parts"), "got:\n{stdout}");
    assert!(stdout.contains("persona:alice"), "got:\n{stdout}");
    assert!(stdout.contains("local"), "got:\n{stdout}");
}

#[test]
fn structural_mode_accepts_a_clean_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(
        dir,
        TWO_COLLECTION_CONTRACT,
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
        "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n",
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
        "{\"id\":\"PART2224AAAAAA\",\"status\":\"bound\",\"torque\":\"9.999\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
        "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n",
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
        "{\"id\":\"PART2225AAAAAA\",\"status\":\"bound\",\"type\":\"nut\",\"torque\":\"2.00\",\"manufacturer\":\"GHOST999\"}\n",
        "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n",
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
    let companies = "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n";
    let base_parts = concat!(
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
        "{\"id\":\"PARTBAD0\",\"status\":\"bound\",\"torque\":\"1.00\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
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
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
        "{\"id\":\"PARTBAD0\",\"status\":\"bound\",\"torque\":\"1.00\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
        "{\"id\":\"PART2224AAAAAA\",\"status\":\"bound\",\"type\":\"nut\",\"torque\":\"2.00\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n",
    );
    fs::write(dir.join("collections/parts.jsonl"), head_parts).unwrap();

    // With --base: only PART2224AAAAAA (new) is in scope → PARTBAD0 is skipped
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

    let companies = "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n";
    let parts =
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n";
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
    // leave PART2223AAAAAA UNTOUCHED — it now violates the new requirement.
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

    // Pre-S5 this skipped PART2223AAAAAA (untouched) and PASSED — the leak. Now
    // the reshaped `parts` collection forces re-validation → FAIL.
    let out = pr_check(dir, Some(&base_sha));
    assert!(
        !out.status.success(),
        "a contract tightening must re-validate untouched records; gate wrongly passed.\nstderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("PART2223AAAAAA"),
        "expected PART2223AAAAAA to be flagged by the tightened contract"
    );
}

#[test]
fn effective_dating_catches_a_changed_record() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_init(dir);

    let companies = "{\"id\":\"CMPY2223AAAAAA\",\"label\":\"Acme\"}\n";
    let base_parts =
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.50\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n";
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

    // HEAD: EDIT PART2223AAAAAA into an invalid state (torque scale 3 > 2).
    let head_parts =
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"bound\",\"type\":\"bolt\",\"torque\":\"1.555\",\"manufacturer\":\"CMPY2223AAAAAA\"}\n";
    fs::write(dir.join("collections/parts.jsonl"), head_parts).unwrap();

    // The changed record IS in scope → the gate catches it even with --base.
    let out = pr_check(dir, Some(&base_sha));
    assert!(
        !out.status.success(),
        "a changed record must be re-validated"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("PART2223AAAAAA"));
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

#[test]
fn weakening_the_parts_lifecycle_floor_fails_the_gate() {
    // ADR-035 §1 / ADR-040: a registry contract may not weaken the
    // regulated parts lifecycle (here it drops the `void` terminal status).
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let contract = r#"{ "format_version": 1, "collections": [
        { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true },
          "lifecycle": { "statuses": ["unbound","bound"], "initial": "unbound",
            "transitions": { "unbound": ["bound"], "bound": [] } },
          "fields": [ { "key": "type", "type": "string", "label": "Type" } ] } ] }"#;
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::create_dir_all(dir.join("collections")).unwrap();
    fs::write(dir.join(".qx/contract.json"), contract).unwrap();
    fs::write(dir.join("collections/parts.jsonl"), "").unwrap();

    let out = pr_check(dir, None);
    assert!(
        !out.status.success(),
        "weakening the parts lifecycle floor must fail the gate"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("floor"),
        "expected a floor violation, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn void_policy_block_fails_the_gate() {
    // ADR-035 §1a: voiding a record still referenced under a `block`
    // relation is rejected by `qx check`.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let contract = r#"{ "format_version": 1, "collections": [
        { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true },
          "open_properties": true,
          "relations": [ { "name": "components", "target": "parts", "kind": "many-many", "void_policy": "block" } ] } ] }"#;
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::create_dir_all(dir.join("collections")).unwrap();
    fs::write(dir.join(".qx/contract.json"), contract).unwrap();
    // PARTAAAAAAAAA3 still references voided PARTAAAAAAAAA2 (valid nano14).
    fs::write(
        dir.join("collections/parts.jsonl"),
        "{\"id\":\"PARTAAAAAAAAA2\",\"status\":\"void\"}\n{\"id\":\"PARTAAAAAAAAA3\",\"components\":[\"PARTAAAAAAAAA2\"]}\n",
    )
    .unwrap();

    let out = pr_check(dir, None);
    assert!(
        !out.status.success(),
        "referencing a voided record under block must fail the gate"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("void_policy"),
        "expected a void_policy error, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// sha256 of an empty byte string — used as a known content-address.
const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn attachment_repo(dir: &std::path::Path, with_blob: bool) {
    let contract = r#"{ "format_version": 1, "collections": [
        { "name": "parts", "id": { "scheme": "nano14", "default": true, "mintable": true },
          "lifecycle": { "statuses": ["unbound","bound","void"], "initial": "unbound",
            "transitions": { "unbound": ["bound","void"], "bound": ["void"], "void": [] } },
          "fields": [ { "key": "datasheet", "type": "attachment", "label": "Datasheet" } ] } ] }"#;
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::create_dir_all(dir.join("collections")).unwrap();
    fs::write(dir.join(".qx/contract.json"), contract).unwrap();
    let rec = format!(
        "{{\"id\":\"PARTAAAAAAAAA2\",\"datasheet\":{{\"ref\":\"sha256:{EMPTY_SHA}\",\"name\":\"x.pdf\"}}}}\n"
    );
    fs::write(dir.join("collections/parts.jsonl"), rec).unwrap();
    if with_blob {
        fs::create_dir_all(dir.join("attachments")).unwrap();
        fs::write(dir.join(format!("attachments/{EMPTY_SHA}.pdf")), b"").unwrap();
    }
}

#[test]
fn attachment_blob_missing_fails_the_gate() {
    let tmp = tempfile::tempdir().unwrap();
    attachment_repo(tmp.path(), false);
    let out = pr_check(tmp.path(), None);
    assert!(!out.status.success(), "a missing attachment blob must fail");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("attachment blob missing"),
        "got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn attachment_blob_present_and_matching_passes() {
    let tmp = tempfile::tempdir().unwrap();
    attachment_repo(tmp.path(), true);
    let out = pr_check(tmp.path(), None);
    assert!(
        out.status.success(),
        "a present, hash-matching blob must pass, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn audit_log_tampering_fails_the_gate() {
    // ADR-037 §1: a PR that rewrites an existing audit_log.jsonl entry
    // (not a pure trailing append) is rejected by `qx check --base`.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git_init(dir);
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    fs::write(
        dir.join("audit_log.jsonl"),
        "{\"request_id\":\"a\",\"action\":\"mint\"}\n{\"request_id\":\"b\",\"action\":\"bind\"}\n",
    )
    .unwrap();
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
    // HEAD tampers with the FIRST (existing) entry — not append-only.
    fs::write(
        dir.join("audit_log.jsonl"),
        "{\"request_id\":\"a\",\"action\":\"TAMPERED\"}\n{\"request_id\":\"b\",\"action\":\"bind\"}\n",
    )
    .unwrap();

    let out = pr_check(dir, Some(&base_sha));
    assert!(
        !out.status.success(),
        "rewriting an existing audit entry must fail the gate"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("append-only"),
        "expected an append-only violation, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn personas_cross_check_rejects_unknown_operator_and_codeowner() {
    // ADR-036 §1/§2: with a personas collection, an audit operator that is
    // not a declared persona AND a CODEOWNERS principal that does not
    // resolve to an active persona both fail the gate.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    fs::write(
        dir.join("collections/personas.jsonl"),
        "{\"id\":\"persona:alice\",\"status\":\"active\",\"github_login\":\"alice\",\"legal_name\":\"Alice A\"}\n\
         {\"id\":\"persona:bob\",\"status\":\"revoked\",\"github_login\":\"bob\",\"legal_name\":\"Bob B\"}\n",
    )
    .unwrap();
    // An audit entry whose operator is NOT a declared persona.
    fs::write(
        dir.join("audit_log.jsonl"),
        "{\"request_id\":\"00000000-0000-0000-0000-000000000001\",\"timestamp\":[1970,1,0,0,0,0,0,0,0],\"actor\":{\"id\":\"github:stranger\",\"display_name\":\"S\",\"source\":{\"kind\":\"git_config\"},\"verified_at\":null,\"claims\":{},\"pubkey\":null},\"action\":{\"kind\":\"add\",\"row\":{}},\"target\":{\"kind\":\"part\",\"id\":\"PART2223AAAAAA\"},\"before\":null,\"after\":null,\"extra\":{}}\n",
    )
    .unwrap();
    // CODEOWNERS names a revoked persona (bob) and a team (skipped).
    fs::write(dir.join("CODEOWNERS"), "*  @bob @org/admins\n").unwrap();

    let out = pr_check(dir, None);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "personas violations must fail the gate, got:\n{stderr}"
    );
    assert!(
        stderr.contains("audit operator `github:stranger`"),
        "expected the operator FK violation, got:\n{stderr}"
    );
    assert!(
        stderr.contains("not active"),
        "expected the revoked-CODEOWNERS-principal violation, got:\n{stderr}"
    );
}

#[test]
fn personas_approver_must_resolve_to_active_persona() {
    // ADR-036 §2: a merge approver (supplied by CI via --approver) who does
    // not resolve to an active persona fails the gate.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    fs::write(
        dir.join("collections/personas.jsonl"),
        "{\"id\":\"persona:alice\",\"status\":\"active\",\"github_login\":\"alice\",\"legal_name\":\"Alice A\"}\n",
    )
    .unwrap();

    // An unregistered approver blocks.
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_qx"));
    cmd.args(["check", "--path"])
        .arg(dir)
        .args(["--approver", "stranger"]);
    let out = cmd.current_dir(dir).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "unregistered approver must block: {stderr}"
    );
    assert!(
        stderr.contains("merge approver `stranger`"),
        "got:\n{stderr}"
    );

    // An active-persona approver passes.
    let mut ok = Command::new(env!("CARGO_BIN_EXE_qx"));
    ok.args(["check", "--path"])
        .arg(dir)
        .args(["--approver", "alice"]);
    let out = ok.current_dir(dir).output().unwrap();
    assert!(
        out.status.success(),
        "active-persona approver must pass, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn manifest_fk_rejects_undeclared_collection() {
    // ADR-034 §3 / capability-grain: a manifest [ops] key naming a
    // collection the contract does not declare fails the gate.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    // `vendors` is not a declared collection (the contract has parts +
    // companies).
    fs::write(
        dir.join(".qx/manifest.toml"),
        "[registry]\nid = \"acme\"\nname = \"Acme\"\n\n[ops]\n\"create:parts\" = \"on\"\n\"create:vendors\" = \"off\"\n",
    )
    .unwrap();
    let out = pr_check(dir, None);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "dangling manifest FK must fail: {stderr}"
    );
    assert!(
        stderr.contains("vendors") && stderr.contains("not declared"),
        "expected a manifest FK violation, got:\n{stderr}"
    );
}

#[test]
fn manifest_fk_passes_when_collections_declared() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    fs::write(
        dir.join(".qx/manifest.toml"),
        "[registry]\nid = \"acme\"\nname = \"Acme\"\n\n[ops]\n\"create:parts\" = \"on\"\n\"transition:companies:archived\" = \"on\"\n\n[roles.lead]\n\"parts:bulk\" = \"approve\"\n",
    )
    .unwrap();
    let out = pr_check(dir, None);
    assert!(
        out.status.success(),
        "all-declared manifest must pass, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn personas_cross_check_passes_when_all_resolve() {
    // The same shape, but every principal resolves to an active persona.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    fs::write(
        dir.join("collections/personas.jsonl"),
        "{\"id\":\"persona:alice\",\"status\":\"active\",\"github_login\":\"alice\",\"legal_name\":\"Alice A\"}\n",
    )
    .unwrap();
    fs::write(
        dir.join("audit_log.jsonl"),
        "{\"request_id\":\"00000000-0000-0000-0000-000000000002\",\"timestamp\":[1970,1,0,0,0,0,0,0,0],\"actor\":{\"id\":\"persona:alice\",\"display_name\":\"Alice\",\"source\":{\"kind\":\"git_config\"},\"verified_at\":null,\"claims\":{},\"pubkey\":null},\"action\":{\"kind\":\"add\",\"row\":{}},\"target\":{\"kind\":\"part\",\"id\":\"PART2223AAAAAA\"},\"before\":null,\"after\":null,\"extra\":{}}\n",
    )
    .unwrap();
    fs::write(dir.join("CODEOWNERS"), "*  @alice\n").unwrap();

    let out = pr_check(dir, None);
    assert!(
        out.status.success(),
        "all-resolve personas repo must pass, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn committed_csv_export_fails_the_gate() {
    // ADR-035: CSV is an export view; a *.csv committed beside the JSONL
    // collections is rejected (generate on demand, never store).
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_repo(dir, TWO_COLLECTION_CONTRACT, "", "");
    fs::write(dir.join("collections/parts.csv"), "id,status\n").unwrap();
    let out = pr_check(dir, None);
    assert!(
        !out.status.success(),
        "a committed CSV export must fail the gate"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("CSV export"),
        "expected a committed-CSV-export error, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn qx_check_dispatches_validation_on_kind() {
    // parts has no open properties and no `resistance` field — that field
    // is contributed only by the `resistor` kind (ADR-035 §5 kind tree).
    let contract = r#"{"format_version":1,"collections":[
        {"name":"parts","id":{"scheme":"nano14","default":true,"mintable":true},
         "lifecycle":{"statuses":["unbound","bound","void"],"initial":"unbound",
           "transitions":{"unbound":["bound","void"],"bound":["void"],"void":[]}},
         "fields":[{"key":"type","type":"string","label":"Type"}]}]}"#;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::create_dir_all(dir.join("collections")).unwrap();
    fs::create_dir_all(dir.join(".qx")).unwrap();
    fs::write(dir.join(".qx/contract.json"), contract).unwrap();
    fs::write(
        dir.join("collections/parts.jsonl"),
        "{\"id\":\"PART2223AAAAAA\",\"status\":\"unbound\",\"kind\":\"resistor\",\"resistance\":\"10k\",\"type\":\"r\"}\n",
    )
    .unwrap();

    // Without the kind tree, `resistance` is an unknown field — fails.
    let out = pr_check(dir, None);
    assert!(
        !out.status.success(),
        "an unknown kind field must fail without the types collection"
    );

    // Declare the kind in the types collection — the field is accepted.
    fs::write(
        dir.join("collections/types.jsonl"),
        "{\"id\":\"resistor\",\"fields\":[{\"key\":\"resistance\",\"type\":\"string\",\"label\":\"R\"}]}\n",
    )
    .unwrap();
    let out2 = pr_check(dir, None);
    assert!(
        out2.status.success(),
        "the kind's field is accepted once the kind tree is declared: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
}
