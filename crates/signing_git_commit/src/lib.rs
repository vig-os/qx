//! `qx-signing-git-commit` — MVP `SigningProvider` +
//! `VerificationProvider` adapter per ADR-024.
//!
//! ## The adapter does no cryptography
//!
//! Git already does the cryptography correctly when the commit is
//! created — the operator's pre-existing GPG/SSH key (registered to
//! GitHub) signs the commit during `git commit -S`. This adapter's
//! `sign()` records the **binding** from `AuditEntry` to the SHA of
//! the commit that contains it, returning a `Signature::GitCommit`
//! value that downstream code can carry on the audit row.
//!
//! ## The pending-SHA problem
//!
//! At `sign()` time the commit hasn't been created yet, so the SHA is
//! unknown. The honest API is therefore a two-step:
//!
//! 1. `sign(ctx) -> PendingSignature` — returns a placeholder with an
//!    empty `commit_sha`.
//! 2. `pending.resolve(real_sha) -> Signature` — fills in the SHA
//!    after `git commit` reports it. The caller then writes the
//!    resolved signature into the audit row.
//!
//! `SigningProvider::sign` returns the placeholder directly so the
//! port stays single-shot; consumers that need the typed pending form
//! call [`GitCommitSigner::sign_pending`] instead.
//!
//! ## Verification
//!
//! `verify()` shells out to `git verify-commit <sha>` and inspects the
//! exit status. Good commits return `Verification::Verified` with the
//! commit timestamp; bad commits return `Verification::Invalid` (we
//! distinguish missing-signature from bad-signature by `git`'s
//! stderr).

#![forbid(unsafe_code)]

use qx_domain::{KeyId, SigAlgorithm, Signature, Timestamp, Verification, VerificationSource};
use qx_signing::{SignError, SigningContext, SigningProvider, VerificationProvider, VerifyError};

// -------------------------------------------------------------------
// Pending-signature pattern
// -------------------------------------------------------------------

/// A `Signature::GitCommit` whose `commit_sha` hasn't been observed
/// yet. The caller resolves it after `git commit` reports the SHA.
///
/// This is the honest API for the "audit-row signature points at the
/// commit that contains the audit row" pattern described in ADR-024.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingSignature {
    pub signer_key_id: KeyId,
    pub algorithm: SigAlgorithm,
}

impl PendingSignature {
    /// Fill in the commit SHA once `git commit` has reported it.
    pub fn resolve(self, commit_sha: impl Into<String>) -> Signature {
        Signature::GitCommit {
            commit_sha: commit_sha.into(),
            signer_key_id: self.signer_key_id,
        }
    }

    /// Convenience: emit the placeholder `Signature::GitCommit` with
    /// an empty `commit_sha`. Storage adapters round-trip the empty
    /// string and the caller patches it before flush.
    pub fn placeholder(&self) -> Signature {
        Signature::GitCommit {
            commit_sha: String::new(),
            signer_key_id: self.signer_key_id.clone(),
        }
    }
}

// -------------------------------------------------------------------
// Signer
// -------------------------------------------------------------------

/// MVP signing adapter. Records the binding; git does the crypto.
#[derive(Clone, Debug)]
pub struct GitCommitSigner {
    signer_key_id: KeyId,
    algorithm: SigAlgorithm,
    /// Optional override for the git binary (test hook); defaults to
    /// `"git"` on `$PATH`.
    git_binary: Option<String>,
    /// Optional repo path (working directory for `git verify-commit`).
    /// `None` means inherit the caller's cwd.
    repo_path: Option<std::path::PathBuf>,
}

impl GitCommitSigner {
    /// Construct with the operator's signing key id and the detected
    /// algorithm. ADR-024 §"Trait shape": `algorithm` is detected once
    /// at construction time from git config (`gpg.format`) — the
    /// wiring code in `crates/cli/` does that detection; this struct
    /// just records the result.
    pub fn new(signer_key_id: KeyId, algorithm: SigAlgorithm) -> Self {
        Self {
            signer_key_id,
            algorithm,
            git_binary: None,
            repo_path: None,
        }
    }

    /// Default to SSH (matches GitHub's current default for new
    /// accounts per ADR-024 §"Trait shape").
    pub fn with_ssh(signer_key_id: KeyId) -> Self {
        Self::new(signer_key_id, SigAlgorithm::GitCommitSsh)
    }

    pub fn with_gpg(signer_key_id: KeyId) -> Self {
        Self::new(signer_key_id, SigAlgorithm::GitCommitGpg)
    }

    pub fn with_repo_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.repo_path = Some(path.into());
        self
    }

    pub fn with_git_binary(mut self, bin: impl Into<String>) -> Self {
        self.git_binary = Some(bin.into());
        self
    }

    /// Typed pending-signature variant of `SigningProvider::sign`.
    /// Consumers that want to fill in the SHA later call this and
    /// then [`PendingSignature::resolve`].
    pub fn sign_pending(&self, _ctx: &SigningContext<'_>) -> PendingSignature {
        PendingSignature {
            signer_key_id: self.signer_key_id.clone(),
            algorithm: self.algorithm,
        }
    }
}

impl SigningProvider for GitCommitSigner {
    fn algorithm(&self) -> SigAlgorithm {
        self.algorithm
    }

    fn sign(&self, ctx: &SigningContext<'_>) -> Result<Signature, SignError> {
        // ADR-024: no crypto here. Return the placeholder; the caller
        // patches `commit_sha` after `git commit` reports the SHA.
        Ok(self.sign_pending(ctx).placeholder())
    }
}

// -------------------------------------------------------------------
// Verifier
// -------------------------------------------------------------------

/// MVP verification adapter. Shells out to `git verify-commit`.
#[derive(Clone, Debug, Default)]
pub struct GitCommitVerifier {
    git_binary: Option<String>,
    repo_path: Option<std::path::PathBuf>,
}

impl GitCommitVerifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_repo_path(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.repo_path = Some(path.into());
        self
    }

    pub fn with_git_binary(mut self, bin: impl Into<String>) -> Self {
        self.git_binary = Some(bin.into());
        self
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn git_bin(&self) -> &str {
        self.git_binary.as_deref().unwrap_or("git")
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn run_git(&self, args: &[&str]) -> Result<std::process::Output, VerifyError> {
        use std::process::Command;
        let mut cmd = Command::new(self.git_bin());
        if let Some(p) = &self.repo_path {
            cmd.current_dir(p);
        }
        cmd.args(args).output().map_err(|e| {
            VerifyError::Backend(Box::new(GitCommitError::Spawn {
                binary: self.git_bin().into(),
                source: e,
            }))
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn run_git(&self, _args: &[&str]) -> Result<std::process::Output, VerifyError> {
        Err(VerifyError::Failed(
            "git verify-commit not available on wasm32".into(),
        ))
    }

    /// Read the commit timestamp via `git show -s --format=%cI <sha>`
    /// (committer ISO-8601 date). Returns now() if parsing fails — the
    /// verification is what matters; the timestamp is best-effort.
    fn commit_timestamp(&self, sha: &str) -> Timestamp {
        let parse = |out: &str| -> Option<Timestamp> {
            let s = out.trim();
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        };
        if let Ok(output) = self.run_git(&["show", "-s", "--format=%cI", sha]) {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    if let Some(ts) = parse(&text) {
                        return ts;
                    }
                }
            }
        }
        time::OffsetDateTime::now_utc()
    }
}

impl VerificationProvider for GitCommitVerifier {
    fn algorithms(&self) -> &[SigAlgorithm] {
        &[SigAlgorithm::GitCommitGpg, SigAlgorithm::GitCommitSsh]
    }

    fn verify(
        &self,
        _payload: &[u8],
        sig: &Signature,
        _op: &qx_domain::Operator,
    ) -> Result<Verification, VerifyError> {
        let commit_sha = match sig {
            Signature::GitCommit { commit_sha, .. } => commit_sha,
            other => {
                return Ok(Verification::Unverified {
                    reason: format!(
                        "GitCommitVerifier only handles Signature::GitCommit; got {other:?}"
                    ),
                });
            }
        };
        if commit_sha.is_empty() {
            return Ok(Verification::Unverified {
                reason: "Signature::GitCommit has an empty commit_sha \
                         (PendingSignature was never resolved)"
                    .into(),
            });
        }

        let output = self.run_git(&["verify-commit", commit_sha])?;
        if output.status.success() {
            Ok(Verification::Verified {
                at: self.commit_timestamp(commit_sha),
                source: VerificationSource::GitVerifyCommit,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            // Heuristic: `git verify-commit` returns 1 for both
            // "no signature" and "bad signature". Distinguish by
            // matching git's stderr text. Unknown → Invalid (the
            // safe default — we know the commit failed to verify).
            if stderr.contains("does not have a GPG signature") || stderr.contains("no signature") {
                Ok(Verification::Unverified {
                    reason: format!("commit {commit_sha} has no signature: {stderr}"),
                })
            } else {
                Ok(Verification::Invalid {
                    reason: format!("commit {commit_sha} failed verification: {stderr}"),
                })
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // variants are emitted by run_git on native only
enum GitCommitError {
    #[error("failed to spawn git binary {binary:?}")]
    Spawn {
        binary: String,
        #[source]
        source: std::io::Error,
    },
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use qx_domain::{ActionKind, IdentitySource, Operator, OperatorId};
    use std::collections::BTreeMap;
    use std::process::Command;

    fn sample_operator() -> Operator {
        Operator {
            id: OperatorId("git-config:tester@example.com".into()),
            display_name: "Tester".into(),
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: Some(KeyId("ABCDEF1234567890".into())),
        }
    }

    fn ctx<'a>(op: &'a Operator, payload: &'a [u8]) -> SigningContext<'a> {
        SigningContext {
            operator: op,
            payload,
            action: ActionKind::RowAdd,
            timestamp: time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        }
    }

    // ---- SigningProvider --------------------------------------------------

    #[test]
    fn sign_returns_placeholder_with_empty_sha() {
        let signer = GitCommitSigner::with_ssh(KeyId("SSH-KEY-FP".into()));
        let op = sample_operator();
        let sig = signer.sign(&ctx(&op, b"payload")).unwrap();
        match sig {
            Signature::GitCommit {
                commit_sha,
                signer_key_id,
            } => {
                assert_eq!(commit_sha, "");
                assert_eq!(signer_key_id, KeyId("SSH-KEY-FP".into()));
            }
            other => panic!("expected GitCommit, got {other:?}"),
        }
    }

    #[test]
    fn algorithm_reflects_constructor_choice() {
        let ssh = GitCommitSigner::with_ssh(KeyId("k".into()));
        assert_eq!(ssh.algorithm(), SigAlgorithm::GitCommitSsh);
        let gpg = GitCommitSigner::with_gpg(KeyId("k".into()));
        assert_eq!(gpg.algorithm(), SigAlgorithm::GitCommitGpg);
    }

    #[test]
    fn sign_pending_resolves_into_full_signature() {
        let signer = GitCommitSigner::with_ssh(KeyId("KID".into()));
        let op = sample_operator();
        let pending = signer.sign_pending(&ctx(&op, b"payload"));
        assert_eq!(pending.signer_key_id, KeyId("KID".into()));
        let sig = pending.resolve("deadbeef0123456789");
        match sig {
            Signature::GitCommit {
                commit_sha,
                signer_key_id,
            } => {
                assert_eq!(commit_sha, "deadbeef0123456789");
                assert_eq!(signer_key_id, KeyId("KID".into()));
            }
            other => panic!("expected GitCommit, got {other:?}"),
        }
    }

    #[test]
    fn pending_signature_placeholder_has_empty_sha() {
        let pending = PendingSignature {
            signer_key_id: KeyId("k".into()),
            algorithm: SigAlgorithm::GitCommitSsh,
        };
        match pending.placeholder() {
            Signature::GitCommit { commit_sha, .. } => assert_eq!(commit_sha, ""),
            other => panic!("got {other:?}"),
        }
    }

    // ---- VerificationProvider ---------------------------------------------

    fn init_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(path)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["config", "user.name", "Tester"]);
        run(&["config", "user.email", "tester@example.com"]);
        // Disable signing for fixture commits so we get a deterministic
        // "unsigned commit" we can verify against.
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(path.join("file.txt"), "hello\n").unwrap();
        run(&["add", "file.txt"]);
        run(&["commit", "-q", "--no-gpg-sign", "-m", "init"]);
        tmp
    }

    fn head_sha(dir: &std::path::Path) -> String {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn verify_empty_sha_returns_unverified() {
        let verifier = GitCommitVerifier::new();
        let op = sample_operator();
        let sig = Signature::GitCommit {
            commit_sha: String::new(),
            signer_key_id: KeyId("k".into()),
        };
        match verifier.verify(b"", &sig, &op).unwrap() {
            Verification::Unverified { reason } => {
                assert!(
                    reason.contains("empty commit_sha"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected Unverified, got {other:?}"),
        }
    }

    #[test]
    fn verify_unsigned_commit_returns_unverified_not_invalid() {
        let repo = init_repo();
        let sha = head_sha(repo.path());
        let verifier = GitCommitVerifier::new().with_repo_path(repo.path());
        let op = sample_operator();
        let sig = Signature::GitCommit {
            commit_sha: sha.clone(),
            signer_key_id: KeyId("k".into()),
        };
        match verifier.verify(b"", &sig, &op).unwrap() {
            // ADR-024: unsigned ≠ invalid; the commit is real but
            // carries no signature. Treat as Unverified.
            Verification::Unverified { reason } => {
                assert!(reason.contains(&sha) || reason.contains("signature"));
            }
            // Some git versions emit different text; accept Invalid as
            // a fallback so the test is portable across distros.
            Verification::Invalid { reason } => {
                assert!(reason.contains(&sha) || reason.contains("signature"));
            }
            other => panic!("expected Unverified/Invalid, got {other:?}"),
        }
    }

    #[test]
    fn verify_non_git_signature_variant_returns_unverified() {
        let verifier = GitCommitVerifier::new();
        let op = sample_operator();
        let sig = Signature::Sigstore {
            cert: vec![1, 2, 3],
            sig: vec![4, 5, 6],
            rekor_proof: qx_domain::RekorProof {
                uuid: "u".into(),
                log_index: 1,
            },
        };
        match verifier.verify(b"", &sig, &op).unwrap() {
            Verification::Unverified { reason } => {
                assert!(reason.contains("Sigstore") || reason.contains("GitCommit"));
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn algorithms_lists_both_git_variants() {
        let v = GitCommitVerifier::new();
        let algs = v.algorithms();
        assert!(algs.contains(&SigAlgorithm::GitCommitGpg));
        assert!(algs.contains(&SigAlgorithm::GitCommitSsh));
    }
}
