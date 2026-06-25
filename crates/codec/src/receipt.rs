//! Print artifact receipt — SSOT for the response payload + the SVG
//! `<metadata>` element (ADR-031 §10, 2026-06-12).
//!
//! The same JSON object rides the protocol response AND the SVG
//! `<metadata>` element, built once and used twice (single
//! constructor = single source of truth). The receipt is BYTE-
//! REPRODUCIBLE: no timestamp, no host fields — only what the render
//! resolved.

use serde::{Deserialize, Serialize};

/// What an artifact carries — the resolved render parameters,
/// keyed on stable wire field names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    /// Canonical payload string (params resolved against the request
    /// defaults).
    pub payload: String,
    /// Resolved symbology in canonical compact form (`micro-m4-m`, …).
    pub symbology: String,
    /// EXACT output canvas (controlling dimension in device px).
    pub size_px: u32,
    /// Per-side padding floors as `[top, right, bottom, left]`.
    pub padding: [u32; 4],
    /// `"overlap"` | `"additive"` | `"clip"` — see [`crate::PaddingMode`].
    pub padding_mode: String,
    /// `"exact"` (canvas held at size) | `"snap"` (canvas snaps down).
    pub size_mode: String,
    pub qr_px: u32,
    pub module_px: u32,
    pub glyph_px: u32,
    /// Foreground color, canonical SVG form.
    pub fg: String,
    /// Background color, canonical SVG form (`"none"` = no bg rect).
    pub bg: String,
    /// Always `"nx75"` — the qx anchor font (ADR-031 §8
    /// final typography).
    pub font: String,
    /// `concat("qx ", env!("CARGO_PKG_VERSION"))` — the codec's
    /// version stamp.
    pub generator: String,
    /// Resolved §10 repeat geometry — present only on composed strips
    /// (`--repeat` and friends); absent on single-label artifacts so
    /// their receipts stay byte-identical to pre-repeat output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<crate::repeat::RepeatResolved>,
}

/// Codec version stamp used by [`Receipt::generator`]. Includes the
/// "qx " prefix so the metadata identifies this binary explicitly.
pub fn generator() -> String {
    format!("qx {}", env!("CARGO_PKG_VERSION"))
}

/// XML-escape a string for embedding inside `<metadata>`. The receipt
/// is JSON inside a `<![CDATA[…]]>` block, so the only thing we need
/// to guard against is a literal `]]>` sequence — split it across two
/// CDATA sections.
pub fn cdata_escape(s: &str) -> String {
    s.replace("]]>", "]]]]><![CDATA[>")
}

/// Render the receipt into a `<metadata>` element body for inscription
/// into the artifact SVG. The body is a CDATA-wrapped JSON object.
pub fn metadata_element(r: &Receipt) -> String {
    let json = serde_json::to_string(r).expect("receipt serializes");
    format!(
        "<metadata type=\"application/json\"><![CDATA[{}]]></metadata>",
        cdata_escape(&json),
    )
}

/// Inverse of [`metadata_element`]: extract the receipt JSON from an
/// SVG document. Returns `None` when the artifact carries no metadata.
pub fn extract_metadata(svg: &str) -> Option<Receipt> {
    let start = svg.find("<metadata")?;
    let rest = &svg[start..];
    let open_end = rest.find('>')? + 1;
    let body_start = start + open_end;
    let close = svg[body_start..].find("</metadata>")?;
    let mut body = &svg[body_start..body_start + close];
    // Strip CDATA wrappers — we may have multiple from the
    // `]]>` escape.
    let mut joined = String::new();
    while let Some(cd_start) = body.find("<![CDATA[") {
        joined.push_str(&body[..cd_start]);
        let after = &body[cd_start + "<![CDATA[".len()..];
        let cd_end = after.find("]]>")?;
        joined.push_str(&after[..cd_end]);
        body = &after[cd_end + "]]>".len()..];
    }
    joined.push_str(body);
    serde_json::from_str(&joined).ok()
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Receipt {
        Receipt {
            id: "23456789ABCDEF".into(),
            payload: "qr id".into(),
            symbology: "micro-m4-m".into(),
            size_px: 64,
            padding: [2, 2, 2, 2],
            padding_mode: "overlap".into(),
            size_mode: "exact".into(),
            qr_px: 63,
            module_px: 3,
            glyph_px: 3,
            fg: "black".into(),
            bg: "white".into(),
            font: "nx75".into(),
            generator: "qx 0.1.0".into(),
            repeat: None,
        }
    }

    #[test]
    fn metadata_roundtrip() {
        let r = sample();
        let body = metadata_element(&r);
        let svg = format!("<svg>{body}</svg>");
        let r2 = extract_metadata(&svg).expect("parses");
        assert_eq!(r2, r);
    }

    #[test]
    fn cdata_escape_handles_embedded_terminator() {
        // The terminator "]]>" appearing in user data would prematurely
        // close the CDATA section. The escape splits it across two
        // sections — the result contains the open marker `<![CDATA[`
        // so the parser sees two CDATA chunks instead of one.
        let escaped = cdata_escape("ok ]]> still");
        assert!(
            escaped.contains("<![CDATA["),
            "escape must split via new CDATA: {escaped}"
        );
        // The roundtrip through a wrapping CDATA element preserves
        // the original payload exactly.
        let wrapped = format!("<![CDATA[{escaped}]]>");
        // Strip alternating CDATA wrappers — emulates a parser.
        let mut s = wrapped.as_str();
        let mut joined = String::new();
        while let Some(start) = s.find("<![CDATA[") {
            joined.push_str(&s[..start]);
            s = &s[start + "<![CDATA[".len()..];
            let end = s.find("]]>").unwrap();
            joined.push_str(&s[..end]);
            s = &s[end + "]]>".len()..];
        }
        joined.push_str(s);
        assert_eq!(joined, "ok ]]> still");
    }

    #[test]
    fn receipt_serializes_with_stable_keys() {
        let r = sample();
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        for k in [
            "id",
            "payload",
            "symbology",
            "size_px",
            "padding",
            "padding_mode",
            "size_mode",
            "qr_px",
            "module_px",
            "glyph_px",
            "fg",
            "bg",
            "font",
            "generator",
        ] {
            assert!(v.get(k).is_some(), "missing key {k}");
        }
    }
}
