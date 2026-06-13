//! The serde command protocol per ADR-030 §1 + ADR-035 §0.
//!
//! `Request` is one parameterized op family (`Create / Resolve / List /
//! Count / Describe / Edit / Transition / Print / Export /
//! PollProposal / Whoami { collection, … }`), not one variant per
//! bespoke operation. Every shell — CLI, TUI, serve, MCP, web (WASM),
//! Tauri — speaks exactly this shape; JSON form is internally tagged
//! with `"op"`.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use part_registry_codec::Padding;
use part_registry_domain::ProposalRef;

// -------------------------------------------------------------------
// Request
// -------------------------------------------------------------------

/// Page window for `List`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    50
}

impl Default for Page {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: default_limit(),
        }
    }
}

/// Sort key + direction for `List`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sort {
    pub field: String,
    #[serde(default)]
    pub dir: SortDir,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

/// The one filter grammar (ADR-035 §0: shared by `List`, `Count`,
/// `Print` selection, and stream reads — no parallel filter dialects).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    /// Lifecycle status (descriptor-declared value, e.g. "bound").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Kind (descriptor-declared; parts only carry this post-ADR-035).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Free-text match over id + declared fields (case-insensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Exact-ish per-field matches (substring, case-insensitive).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
}

/// Selection for ops that act on a set of entities (`Print`, future
/// `Export` subsets): explicit ids or the shared [`Filter`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    Ids(Vec<String>),
    Filter(Filter),
}

/// Per-side padding floor on the wire — the CSS-shorthand expansion
/// rule shared by CLI and wire (ADR-031 §8, 2026-06-11): `2` (all
/// four sides) | `[2,6]` (vertical, horizontal) | `[2,6,4,6]` (top,
/// right, bottom, left — CSS clockwise). The untagged integer form
/// keeps pre-shorthand payloads (`"padding_px": 2`) deserializing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PaddingSpec {
    /// `2` — the same floor on all four sides.
    Uniform(u32),
    /// `[2,6]` — vertical (top/bottom), horizontal (right/left).
    VertHorz([u32; 2]),
    /// `[2,6,4,6]` — top, right, bottom, left (CSS clockwise).
    Sides([u32; 4]),
}

impl PaddingSpec {
    /// The one expansion rule into the codec's per-side floors.
    pub fn expand(self) -> Padding {
        match self {
            PaddingSpec::Uniform(all) => Padding::uniform(all),
            PaddingSpec::VertHorz([v, h]) => Padding::axes(v, h),
            PaddingSpec::Sides([t, r, b, l]) => Padding::sides(t, r, b, l),
        }
    }
}

impl FromStr for PaddingSpec {
    type Err = String;

    /// CSS-shorthand text form: `2` | `2,6` | `2,6,4,6` — the same
    /// expansion the wire arrays use.
    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<u32> = s
            .split(',')
            .map(|p| {
                p.trim()
                    .parse::<u32>()
                    .map_err(|_| format!("padding {s:?}: {p:?} is not a whole number of px"))
            })
            .collect::<Result<_, _>>()?;
        match parts.as_slice() {
            [all] => Ok(PaddingSpec::Uniform(*all)),
            [v, h] => Ok(PaddingSpec::VertHorz([*v, *h])),
            [t, r, b, l] => Ok(PaddingSpec::Sides([*t, *r, *b, *l])),
            _ => Err(format!(
                "padding {s:?}: expected 1, 2, or 4 comma-separated values \
                 (all | vertical,horizontal | top,right,bottom,left)"
            )),
        }
    }
}

/// Print options (ADR-031). `unit` selects the renderer: "mm" (the
/// default — the original mm-native renderer, behavior unchanged) or
/// "px" (the ADR-031 §2 px-true device-pixel renderer; obligation
/// `px-true-qr-render`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintOptions {
    /// Geometry: "vert" | "horz" | "flag".
    #[serde(default = "default_layout")]
    pub layout: String,
    #[serde(default = "default_size_mm")]
    pub size_mm: f64,
    /// Human-ID grouping: "44" | "444" | "554" | "auto".
    #[serde(default = "default_chars")]
    pub chars: String,
    /// Deprecated input (ADR-031 §8): `micro: true` means symbology
    /// "micro" — consulted only when `symbology` is absent. Response
    /// labels always carry the resolved `symbology` string.
    #[serde(default)]
    pub micro: bool,
    /// Symbology in the canonical compact form the CLI speaks
    /// (`<family>[-<version>][-<ec>]`: "micro", "micro-m3-l",
    /// "qr-v1-m", …). Version/EC auto-fit against the payload when
    /// unpinned; response labels carry the RESOLVED string
    /// (e.g. "micro-m4-m"). Wins over the deprecated `micro` flag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbology: Option<String>,
    /// Required when layout == "flag".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cable_od_mm: Option<f64>,
    #[serde(default = "default_copies")]
    pub copies: u32,
    /// Append print events to the audit surface (default true).
    #[serde(default = "default_true")]
    pub log: bool,
    /// Sizing unit (ADR-031 §3): "mm" (default) or "px".
    #[serde(default = "default_unit")]
    pub unit: String,
    /// The EXACT output canvas in device px along the label's
    /// controlling dimension (unit = "px"; ADR-031 §2/§8): the module
    /// size is deduced per `padding_mode` (overlap: max `m` with
    /// `data·m + 2·max(padding_px, quiet·m) ≤ size_px`) and the render
    /// errors if the symbol cannot fit. When absent, `size_mm`
    /// converts at `dpi` into this canvas size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_px: Option<u32>,
    /// Minimum padding in device px, measured canvas edge → module
    /// part — the ADR-031 §4 floor consumed by the module deduction;
    /// the controlling axis absorbs the remainder on top of its floors
    /// so the canvas stays exactly `size_px`. Per-side CSS shorthand
    /// (§8): `2` | `[2,6]` | `[2,6,4,6]` — the plain-integer form is
    /// the pre-shorthand wire shape and still deserializes. Default 0
    /// (max module size at the requested canvas; the quiet zone still
    /// guarantees white under "overlap").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_px: Option<PaddingSpec>,
    /// How the quiet zone counts toward the `padding_px` floor
    /// (ADR-031 §8): "overlap" (default — the quiet zone satisfies
    /// outside padding; printers donate intrinsic margins) or
    /// "additive" (quiet zone excluded; full-bleed/die-cut).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_mode: Option<String>,
    /// Dots per inch for the mm → px conversion. Default 300.0 ≈
    /// Brother QL class heads (ADR-031 §3; the per-printer profile
    /// default is an ADR-031 open question — until it lands, 300 dpi
    /// is the documented fallback).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<f64>,
    /// ADR-031 §10 — flat-list payload DSL (stage 1). Whitespace-
    /// separated leaves in axis order: `qr[:TYPE] | id[:GROUPING|chars-N]
    /// | space[:SIZE]`. Element params win over the global flags
    /// (`chars`, `symbology`); the global flags win over contract
    /// defaults. Absent ⇒ the resolved shape comes from the legacy
    /// flags alone (effective `"qr id"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>,
    /// `--fg` color (ADR-031 §10). Accepted: `#RGB`/`#RRGGBB`/
    /// `#RRGGBBAA`, `rgb(r,g,b)`, lowercase ascii names. Default
    /// black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    /// `--bg` color (ADR-031 §10). Accepts the same forms as `fg`,
    /// plus `"none"` (omits the background rect). Default white.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    /// `--size-mode exact|snap` (ADR-031 §8 2026-06-11). `exact`
    /// (default) holds the canvas at the requested size; `snap`
    /// treats `size_px` as an UPPER BOUND and the canvas snaps DOWN
    /// to the content lattice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_mode: Option<String>,
    /// `--id-chars` — id-text solver input (ADR-031 §10): how many
    /// id characters render.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_chars: Option<u32>,
    /// `--rows` — id-text solver input: how many rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u32>,
    /// `--id-size <N>[px|mm]` — id-text solver input: glyph height
    /// in device px (mm rides the value like `--size`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_size_px: Option<u32>,
    /// `--repeat <n|fill>` (ADR-031 §10): compose the rendered label
    /// into N copies along the canvas axis. `"fill"` fits as many as
    /// `--length` allows at the gap floor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<String>,
    /// `--repeat-axis along|across` (default along = canvas flow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_axis: Option<String>,
    /// `--repeat-gap <N>[px|mm]` — explicit inter-copy gap in device px.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_gap_px: Option<u32>,
    /// `--repeat-orient same|alternate` (alternate rotates every
    /// second copy 180°). Default `same`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_orient: Option<String>,
    /// `--length <N>[px|mm]` — required for `fill` and for derived
    /// gaps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_px: Option<u32>,
    /// `--spacing linear|cyclic` — linear has n-1 gaps; cyclic has
    /// n gaps (closed loops). Default linear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<String>,
    /// `--rotate 0|90|180|270` — whole-label rotation applied BEFORE
    /// repeating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<u32>,
    /// `--length-excess <N>[px|mm]` — BLANK leader/tail in device px.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_excess_px: Option<u32>,
    /// `--excess-at start|end` — which end carries the excess zone.
    /// Default `end`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excess_at: Option<String>,
}

fn default_layout() -> String {
    "horz".into()
}
fn default_size_mm() -> f64 {
    8.0
}
fn default_chars() -> String {
    "auto".into()
}
fn default_copies() -> u32 {
    1
}
fn default_true() -> bool {
    true
}
fn default_unit() -> String {
    "mm".into()
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            layout: default_layout(),
            size_mm: default_size_mm(),
            chars: default_chars(),
            micro: false,
            symbology: None,
            cable_od_mm: None,
            copies: default_copies(),
            log: default_true(),
            unit: default_unit(),
            size_px: None,
            padding_px: None,
            padding_mode: None,
            dpi: None,
            payload: None,
            fg: None,
            bg: None,
            size_mode: None,
            id_chars: None,
            rows: None,
            id_size_px: None,
            repeat: None,
            repeat_axis: None,
            repeat_gap_px: None,
            repeat_orient: None,
            length_px: None,
            spacing: None,
            rotate: None,
            length_excess_px: None,
            excess_at: None,
        }
    }
}

/// The command protocol. JSON is internally tagged with `"op"`.
///
/// The size difference between variants is acceptable here: the
/// protocol enum is dispatched once per request — it lives on the
/// stack briefly, never in hot Vec's or maps. Boxing `Print` would
/// trade clarity for negligible savings.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum Request {
    /// Universal resolve over the global id space (bare value =
    /// default scheme; 8-char human prefix accepted, ambiguity is an
    /// error). ADR-035 §0.
    Resolve { id: String },
    /// Generic query over one collection.
    List {
        collection: String,
        #[serde(default)]
        filter: Filter,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sort: Vec<Sort>,
        #[serde(default)]
        page: Page,
    },
    /// Single-field group-by count (the only aggregation — never a join).
    Count {
        collection: String,
        #[serde(default)]
        filter: Filter,
        by: String,
    },
    /// Render the registry's descriptors ("what exists + how it's
    /// minted" is introspectable data).
    Describe {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        collection: Option<String>,
    },
    /// Create entities. For `parts` this is mint: `n` fresh ids.
    Create {
        collection: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        n: Option<u32>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        fields: BTreeMap<String, String>,
    },
    /// Status-preserving field edit (status-changing ⇒ `Transition`).
    Edit {
        collection: String,
        id: String,
        fields: BTreeMap<String, String>,
    },
    /// Lifecycle transition, with optional fields payload whose
    /// `meaningful_from` is satisfied by the target status
    /// (`bind` = Transition{parts, →bound, fields}).
    Transition {
        collection: String,
        id: String,
        to: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        fields: BTreeMap<String, String>,
    },
    /// Render labels for a selection (delivery is a shell capability —
    /// the response carries the rendered SVGs).
    Print {
        collection: String,
        selection: Selection,
        #[serde(default)]
        options: PrintOptions,
    },
    /// Flat export of a collection (generated artifact — never
    /// committed beside the source of truth).
    Export { collection: String, format: String },
    /// Poll a submitted proposal's status.
    PollProposal { proposal: ProposalRef },
    /// Current operator identity.
    Whoami,
}

// -------------------------------------------------------------------
// Response
// -------------------------------------------------------------------

/// Error taxonomy mirrored by every shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    NotFound,
    Ambiguous,
    Validation,
    Unsupported,
    Auth,
    Backend,
    BadRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub kind: ErrorKind,
    pub message: String,
}

/// Wire response: `{"ok":true,"data":…}` or `{"ok":false,"error":…}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok { ok: bool, data: Json },
    Err { ok: bool, error: ErrorBody },
}

impl Response {
    pub fn ok(data: impl Serialize) -> Self {
        match serde_json::to_value(data) {
            Ok(v) => Response::Ok { ok: true, data: v },
            Err(e) => Response::error(ErrorKind::Backend, format!("encode response: {e}")),
        }
    }

    pub fn error(kind: ErrorKind, message: impl Into<String>) -> Self {
        Response::Err {
            ok: false,
            error: ErrorBody {
                kind,
                message: message.into(),
            },
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, Response::Ok { .. })
    }

    /// Test/shell convenience: the `data` payload of an Ok response.
    pub fn data(&self) -> Option<&Json> {
        match self {
            Response::Ok { data, .. } => Some(data),
            Response::Err { .. } => None,
        }
    }

    /// Test/shell convenience: the error body of an Err response.
    pub fn err(&self) -> Option<&ErrorBody> {
        match self {
            Response::Ok { .. } => None,
            Response::Err { error, .. } => Some(error),
        }
    }
}
