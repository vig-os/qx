//! Color parsing + the contrast/polarity WARN tiers (ADR-031 §10
//! 2026-06-12).
//!
//! `--fg <color>` / `--bg <color|none>` accept four forms passed
//! through verbatim to SVG:
//!
//! - `#RGB` / `#RRGGBB` / `#RRGGBBAA` — CSS hex.
//! - `rgb(r,g,b)` — CSS functional, integers in 0..=255.
//! - lowercase ascii names (`black`, `white`, `red`, …) — passed
//!   through; we don't ship a colour-name table.
//! - `none` for `bg` only — omits the background rect (the existing
//!   px renderer's accidental transparency becomes the EXPLICIT
//!   surface-dependent escape).
//!
//! Relative luminance comes off hex + `rgb(…)` parses for the contrast
//! tier; names are pass-through (no table), so we never raise a
//! contrast WARN against an unknown polarity — but `bg=none` always
//! warns "surface-dependent".

/// What an `--fg` / `--bg` value parses to. Lifetime-free so it rides
/// through the request → render → metadata path verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Color {
    /// The canonical form to inscribe in the SVG (`fill="…"`). For
    /// `bg=none` callers branch on [`Color::is_none`] and omit the rect.
    pub svg: String,
    /// Optional sRGB triple, used by [`relative_luminance`]. `None`
    /// for opaque names ("red") and for `bg=none` — contrast/polarity
    /// warnings skip those gracefully.
    pub rgb: Option<[u8; 3]>,
    /// `bg=none` — caller must omit the background rect.
    pub none: bool,
}

impl Color {
    /// `bg=none` — the rect is omitted.
    pub fn is_none(&self) -> bool {
        self.none
    }
}

/// Parse a color value. `allow_none` is true ONLY for `--bg` (the §10
/// `"none"` escape for surface-dependent printing).
pub fn parse(input: &str, allow_none: bool) -> Result<Color, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err("color: expected #RGB | #RRGGBB | #RRGGBBAA | rgb(r,g,b) | name | none".into());
    }
    if s.eq_ignore_ascii_case("none") {
        if !allow_none {
            return Err(
                "color: \"none\" is only valid for --bg (omits the background rect)".into(),
            );
        }
        return Ok(Color {
            svg: "none".into(),
            rgb: None,
            none: true,
        });
    }
    if let Some(rest) = s.strip_prefix('#') {
        return parse_hex(rest, s);
    }
    if let Some(rest) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        return parse_rgb(rest, s);
    }
    if is_lowercase_name(s) {
        return Ok(Color {
            svg: s.into(),
            rgb: None,
            none: false,
        });
    }
    Err(format!(
        "color: {input:?}: expected #RGB | #RRGGBB | #RRGGBBAA | rgb(r,g,b) | \
         lowercase name | none"
    ))
}

fn parse_hex(rest: &str, original: &str) -> Result<Color, String> {
    if !rest.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("color: {original:?}: hex must be 0-9 a-f"));
    }
    let rgb = match rest.len() {
        3 => Some(expand3(rest)),
        6 => Some(unhex(&rest[0..6])),
        8 => Some(unhex(&rest[0..6])),
        _ => {
            return Err(format!(
                "color: {original:?}: hex length must be 3, 6, or 8"
            ));
        }
    };
    Ok(Color {
        svg: format!("#{rest}"),
        rgb,
        none: false,
    })
}

fn expand3(s: &str) -> [u8; 3] {
    let mut bytes = [0u8; 3];
    for (i, c) in s.chars().enumerate() {
        let v = c.to_digit(16).expect("ascii_hexdigit") as u8;
        bytes[i] = (v << 4) | v;
    }
    bytes
}

fn unhex(s: &str) -> [u8; 3] {
    let bytes = s.as_bytes();
    [
        from_hex_pair(bytes[0], bytes[1]),
        from_hex_pair(bytes[2], bytes[3]),
        from_hex_pair(bytes[4], bytes[5]),
    ]
}

fn from_hex_pair(hi: u8, lo: u8) -> u8 {
    (hex_nibble(hi) << 4) | hex_nibble(lo)
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => 10 + b - b'a',
        b'A'..=b'F' => 10 + b - b'A',
        _ => 0,
    }
}

fn parse_rgb(rest: &str, original: &str) -> Result<Color, String> {
    let parts: Vec<&str> = rest.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(format!(
            "color: {original:?}: rgb() expects three integers 0..=255"
        ));
    }
    let mut rgb = [0u8; 3];
    for (i, p) in parts.iter().enumerate() {
        rgb[i] = p
            .parse::<u16>()
            .ok()
            .filter(|v| *v <= 255)
            .map(|v| v as u8)
            .ok_or_else(|| {
                format!("color: {original:?}: rgb() component {p:?} is not an integer 0..=255")
            })?;
    }
    Ok(Color {
        svg: format!("rgb({},{},{})", rgb[0], rgb[1], rgb[2]),
        rgb: Some(rgb),
        none: false,
    })
}

fn is_lowercase_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// CSS-style relative luminance (WCAG 2.x) for an sRGB triple. Returns
/// 0.0..=1.0; the contrast ratio is `(L1 + 0.05) / (L2 + 0.05)`.
pub fn relative_luminance(rgb: [u8; 3]) -> f64 {
    fn ch(c: u8) -> f64 {
        let v = f64::from(c) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * ch(rgb[0]) + 0.7152 * ch(rgb[1]) + 0.0722 * ch(rgb[2])
}

/// Contrast ratio between two relative luminances (WCAG 2.x).
pub fn contrast_ratio(la: f64, lb: f64) -> f64 {
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Build the §10 color warning string (or `None`).
///
/// Rules:
/// - `bg=none` — always WARN ("surface-dependent").
/// - Both `fg` and `bg` carry an sRGB triple — contrast below ~3:1
///   WARNs, and inverted polarity (`L_fg > L_bg`) WARNs.
/// - Either color is name-only — skip the contrast/polarity tier.
pub fn warning(fg: &Color, bg: &Color) -> Option<String> {
    if bg.none {
        return Some(
            "colors: --bg none is surface-dependent — decode contrast \
             relies on the physical surface"
                .into(),
        );
    }
    let (Some(fg_rgb), Some(bg_rgb)) = (fg.rgb, bg.rgb) else {
        return None;
    };
    let lf = relative_luminance(fg_rgb);
    let lb = relative_luminance(bg_rgb);
    let cr = contrast_ratio(lf, lb);
    if lf > lb {
        return Some(format!(
            "colors: inverted polarity (fg lighter than bg, contrast {cr:.2}:1) — \
             many scanners expect dark-on-light"
        ));
    }
    if cr < 3.0 {
        return Some(format!(
            "colors: low contrast ({cr:.2}:1, below ~3:1) — decode may fail \
             on consumer scanners"
        ));
    }
    None
}

/// The default foreground (black) — used when the request carries no
/// `--fg`. Pinned so the metadata receipt has a value to echo.
pub fn default_fg() -> Color {
    Color {
        svg: "black".into(),
        rgb: Some([0, 0, 0]),
        none: false,
    }
}

/// The default background (white).
pub fn default_bg() -> Color {
    Color {
        svg: "white".into(),
        rgb: Some([255, 255, 255]),
        none: false,
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_3_6_8() {
        let c = parse("#000", false).unwrap();
        assert_eq!(c.svg, "#000");
        assert_eq!(c.rgb, Some([0, 0, 0]));
        let c = parse("#ffffff", false).unwrap();
        assert_eq!(c.rgb, Some([255, 255, 255]));
        let c = parse("#ff0000aa", false).unwrap();
        assert_eq!(c.rgb, Some([255, 0, 0]));
    }

    #[test]
    fn parses_rgb_function() {
        let c = parse("rgb(255, 128, 0)", false).unwrap();
        assert_eq!(c.svg, "rgb(255,128,0)");
        assert_eq!(c.rgb, Some([255, 128, 0]));
    }

    #[test]
    fn rejects_rgb_overflow() {
        assert!(parse("rgb(256,0,0)", false).is_err());
    }

    #[test]
    fn parses_lowercase_name() {
        let c = parse("orange", false).unwrap();
        assert_eq!(c.svg, "orange");
        assert!(c.rgb.is_none());
    }

    #[test]
    fn none_only_for_bg() {
        assert!(parse("none", false).is_err());
        let c = parse("none", true).unwrap();
        assert!(c.is_none());
    }

    #[test]
    fn contrast_warn_low() {
        let fg = parse("#444", false).unwrap();
        let bg = parse("#333", false).unwrap();
        let w = warning(&fg, &bg).expect("low contrast");
        assert!(w.contains("low contrast") || w.contains("inverted"), "{w}");
    }

    #[test]
    fn inverted_polarity_warns() {
        let fg = parse("#fff", false).unwrap();
        let bg = parse("#000", false).unwrap();
        let w = warning(&fg, &bg).expect("inverted");
        assert!(w.contains("inverted polarity"), "{w}");
    }

    #[test]
    fn good_contrast_no_warn() {
        let fg = parse("#000", false).unwrap();
        let bg = parse("#fff", false).unwrap();
        assert!(warning(&fg, &bg).is_none());
    }

    #[test]
    fn names_skip_contrast_tier() {
        let fg = parse("black", false).unwrap();
        let bg = parse("white", false).unwrap();
        // Name-only — no triples to compute against.
        assert!(warning(&fg, &bg).is_none());
    }

    #[test]
    fn bg_none_warns_surface() {
        let fg = parse("#000", false).unwrap();
        let bg = parse("none", true).unwrap();
        let w = warning(&fg, &bg).expect("surface");
        assert!(w.contains("surface-dependent"));
    }

    #[test]
    fn rejects_unknown_format() {
        for bad in ["", "#xyz", "rgb(1,2)", "rgb(1,2,3,4)", "BLACK", "MyColor"] {
            assert!(parse(bad, false).is_err(), "should reject {bad:?}");
        }
    }
}
