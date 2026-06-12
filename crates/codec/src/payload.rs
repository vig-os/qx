//! Payload composition DSL — flat-list stage (ADR-031 §10, 2026-06-12).
//!
//! Stage 1 of the print-contract batch: payload is a flat,
//! whitespace-separated list of leaf elements along the layout axis,
//! with nested groups (`[h: …]`, `[v: …]`, `[c WxH: …]`) and
//! canvas-zone semantics deferred. The grammar:
//!
//! ```text
//! payload  := element (WS+ element)*
//! element  := qr[:TYPE] | id[:GROUPING|chars-N] | space[:SIZE]
//! ```
//!
//! - `qr` — a QR symbology block; `qr:micro-m3-l` pins the family,
//!   `qr` alone uses the request-level default (`--type`).
//! - `id` — the human-id text block; `id:44`/`id:444`/`id:554` declares
//!   the grouping vector, `id:chars-N` declares HOW MANY of the id
//!   characters to show. Without a param the request-level
//!   `--chars`/`--id-chars` defaults win.
//! - `space:Npx` / `space:Nmm` / `space:N` — a blank gap of the given
//!   size along the layout axis. Bare `space` is a flex spacer (not
//!   wired in stage 1; bare reads as 0px today).
//!
//! Element params > global flags > contract defaults (§10). The order
//! of elements in the string IS the order on the axis. ANY nesting
//! bracket (`[`) is rejected with the staged error message so the
//! grammar's recursion never feels accidentally enabled.
//!
//! This module owns ONE parser, used by both the engine (request →
//! resolved tree) and the metadata receipt (canonical string back into
//! the SVG, ADR-031 §10 last bullet).

use serde::{Deserialize, Serialize};

/// One element on the flat payload axis (stage 1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Element {
    /// `qr` or `qr:TYPE` — the symbology family/version/EC pin (the
    /// canonical compact form parsed downstream by [`crate::Symbology`]).
    /// `None` falls back to the request-level `--type`.
    Qr {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        symbology: Option<String>,
    },
    /// `id`, `id:GROUPING` (44/444/554), or `id:chars-N` — the human-id
    /// text block. `grouping` declares the row split; `id_chars`
    /// declares how many id characters to show. Either may be `None`,
    /// in which case the request-level `--chars`/`--id-chars` defaults
    /// win (§10: element params > global flags > defaults).
    Id {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grouping: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id_chars: Option<u32>,
    },
    /// `space`, `space:Npx`, `space:Nmm`, or `space:N` (bare = px) —
    /// a blank gap along the layout axis. `None` reads as a flex
    /// spacer (stage 2 wires the box engine; today it reads as 0).
    Space {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<SpaceSize>,
    },
}

/// Sized spacer — the unit rides the value like `--size` (ADR-031 §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "lowercase")]
pub enum SpaceSize {
    /// `space:8px` — integer device px.
    Px { px: u32 },
    /// `space:8mm` — millimetres (mm rides the value as an integer the
    /// stage-1 grammar accepts; fractional mm lands with the box engine).
    Mm { mm: u32 },
}

/// The resolved payload (stage 1: a flat list with the canonical string
/// — what ships in the metadata receipt, ADR-031 §10 last bullet).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub elements: Vec<Element>,
    /// The canonical payload string (params resolved against the
    /// request defaults; the form the response receipt + SVG metadata
    /// both carry).
    pub canonical: String,
}

/// Parse a flat-list payload string. Rejects nesting (`[` anywhere)
/// with the staged error per ADR-031 §10.
pub fn parse(input: &str) -> Result<Vec<Element>, String> {
    if input.contains('[') || input.contains(']') {
        return Err("payload: staged: nesting not yet enabled (ADR-031 §10) — \
             flat list only (qr|id|space, whitespace-separated)"
            .into());
    }
    let mut out = Vec::new();
    for token in input.split_whitespace() {
        out.push(parse_element(token)?);
    }
    Ok(out)
}

fn parse_element(token: &str) -> Result<Element, String> {
    let (head, param) = match token.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (token, None),
    };
    match head {
        "qr" => Ok(Element::Qr {
            symbology: param.map(str::to_string),
        }),
        "id" => parse_id(param),
        "space" => parse_space(param),
        other => Err(format!(
            "payload: unknown element {other:?} (stage 1 supports: qr, id, space)"
        )),
    }
}

fn parse_id(param: Option<&str>) -> Result<Element, String> {
    let Some(p) = param else {
        return Ok(Element::Id {
            grouping: None,
            id_chars: None,
        });
    };
    // `chars-N` declares the id-char budget; everything else is a
    // grouping vector (44 / 444 / 554 — validated by the engine
    // against the descriptor's declared groupings).
    if let Some(n) = p.strip_prefix("chars-") {
        let n: u32 = n
            .parse()
            .map_err(|_| format!("payload: id:chars-{n:?}: expected a whole number"))?;
        return Ok(Element::Id {
            grouping: None,
            id_chars: Some(n),
        });
    }
    Ok(Element::Id {
        grouping: Some(p.to_string()),
        id_chars: None,
    })
}

fn parse_space(param: Option<&str>) -> Result<Element, String> {
    let Some(p) = param else {
        return Ok(Element::Space { size: None });
    };
    if let Some(px) = p.strip_suffix("px") {
        let n: u32 = px
            .parse()
            .map_err(|_| format!("payload: space:{p:?}: px sizes are whole device px"))?;
        return Ok(Element::Space {
            size: Some(SpaceSize::Px { px: n }),
        });
    }
    if let Some(mm) = p.strip_suffix("mm") {
        let n: u32 = mm
            .parse()
            .map_err(|_| format!("payload: space:{p:?}: mm sizes are whole mm in stage 1"))?;
        return Ok(Element::Space {
            size: Some(SpaceSize::Mm { mm: n }),
        });
    }
    // Bare number = px (matching `--size` bare = mm but spacer-bare =
    // px, the stage-1 default for inter-element gaps).
    let n: u32 = p
        .parse()
        .map_err(|_| format!("payload: space:{p:?}: expected <N>[px|mm]"))?;
    Ok(Element::Space {
        size: Some(SpaceSize::Px { px: n }),
    })
}

/// Render an element back into its canonical string form.
pub fn element_canonical(e: &Element) -> String {
    match e {
        Element::Qr { symbology: None } => "qr".into(),
        Element::Qr {
            symbology: Some(s), ..
        } => format!("qr:{s}"),
        Element::Id {
            grouping: None,
            id_chars: None,
        } => "id".into(),
        Element::Id {
            grouping: Some(g), ..
        } => format!("id:{g}"),
        Element::Id {
            id_chars: Some(n), ..
        } => format!("id:chars-{n}"),
        Element::Space { size: None } => "space".into(),
        Element::Space {
            size: Some(SpaceSize::Px { px }),
        } => format!("space:{px}px"),
        Element::Space {
            size: Some(SpaceSize::Mm { mm }),
        } => format!("space:{mm}mm"),
    }
}

/// Canonical-form string for the whole element list (ADR-031 §10:
/// "resolved payload string (canonical form, params resolved) goes in
/// the response receipt").
pub fn canonicalize(elements: &[Element]) -> String {
    elements
        .iter()
        .map(element_canonical)
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_qr_id() {
        let p = parse("qr id").unwrap();
        assert_eq!(
            p,
            vec![
                Element::Qr { symbology: None },
                Element::Id {
                    grouping: None,
                    id_chars: None,
                },
            ]
        );
        assert_eq!(canonicalize(&p), "qr id");
    }

    #[test]
    fn parses_id_qr_reverses_order() {
        let p = parse("id qr").unwrap();
        assert_eq!(
            p,
            vec![
                Element::Id {
                    grouping: None,
                    id_chars: None,
                },
                Element::Qr { symbology: None },
            ]
        );
    }

    #[test]
    fn parses_id_only() {
        let p = parse("id").unwrap();
        assert_eq!(
            p,
            vec![Element::Id {
                grouping: None,
                id_chars: None,
            }]
        );
    }

    #[test]
    fn parses_qr_only() {
        let p = parse("qr").unwrap();
        assert_eq!(p, vec![Element::Qr { symbology: None }]);
    }

    #[test]
    fn parses_element_params() {
        let p = parse("qr:micro-m3-l id:554 space:8px").unwrap();
        assert_eq!(
            p,
            vec![
                Element::Qr {
                    symbology: Some("micro-m3-l".into()),
                },
                Element::Id {
                    grouping: Some("554".into()),
                    id_chars: None,
                },
                Element::Space {
                    size: Some(SpaceSize::Px { px: 8 }),
                },
            ]
        );
    }

    #[test]
    fn parses_id_chars_n() {
        let p = parse("id:chars-8").unwrap();
        assert_eq!(
            p,
            vec![Element::Id {
                grouping: None,
                id_chars: Some(8),
            }]
        );
        assert_eq!(canonicalize(&p), "id:chars-8");
    }

    #[test]
    fn space_units() {
        let p = parse("space:5px space:3mm space:4").unwrap();
        assert_eq!(
            p,
            vec![
                Element::Space {
                    size: Some(SpaceSize::Px { px: 5 }),
                },
                Element::Space {
                    size: Some(SpaceSize::Mm { mm: 3 }),
                },
                Element::Space {
                    size: Some(SpaceSize::Px { px: 4 }),
                },
            ]
        );
    }

    #[test]
    fn rejects_nesting_with_staged_message() {
        let cases = ["[h: qr id]", "qr [v: id]", "qr]", "[c 64x64: qr@(0,0)]"];
        for input in cases {
            let err = parse(input).expect_err(input);
            assert!(
                err.contains("staged: nesting not yet enabled"),
                "input {input:?} err {err:?}",
            );
            assert!(err.contains("ADR-031 §10"));
        }
    }

    #[test]
    fn rejects_unknown_element() {
        let err = parse("foo").expect_err("unknown element");
        assert!(err.contains("unknown element"), "got: {err}");
    }

    #[test]
    fn empty_payload_is_empty_vec() {
        assert_eq!(parse("").unwrap(), Vec::<Element>::new());
        assert_eq!(parse("   ").unwrap(), Vec::<Element>::new());
    }
}
