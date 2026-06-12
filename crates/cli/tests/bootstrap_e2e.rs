//! End-to-end tests for `bootstrap_data_repo` (#35).
//!
//! Uses a local bare repo as the "remote" — no network, no GitHub
//! flakiness. Exercises the clone-if-missing + fetch+reset-if-present
//! paths plus the `PARTREG_OFFLINE` short-circuit.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;

use part_registry_cli::{bootstrap_data_repo, bootstrap_data_repo_with_options, CliError};

/// Spin up a "remote" bare repo seeded with one commit on `main`
/// containing a single `registry.csv` (header only).
fn make_remote(tmp: &Path) -> std::path::PathBuf {
    let work = tmp.join("origin-work");
    let bare = tmp.join("origin.git");
    fs::create_dir_all(&work).unwrap();

    git(&["init", "--initial-branch=main"], &work);
    fs::write(
        work.join("registry.csv"),
        "id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes,components,manufacturer_id,metadata,signatures,chain_hash\n",
    )
    .unwrap();
    // Use deterministic identity so the commit doesn't depend on the
    // host's git config.
    git(&["config", "user.email", "test@example.invalid"], &work);
    git(&["config", "user.name", "Test"], &work);
    // Disable signing in case the host has commit.gpgsign=true global.
    git(&["config", "commit.gpgsign", "false"], &work);
    git(&["add", "registry.csv"], &work);
    git(&["commit", "-m", "seed"], &work);
    git(
        &[
            "clone",
            "--bare",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
        tmp,
    );
    bare
}

fn git(args: &[&str], cwd: &Path) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bootstrap_clones_into_empty_target() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = make_remote(tmp.path());
    let target = tmp.path().join("clone");

    bootstrap_data_repo(bare.to_str().unwrap(), "main", &target).unwrap();

    assert!(target.join(".git").exists(), "expected .git dir");
    assert!(
        target.join("registry.csv").exists(),
        "expected registry.csv from remote"
    );
}

#[test]
fn bootstrap_refreshes_existing_clone() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = make_remote(tmp.path());
    let target = tmp.path().join("clone");
    bootstrap_data_repo(bare.to_str().unwrap(), "main", &target).unwrap();

    // Mutate the upstream bare repo via a temporary clone-and-push so
    // the next bootstrap call has something to pull.
    let push_src = tmp.path().join("push-src");
    git(
        &["clone", bare.to_str().unwrap(), push_src.to_str().unwrap()],
        tmp.path(),
    );
    git(&["config", "user.email", "t@e.invalid"], &push_src);
    git(&["config", "user.name", "t"], &push_src);
    git(&["config", "commit.gpgsign", "false"], &push_src);
    fs::write(push_src.join("audit_log.csv"), "header\n").unwrap();
    git(&["add", "audit_log.csv"], &push_src);
    git(&["commit", "-m", "add audit log"], &push_src);
    git(&["push", "origin", "main"], &push_src);

    // Locally dirty the working tree to verify reset --hard sees through it.
    fs::write(target.join("local-noise.txt"), "stale\n").unwrap();
    bootstrap_data_repo(bare.to_str().unwrap(), "main", &target).unwrap();

    assert!(
        target.join("audit_log.csv").exists(),
        "expected new file from second clone push"
    );
    assert!(
        !target.join("local-noise.txt").exists(),
        "reset --hard should drop local-only file"
    );
}

#[test]
fn bootstrap_offline_short_circuits_when_dir_present() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("preexisting");
    fs::create_dir_all(&target).unwrap();
    // Use the explicit-flag entry point so we don't mutate process
    // env (which races under cargo's parallel test scheduler).
    bootstrap_data_repo_with_options("ignored-url", "main", &target, true).unwrap();
}

#[test]
fn bootstrap_offline_errors_when_dir_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("does-not-exist");

    let result = bootstrap_data_repo_with_options("ignored-url", "main", &target, true);

    match result {
        Err(CliError::Bootstrap(msg)) => {
            assert!(
                msg.contains("PARTREG_OFFLINE"),
                "expected offline error, got {msg}"
            );
        }
        other => panic!("expected Bootstrap error, got {other:?}"),
    }
}

#[test]
fn bootstrap_fails_loudly_on_bad_url() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("clone");
    let result = bootstrap_data_repo("file:///nonexistent/bare.git", "main", &target);
    match result {
        Err(CliError::Bootstrap(_)) => {}
        other => panic!("expected Bootstrap error, got {other:?}"),
    }
}
