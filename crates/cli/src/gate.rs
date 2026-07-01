//! The vendored gate (ADR-038 §1): `.qx/gate/` carries the gate binary +
//! its `manifest.toml`, and — for the regulated `vendored` knob — the
//! attestation, source tarball, and Nix recipe so the data repo can verify
//! *and rebuild* the exact gate it runs, from the repo alone.
//!
//! [`verify`] is the **pin-verify-before-exec** step (ADR-034 §2): the
//! binary's sha256 must match the manifest before the gate is ever run, so a
//! tampered gate is caught, not executed. The vendored knob additionally
//! requires the provenance bundle to be present.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// `.qx/gate/manifest.toml` — the pin the runner verifies before exec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateManifest {
    /// `"vendored"` (regulated default — binary + provenance carried in the
    /// repo) or `"fetched"` (binary pulled from the pinned release URL).
    pub knob: String,
    /// The released gate version this manifest pins (e.g. `v0.1.0`).
    pub version: String,
    /// The gate binary filename under `.qx/gate/`.
    pub binary: String,
    /// Lowercase-hex sha256 of the binary — verified before exec.
    pub sha256: String,
    /// SLSA/sigstore attestation bundle filename (vendored knob).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<String>,
    /// Source tarball filename — the rebuild-in-2040 path (vendored knob).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Nix recipe filename (`flake.lock` snapshot) — reproducible rebuild.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
}

impl GateManifest {
    pub fn parse(s: &str) -> Result<Self, String> {
        toml::from_str(s).map_err(|e| format!("gate manifest: {e}"))
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("GateManifest serializes")
    }
}

/// Lowercase-hex sha256, dependency-free (no `hex` crate needed).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Pin-verify-before-exec (ADR-034 §2 / ADR-038 §1). `dir` is the `.qx/gate/`
/// directory. Reads the manifest, hashes the vendored binary, and rejects any
/// mismatch. For the `vendored` knob, every declared provenance file must be
/// present (a repo-alone-verifiable, rebuildable gate). Returns the manifest
/// on success so callers can log the pinned version.
pub fn verify(dir: &Path) -> Result<GateManifest, String> {
    let manifest_path = dir.join("manifest.toml");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read {}: {e}", manifest_path.display()))?;
    let m = GateManifest::parse(&raw)?;

    let bin_path = dir.join(&m.binary);
    let bytes = std::fs::read(&bin_path)
        .map_err(|e| format!("read gate binary {}: {e}", bin_path.display()))?;
    let got = sha256_hex(&bytes);
    if got != m.sha256 {
        return Err(format!(
            "gate pin mismatch — manifest sha256 {} != binary {} ({})",
            m.sha256,
            got,
            bin_path.display()
        ));
    }

    if m.knob == "vendored" {
        for (label, name) in [
            ("attestation", &m.attestation),
            ("source", &m.source),
            ("recipe", &m.recipe),
        ] {
            match name {
                None => {
                    return Err(format!(
                        "vendored gate manifest is missing the {label} field (ADR-038 §1)"
                    ));
                }
                Some(f) if !dir.join(f).exists() => {
                    return Err(format!("vendored gate {label} file not found: {f}"));
                }
                Some(_) => {}
            }
        }
    }

    Ok(m)
}

/// Write `.qx/gate/manifest.toml` pinning a just-fetched vendored gate. Pure
/// (no I/O beyond the one write) so the pin logic is testable without network.
/// `bin` is the gate binary bytes (its sha256 becomes the pin).
pub fn seed_manifest(
    dir: &Path,
    version: &str,
    bin: &[u8],
    attestation: &str,
    source: &str,
    recipe: &str,
) -> Result<GateManifest, String> {
    let m = GateManifest {
        knob: "vendored".into(),
        version: version.into(),
        binary: "qx".into(),
        sha256: sha256_hex(bin),
        attestation: Some(attestation.into()),
        source: Some(source.into()),
        recipe: Some(recipe.into()),
    };
    std::fs::write(dir.join("manifest.toml"), m.to_toml())
        .map_err(|e| format!("write manifest: {e}"))?;
    Ok(m)
}

/// `qx gate vendor <version>` (ADR-038 §1): fetch a released static musl gate
/// + provenance into `.qx/gate/` and pin it. Shells to `gh` (release download
/// + tarball/flake.lock from the tag), which also handles auth.
pub fn vendor(version: &str, dir: &Path, repo: &str) -> Result<GateManifest, String> {
    use std::process::Command;
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let dir_s = dir.to_str().ok_or("non-utf8 gate path")?;

    // 1. Binary + sha256 + sigstore signature/cert from the release.
    let out = Command::new("gh")
        .args([
            "release", "download", version, "--repo", repo, "--dir", dir_s, "--clobber",
            "--pattern", "qx-x86_64-unknown-linux-musl",
            "--pattern", "qx-x86_64-unknown-linux-musl.sig",
            "--pattern", "qx-x86_64-unknown-linux-musl.pem",
        ])
        .output()
        .map_err(|e| format!("run gh release download (is gh installed + authed?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "gh release download {version}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // 2. Nix recipe (flake.lock) at the tag → the rebuild path.
    let lock = Command::new("gh")
        .args([
            "api",
            &format!("repos/{repo}/contents/flake.lock?ref={version}"),
            "--jq",
            ".content",
        ])
        .output()
        .map_err(|e| format!("run gh api flake.lock: {e}"))?;
    if !lock.status.success() {
        return Err(format!(
            "fetch flake.lock@{version}: {}",
            String::from_utf8_lossy(&lock.stderr).trim()
        ));
    }
    let lock_b64: String = String::from_utf8_lossy(&lock.stdout)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    // GitHub returns base64 (RFC 4648) — decode with the stdlib-free path.
    let lock_bytes = base64_decode(&lock_b64).map_err(|e| format!("decode flake.lock: {e}"))?;
    std::fs::write(dir.join("flake.lock"), &lock_bytes).map_err(|e| e.to_string())?;

    // 3. Source tarball at the tag → repo-alone rebuild custody.
    let src_path = dir.join("qx-src.tar.gz");
    let src = Command::new("gh")
        .args(["api", &format!("repos/{repo}/tarball/{version}")])
        .output()
        .map_err(|e| format!("run gh api tarball: {e}"))?;
    if !src.status.success() {
        return Err(format!(
            "fetch source tarball@{version}: {}",
            String::from_utf8_lossy(&src.stderr).trim()
        ));
    }
    std::fs::write(&src_path, &src.stdout).map_err(|e| e.to_string())?;

    // 4. Rename the binary to `qx`, pin it in the manifest.
    std::fs::rename(dir.join("qx-x86_64-unknown-linux-musl"), dir.join("qx"))
        .map_err(|e| format!("rename gate binary: {e}"))?;
    let bin = std::fs::read(dir.join("qx")).map_err(|e| e.to_string())?;
    let m = seed_manifest(
        dir,
        version,
        &bin,
        "qx-x86_64-unknown-linux-musl.pem",
        "qx-src.tar.gz",
        "flake.lock",
    )?;
    // Sanity: the freshly-seeded gate must pin-verify.
    verify(dir)?;
    Ok(m)
}

/// Minimal RFC 4648 base64 decoder (GitHub Contents API payloads). No dep.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let val = |c: u8| T.iter().position(|&t| t == c).map(|p| p as u32);
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = val(c).ok_or_else(|| format!("bad base64 char {c:#x}"))?;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) {
        std::fs::write(dir.join(name), bytes).unwrap();
    }

    #[test]
    fn seed_manifest_pins_and_verifies() {
        let d = tempfile::tempdir().unwrap();
        let bin = b"gate-binary";
        write(d.path(), "qx", bin);
        write(d.path(), "qx-x86_64-unknown-linux-musl.pem", b"cert");
        write(d.path(), "qx-src.tar.gz", b"src");
        write(d.path(), "flake.lock", b"lock");
        let m = seed_manifest(
            d.path(),
            "v0.14.0",
            bin,
            "qx-x86_64-unknown-linux-musl.pem",
            "qx-src.tar.gz",
            "flake.lock",
        )
        .unwrap();
        assert_eq!(m.sha256, sha256_hex(bin));
        // the seeded manifest must pass the vendored-knob pin-verify
        assert_eq!(verify(d.path()).unwrap().version, "v0.14.0");
    }

    #[test]
    fn base64_decodes_github_payload() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn manifest_roundtrips() {
        let m = GateManifest {
            knob: "vendored".into(),
            version: "v0.1.0".into(),
            binary: "qx".into(),
            sha256: "abc".into(),
            attestation: Some("qx.intoto.jsonl".into()),
            source: Some("qx-src.tar.zst".into()),
            recipe: Some("flake.lock".into()),
        };
        assert_eq!(GateManifest::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn verify_accepts_a_matching_pin() {
        let d = tempfile::tempdir().unwrap();
        let bin = b"#!/bin/sh\necho gate\n";
        write(d.path(), "qx", bin);
        let m = GateManifest {
            knob: "fetched".into(),
            version: "v0.1.0".into(),
            binary: "qx".into(),
            sha256: sha256_hex(bin),
            attestation: None,
            source: None,
            recipe: None,
        };
        write(d.path(), "manifest.toml", m.to_toml().as_bytes());
        assert_eq!(verify(d.path()).unwrap().version, "v0.1.0");
    }

    #[test]
    fn verify_rejects_a_tampered_binary() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), "qx", b"original");
        let m = GateManifest {
            knob: "fetched".into(),
            version: "v0.1.0".into(),
            binary: "qx".into(),
            sha256: sha256_hex(b"original"),
            attestation: None,
            source: None,
            recipe: None,
        };
        write(d.path(), "manifest.toml", m.to_toml().as_bytes());
        // tamper AFTER the manifest was pinned
        write(d.path(), "qx", b"tampered!");
        let err = verify(d.path()).unwrap_err();
        assert!(err.contains("pin mismatch"), "{err}");
    }

    #[test]
    fn verify_requires_provenance_for_the_vendored_knob() {
        let d = tempfile::tempdir().unwrap();
        let bin = b"gate";
        write(d.path(), "qx", bin);
        let m = GateManifest {
            knob: "vendored".into(),
            version: "v0.1.0".into(),
            binary: "qx".into(),
            sha256: sha256_hex(bin),
            attestation: Some("qx.intoto.jsonl".into()),
            source: Some("qx-src.tar.zst".into()),
            recipe: Some("flake.lock".into()),
        };
        write(d.path(), "manifest.toml", m.to_toml().as_bytes());
        // provenance files absent → vendored verify must fail
        let err = verify(d.path()).unwrap_err();
        assert!(err.contains("not found"), "{err}");
        // add them → passes
        write(d.path(), "qx.intoto.jsonl", b"{}");
        write(d.path(), "qx-src.tar.zst", b"src");
        write(d.path(), "flake.lock", b"lock");
        assert!(verify(d.path()).is_ok());
    }
}
