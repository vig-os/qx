//! The serde command protocol per ADR-030 §1 + ADR-035 §0.
//!
//! `Request` is one parameterized op family (`Create / Resolve / List /
//! Count / Describe / Edit / Transition / Print / Export /
//! PollProposal / Whoami { collection, … }`), not one variant per
//! bespoke operation. Every shell — CLI, TUI, serve, MCP, web (WASM),
//! Tauri — speaks exactly this shape; JSON form is internally tagged
//! with `"op"`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

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
    #[serde(default)]
    pub micro: bool,
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
    /// the actual uniform white absorbs the remainder so the canvas
    /// stays exactly `size_px`. Default 0 (max module size at the
    /// requested canvas; the quiet zone still guarantees white under
    /// "overlap").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_px: Option<u32>,
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
            cable_od_mm: None,
            copies: default_copies(),
            log: default_true(),
            unit: default_unit(),
            size_px: None,
            padding_px: None,
            padding_mode: None,
            dpi: None,
        }
    }
}

/// The command protocol. JSON is internally tagged with `"op"`.
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
