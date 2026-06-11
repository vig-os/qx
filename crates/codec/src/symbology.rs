//! Symbology type grammar + auto-fit resolution (ADR-031 §8,
//! 2026-06-11: print contracts).
//!
//! ONE parser owns the canonical compact string the CLI, the wire, and
//! the response labels all speak: `<family>[-<version>][-<ec>]` —
//! `micro`, `micro-m4`, `micro-m3-l`, `qr`, `qr-v1-m`. Version and EC
//! level are **contract parameters, not hardcodes**: when either is
//! unspecified, [`Symbology::resolve`] auto-fits against the actual
//! payload — strongest feasible EC on the contract-exposed ladder
//! (`m` > `l`, per §8 "expose ec: l|m"), then the smallest feasible
//! version at that EC. For the nano14 payload that lands on the
//! pre-contract defaults (`micro` → `micro-m4-m`, `qr` → `qr-v1-m`),
//! so the deprecated `micro` flag keeps its exact geometry.
//!
//! Feasibility is decided by the encoder itself (try-encode is the
//! SSOT — no capacity table to drift): an infeasible pin errors with
//! the feasible space for the payload, e.g. `micro-m4-q` over 14
//! alnum chars → "M4-Q caps at 13 alnum chars; feasible for this
//! payload: micro-m4-l, micro-m4-m, micro-m3-l".

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::qr::{encode_pinned, QrMatrix, QR_BORDER_MICRO, QR_BORDER_STANDARD};
use crate::CodecError;

/// Symbology family. `dm` (Data Matrix) and `code128` are documented
/// future families (ADR-031 §8) — the parser names them in its error
/// hint but they are not implemented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    /// Micro QR (versions M1–M4, quiet zone 2 modules).
    Micro,
    /// Standard QR (versions V1–V40, quiet zone 4 modules).
    Qr,
}

impl Family {
    fn name(self) -> &'static str {
        match self {
            Family::Micro => "micro",
            Family::Qr => "qr",
        }
    }

    fn version_prefix(self) -> char {
        match self {
            Family::Micro => 'm',
            Family::Qr => 'v',
        }
    }

    fn version_max(self) -> u8 {
        match self {
            Family::Micro => 4,
            Family::Qr => 40,
        }
    }

    /// Quiet-zone width in modules (ISO/IEC 18004 §6.3.8) — the
    /// family's contribution to the one §8 deduction engine.
    pub fn quiet_modules(self) -> u32 {
        match self {
            Family::Micro => QR_BORDER_MICRO as u32,
            Family::Qr => QR_BORDER_STANDARD as u32,
        }
    }

    /// Human version label for error messages: `M4` / `V1`.
    fn version_label(self, version: u8) -> String {
        format!("{}{version}", self.version_prefix().to_ascii_uppercase())
    }
}

/// Error-correction level. The contract-exposed auto ladder is
/// `l|m` (ADR-031 §8); `q` and `h` are pin-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ec {
    L,
    M,
    Q,
    H,
}

impl Ec {
    fn name(self) -> &'static str {
        match self {
            Ec::L => "l",
            Ec::M => "m",
            Ec::Q => "q",
            Ec::H => "h",
        }
    }
}

/// A possibly-underspecified symbology request: family always known,
/// version/EC auto-fit by [`Symbology::resolve`] when `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Symbology {
    pub family: Family,
    pub version: Option<u8>,
    pub ec: Option<Ec>,
}

impl Symbology {
    /// Family-only request (version + EC auto-fit).
    pub fn family(family: Family) -> Self {
        Self {
            family,
            version: None,
            ec: None,
        }
    }
}

impl fmt::Display for Symbology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.family.name())?;
        if let Some(v) = self.version {
            write!(f, "-{}{v}", self.family.version_prefix())?;
        }
        if let Some(e) = self.ec {
            write!(f, "-{}", e.name())?;
        }
        Ok(())
    }
}

/// A fully pinned symbology — what a render actually used. Response
/// labels carry [`ResolvedSymbology::compact`] (e.g. `micro-m4-m`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSymbology {
    pub family: Family,
    pub version: u8,
    pub ec: Ec,
}

impl ResolvedSymbology {
    /// The canonical compact string: `micro-m4-m`, `qr-v1-m`, …
    pub fn compact(&self) -> String {
        format!(
            "{}-{}{}-{}",
            self.family.name(),
            self.family.version_prefix(),
            self.version,
            self.ec.name()
        )
    }

    /// Quiet-zone width in modules for the §8 deduction.
    pub fn quiet_modules(&self) -> u32 {
        self.family.quiet_modules()
    }
}

impl fmt::Display for ResolvedSymbology {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.compact())
    }
}

// -------------------------------------------------------------------
// The one parser
// -------------------------------------------------------------------

const FAMILY_HINT: &str = "implemented families: micro, qr (dm and code128 are documented \
     future families, ADR-031 §8)";

impl FromStr for Symbology {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let compact = s.trim().to_ascii_lowercase();
        let mut segments = compact.split('-');
        let family = match segments.next().unwrap_or("") {
            "micro" => Family::Micro,
            "qr" => Family::Qr,
            future @ ("dm" | "code128") => {
                return Err(format!(
                    "symbology family {future:?} is documented but not implemented yet; \
                     {FAMILY_HINT}"
                ));
            }
            other => {
                return Err(format!("unknown symbology family {other:?}; {FAMILY_HINT}"));
            }
        };
        let rest: Vec<&str> = segments.collect();
        if rest.len() > 2 {
            return Err(format!(
                "malformed symbology {s:?}: expected <family>[-<version>][-<ec>] \
                 (e.g. micro, micro-m3-l, qr-v1-m)"
            ));
        }
        let mut version: Option<u8> = None;
        let mut ec: Option<Ec> = None;
        for (i, token) in rest.iter().enumerate() {
            if let Some(e) = parse_ec(token) {
                if i + 1 != rest.len() {
                    return Err(format!(
                        "malformed symbology {s:?}: the EC level must come last \
                         (<family>[-<version>][-<ec>])"
                    ));
                }
                ec = Some(e);
            } else if i == 0 {
                version = Some(parse_version(token, family)?);
            } else {
                return Err(format!(
                    "malformed symbology {s:?}: {token:?} is not an EC level (l, m, q, h)"
                ));
            }
        }
        let sym = Symbology {
            family,
            version,
            ec,
        };
        validate_combination(&sym, s)?;
        Ok(sym)
    }
}

fn parse_ec(token: &str) -> Option<Ec> {
    match token {
        "l" => Some(Ec::L),
        "m" => Some(Ec::M),
        "q" => Some(Ec::Q),
        "h" => Some(Ec::H),
        _ => None,
    }
}

fn parse_version(token: &str, family: Family) -> Result<u8, String> {
    let prefix = family.version_prefix();
    let digits = token.strip_prefix(prefix).ok_or_else(|| {
        format!(
            "{family} versions are written {prefix}1..{prefix}{max} (got {token:?})",
            family = family.name(),
            max = family.version_max(),
        )
    })?;
    let v: u8 = digits.parse().map_err(|_| {
        format!(
            "{family} versions are written {prefix}1..{prefix}{max} (got {token:?})",
            family = family.name(),
            max = family.version_max(),
        )
    })?;
    if v < 1 || v > family.version_max() {
        return Err(format!(
            "{family} version {prefix}{v} out of range ({prefix}1..{prefix}{max})",
            family = family.name(),
            max = family.version_max(),
        ));
    }
    Ok(v)
}

/// Structural micro-QR constraints the encoder would only reject with
/// an opaque error: M1 has no selectable EC; Q exists only at M4; H
/// does not exist in the micro family at all.
fn validate_combination(sym: &Symbology, input: &str) -> Result<(), String> {
    if sym.family != Family::Micro {
        return Ok(());
    }
    match (sym.version, sym.ec) {
        (_, Some(Ec::H)) => Err(format!(
            "micro QR has no EC level h (got {input:?}); micro levels: l, m, q (q only at m4)"
        )),
        (Some(1), Some(_)) => Err(format!(
            "micro-m1 has no selectable EC level (got {input:?}); pick m2..m4"
        )),
        (Some(v @ 2..=3), Some(Ec::Q)) => Err(format!(
            "EC level q requires micro-m4 (got micro-m{v}-q); m2/m3 levels: l, m"
        )),
        _ => Ok(()),
    }
}

// -------------------------------------------------------------------
// Auto-fit resolution — feasibility decided by the encoder (SSOT)
// -------------------------------------------------------------------

/// The contract-exposed auto-fit EC ladder, strongest first
/// (ADR-031 §8: "expose `ec: l|m`"); q/h stay pin-only.
const AUTO_EC_LADDER: [Ec; 2] = [Ec::M, Ec::L];

/// QR alphanumeric-mode charset (ISO/IEC 18004 table 5).
fn is_qr_alnum(payload: &str) -> bool {
    payload
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase() || " $%*+-./:".contains(c))
}

impl Symbology {
    /// Resolve version/EC against the payload and encode.
    ///
    /// Pinned parameters are honored as-is; unspecified ones auto-fit:
    /// the strongest feasible EC on the exposed `m|l` ladder, then the
    /// smallest feasible version at that EC. Infeasible requests error
    /// ([`CodecError::Encode`]) with the feasible space for this
    /// payload so the caller can re-pin.
    pub fn resolve(&self, payload: &str) -> Result<(ResolvedSymbology, QrMatrix), CodecError> {
        let ladder: Vec<Ec> = match self.ec {
            Some(e) => vec![e],
            None => AUTO_EC_LADDER.to_vec(),
        };
        let versions: Vec<u8> = match self.version {
            Some(v) => vec![v],
            None => (1..=self.family.version_max()).collect(),
        };
        let mut pinned_failure: Option<CodecError> = None;
        for &ec in &ladder {
            for &version in &versions {
                match encode_pinned(payload, self.family, version, ec) {
                    Ok(matrix) => {
                        return Ok((
                            ResolvedSymbology {
                                family: self.family,
                                version,
                                ec,
                            },
                            matrix,
                        ));
                    }
                    Err(e) => {
                        if self.version.is_some() && self.ec.is_some() {
                            pinned_failure = Some(e);
                        }
                    }
                }
            }
        }
        Err(CodecError::Encode(
            self.infeasible_message(payload, pinned_failure),
        ))
    }

    fn infeasible_message(&self, payload: &str, pinned_failure: Option<CodecError>) -> String {
        let len = payload.chars().count();
        let alnum = is_qr_alnum(payload);
        let unit = if alnum { "alnum chars" } else { "chars" };
        let head = match (self.version, self.ec) {
            (Some(v), Some(e)) => match capacity_probe(self.family, v, e, alnum, len) {
                Some(cap) => format!(
                    "{}-{} caps at {cap} {unit}",
                    self.family.version_label(v),
                    e.name().to_ascii_uppercase()
                ),
                None => format!(
                    "{self} cannot encode this payload{}",
                    pinned_failure
                        .map(|err| format!(" ({err})"))
                        .unwrap_or_default()
                ),
            },
            _ => format!("{self} cannot hold this {len}-char payload ({unit})"),
        };
        let feasible = feasible_space(self.family, payload);
        if feasible.is_empty() {
            let escape = match self.family {
                Family::Micro => "; nothing in the micro family fits — try qr",
                Family::Qr => "; nothing in the qr family fits this payload",
            };
            format!("{head}{escape}")
        } else {
            format!("{head}; feasible for this payload: {}", feasible.join(", "))
        }
    }
}

/// Largest payload length below `len` the pinned (version, EC) still
/// encodes — probed against the encoder itself so the number can never
/// drift from reality. `None` when the combination encodes nothing.
fn capacity_probe(family: Family, version: u8, ec: Ec, alnum: bool, len: usize) -> Option<usize> {
    // 'A' keeps the probe in alphanumeric mode; 'a' forces byte mode,
    // matching how the real payload would encode.
    let fill = if alnum { 'A' } else { 'a' };
    (1..len)
        .rev()
        .find(|&n| encode_pinned(&fill.to_string().repeat(n), family, version, ec).is_ok())
}

/// The feasible (version, EC) space for this payload on the exposed
/// `l|m` ladder, spanning the smallest feasible version and one step
/// up (headroom), largest version first — for nano14 over micro this
/// is exactly `micro-m4-l, micro-m4-m, micro-m3-l`.
fn feasible_space(family: Family, payload: &str) -> Vec<String> {
    let fits = |version: u8, ec: Ec| encode_pinned(payload, family, version, ec).is_ok();
    let Some(lo) = (1..=family.version_max()).find(|&v| fits(v, Ec::L)) else {
        return Vec::new();
    };
    let hi = lo.saturating_add(1).min(family.version_max());
    let mut out = Vec::new();
    for version in (lo..=hi).rev() {
        for ec in [Ec::L, Ec::M] {
            if fits(version, ec) {
                out.push(
                    ResolvedSymbology {
                        family,
                        version,
                        ec,
                    }
                    .compact(),
                );
            }
        }
    }
    out
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed nano14 fixture (14 alphanumeric chars).
    const FIXED_ID: &str = "K7M3PQ9RT5VAXY";

    fn sym(s: &str) -> Symbology {
        s.parse().expect("parses")
    }

    fn resolved(s: &str, payload: &str) -> String {
        sym(s).resolve(payload).expect("resolves").0.compact()
    }

    // ---------- the one parser: valid forms ----------

    #[test]
    fn parser_accepts_the_canonical_compact_grammar() {
        let cases = [
            ("micro", Family::Micro, None, None),
            ("micro-m4", Family::Micro, Some(4), None),
            ("micro-m3-l", Family::Micro, Some(3), Some(Ec::L)),
            ("micro-m4-q", Family::Micro, Some(4), Some(Ec::Q)),
            ("micro-l", Family::Micro, None, Some(Ec::L)),
            ("micro-m", Family::Micro, None, Some(Ec::M)),
            ("qr", Family::Qr, None, None),
            ("qr-v1", Family::Qr, Some(1), None),
            ("qr-v1-m", Family::Qr, Some(1), Some(Ec::M)),
            ("qr-v40-h", Family::Qr, Some(40), Some(Ec::H)),
            ("QR-V1-M", Family::Qr, Some(1), Some(Ec::M)), // case-insensitive
        ];
        for (input, family, version, ec) in cases {
            assert_eq!(
                sym(input),
                Symbology {
                    family,
                    version,
                    ec
                },
                "input {input:?}"
            );
        }
    }

    #[test]
    fn parser_rejects_malformed_and_unknown_inputs() {
        let cases = [
            ("datamatrix", "unknown symbology family"),
            ("", "unknown symbology family"),
            ("micro-x9", "micro versions are written m1..m4"),
            ("micro-m5", "out of range"),
            ("micro-v1", "micro versions are written m1..m4"),
            ("qr-m4", "qr versions are written v1..v40"),
            ("qr-v0", "out of range"),
            ("qr-v41", "out of range"),
            ("micro-l-m4", "must come last"),
            ("micro-m4-l-q", "expected <family>[-<version>][-<ec>]"),
            ("micro-m4-x", "not an EC level"),
            ("micro-m4-h", "micro QR has no EC level h"),
            ("micro-h", "micro QR has no EC level h"),
            ("micro-m1-l", "micro-m1 has no selectable EC level"),
            ("micro-m3-q", "EC level q requires micro-m4"),
        ];
        for (input, expected) in cases {
            let err = input.parse::<Symbology>().expect_err(input);
            assert!(err.contains(expected), "{input:?}: got {err:?}");
        }
    }

    #[test]
    fn future_families_get_a_documented_hint() {
        for input in ["dm", "code128"] {
            let err = input.parse::<Symbology>().expect_err(input);
            assert!(err.contains("not implemented yet"), "got {err:?}");
            assert!(err.contains("micro, qr"), "got {err:?}");
        }
    }

    // ---------- auto-fit: strongest exposed EC, smallest version ----------

    #[test]
    fn auto_fit_preserves_the_pre_contract_defaults() {
        // micro=true used to mean M4/EC-M; bare qr meant V1/EC-M.
        assert_eq!(resolved("micro", FIXED_ID), "micro-m4-m");
        assert_eq!(resolved("qr", FIXED_ID), "qr-v1-m");
    }

    #[test]
    fn auto_fit_with_pinned_version_or_ec() {
        // M3 pinned: M caps at 11 alnum, so EC auto-falls to L.
        assert_eq!(resolved("micro-m3", FIXED_ID), "micro-m3-l");
        assert_eq!(resolved("micro-m4", FIXED_ID), "micro-m4-m");
        // EC pinned, version auto: smallest version that fits at L is M3.
        assert_eq!(resolved("micro-l", FIXED_ID), "micro-m3-l");
        // q is pin-only: 13 alnum chars fit M4-Q.
        assert_eq!(resolved("micro-q", "K7M3PQ9RT5VAX"), "micro-m4-q");
        assert_eq!(resolved("qr-v1", FIXED_ID), "qr-v1-m");
    }

    #[test]
    fn m3_l_encodes_exactly_14_alnum_chars() {
        // The ADR-031 §8 claim: M3-L caps at exactly 14 — the nano14
        // payload fits, 15 chars do not.
        let (r, matrix) = sym("micro-m3-l").resolve(FIXED_ID).expect("14 fits M3-L");
        assert_eq!(r.compact(), "micro-m3-l");
        assert_eq!(matrix.size, 15, "M3 is 15 data modules");
        assert_eq!(r.quiet_modules(), 2);
        let err = sym("micro-m3-l")
            .resolve("K7M3PQ9RT5VAXYZ")
            .expect_err("15 alnum chars exceed M3-L");
        assert!(
            err.to_string().contains("M3-L caps at 14 alnum chars"),
            "got: {err}"
        );
    }

    #[test]
    fn resolved_matrix_sizes_match_the_symbology() {
        let cases = [
            ("micro", 17, 2), // M4
            ("micro-m3-l", 15, 2),
            ("qr", 21, 4), // V1
            ("qr-v2", 25, 4),
        ];
        for (input, data, quiet) in cases {
            let (r, matrix) = sym(input).resolve(FIXED_ID).expect(input);
            assert_eq!(matrix.size, data, "{input}: data modules");
            assert_eq!(r.quiet_modules(), quiet, "{input}: quiet modules");
        }
    }

    // ---------- infeasible pins: the feasibility list ----------

    #[test]
    fn infeasible_pin_errors_with_the_feasible_space() {
        let err = sym("micro-m4-q")
            .resolve(FIXED_ID)
            .expect_err("M4-Q caps at 13");
        let msg = err.to_string();
        assert!(matches!(err, CodecError::Encode(_)), "got {err:?}");
        assert!(
            msg.contains("M4-Q caps at 13 alnum chars"),
            "cap named, got: {msg}"
        );
        assert!(
            msg.contains("feasible for this payload: micro-m4-l, micro-m4-m, micro-m3-l"),
            "feasible space listed, got: {msg}"
        );
    }

    #[test]
    fn infeasible_version_pin_names_the_feasible_space() {
        // M2 caps far below 14 at every EC.
        let err = sym("micro-m2").resolve(FIXED_ID).expect_err("M2 too small");
        let msg = err.to_string();
        assert!(msg.contains("micro-m2"), "pin named, got: {msg}");
        assert!(msg.contains("micro-m3-l"), "feasible space, got: {msg}");
    }

    #[test]
    fn payload_too_big_for_the_whole_family_points_at_qr() {
        // 22 alnum chars exceed even M4-L (21).
        let err = sym("micro")
            .resolve("K7M3PQ9RT5VAXYK7M3PQ9R")
            .expect_err("exceeds micro entirely");
        let msg = err.to_string();
        assert!(msg.contains("try qr"), "escape hint, got: {msg}");
    }
}
