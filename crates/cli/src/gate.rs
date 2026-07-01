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

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, bytes: &[u8]) {
        std::fs::write(dir.join(name), bytes).unwrap();
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
