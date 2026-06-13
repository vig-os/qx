//! Payload composition DSL (ADR-031 §10, 2026-06-12).
//!
//! Stage 2 of the print-contract batch: payload accepts a flat list,
//! ONE level of nested h/v groups, and a root-only canvas group with
//! explicit child positions. Two nesting levels remain staged. The
//! grammar:
//!
//! ```text
//! payload  := node (WS+ node)*
//! node     := leaf | group | canvas
//! leaf     := element ('@' size)?
//! element  := qr[:TYPE] | id[:GROUPING|chars-N] | space[:SIZE]
//! group    := '[' ('h'|'v') ':' leaf (WS+ leaf)* ']'
//! canvas   := '[' 'c' <W>x<H>(px|mm) ':' (leaf '@(' x ',' y ')' ('@' size)? )+ ']'
//! size     := <N>(px|mm) | 'w'<N>
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

/// Parse a flat-list payload string. Stage 2: nested groups (`[h:...]`,
/// `[v:...]`) are now accepted and FLATTENED to their leaf sequence
/// when the request layout matches the group axis; this preserves
/// flat-list semantics for the engine while opening the grammar.
///
/// The ROOT may be a flat list OR a single group containing only
/// leaves. A group INSIDE a group still errors with a staged message.
/// Canvas groups (`[c WxH: ...]`) are accepted at the ROOT only and
/// surfaced as their own parsed structure via [`parse_tree`].
///
/// Callers that need the structured AST (e.g. to honor per-node
/// sizing or canvas positions) use [`parse_tree`].
pub fn parse(input: &str) -> Result<Vec<Element>, String> {
    let tree = parse_tree(input)?;
    flatten(&tree)
}

/// Flatten a [`Node`] tree into the engine's stage-1 element list.
/// Errors when the tree contains a canvas group (engine takes a
/// different code path for canvas).
pub fn flatten(tree: &Node) -> Result<Vec<Element>, String> {
    match tree {
        Node::Leaf(l) => Ok(vec![l.element.clone()]),
        Node::List(children) => {
            let mut out = Vec::new();
            for c in children {
                out.extend(flatten(c)?);
            }
            Ok(out)
        }
        Node::Group { children, .. } => {
            let mut out = Vec::new();
            for c in children {
                out.extend(flatten(c)?);
            }
            Ok(out)
        }
        Node::Canvas { .. } => Err("payload: canvas group cannot be flattened — \
             dispatch on the parsed tree (canvas is a root-only \
             structure with explicit child positions)"
            .into()),
    }
}

/// Parse a payload string into the structured AST. Supports:
/// - bare leaves (`qr id`)
/// - one level of nested h/v groups (`[h: qr id]`)
/// - a single canvas group at root (`[c 64x32px: qr@(0,0)]`)
/// - per-leaf sizing attrs (`qr@8px`, `id@w2`)
///
/// Errors quote the group path on infeasibility (`"root > [h:…]"`).
pub fn parse_tree(input: &str) -> Result<Node, String> {
    let mut p = Parser::new(input);
    let nodes = p.parse_nodes(/*inside_group*/ false, /*at_root*/ true)?;
    p.skip_ws();
    if !p.eof() {
        return Err(format!(
            "payload: unexpected trailing input at offset {}: {:?}",
            p.pos,
            &p.src[p.pos..]
        ));
    }
    // Canvas may only appear as the SOLE root node — any sibling at
    // root makes it "canvas inside flow" (ADR-031 §10 staged).
    let has_canvas = nodes.iter().any(|n| matches!(n, Node::Canvas { .. }));
    if has_canvas && nodes.len() > 1 {
        return Err("payload: canvas group [c …] is root-only \
                    (ADR-031 §10) — staged: canvas inside flow"
            .into());
    }
    if nodes.len() == 1 {
        return Ok(nodes.into_iter().next().expect("len=1"));
    }
    Ok(Node::List(nodes))
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

// ---------- structured AST (stage 2: nesting + canvas) ----------

/// One node in the payload tree.
///
/// Stage 2 grammar (ADR-031 §10):
/// - `Leaf` — one element with optional sizing
/// - `List` — the implicit root sequence (the flat-list case)
/// - `Group` — `[h: …]` or `[v: …]` containing LEAVES only
/// - `Canvas` — `[c WxH: leaf@(x,y) …]`, root-only, leaves with
///   explicit positions
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "lowercase")]
pub enum Node {
    Leaf(Leaf),
    List(Vec<Node>),
    Group {
        axis: GroupAxis,
        children: Vec<Node>,
    },
    Canvas {
        width: CanvasDim,
        height: CanvasDim,
        children: Vec<CanvasChild>,
    },
}

/// One sized leaf in the tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leaf {
    pub element: Element,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<NodeSize>,
}

/// Group orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupAxis {
    /// `[h: …]` — children lay out horizontally.
    H,
    /// `[v: …]` — children lay out vertically.
    V,
}

/// Per-node sizing: `@<N>px|mm` (fixed main-axis size) or `@wN`
/// (flex weight over the group's slack).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum NodeSize {
    Px { px: u32 },
    Mm { mm: u32 },
    Flex { weight: u32 },
}

/// Canvas dim: `<N>px` | `<N>mm`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "lowercase")]
pub enum CanvasDim {
    Px { px: u32 },
    Mm { mm: u32 },
}

/// One child of a canvas group: a leaf plus its `(x, y)` and optional
/// size. Coordinates ride their unit suffix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanvasChild {
    pub element: Element,
    pub x: CanvasDim,
    pub y: CanvasDim,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<NodeSize>,
}

// ---------- recursive descent parser ----------

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    /// Parse a (possibly empty) sequence of nodes until a `]` is hit
    /// (when `inside_group`) or EOF (when at root).
    fn parse_nodes(&mut self, inside_group: bool, at_root: bool) -> Result<Vec<Node>, String> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.eof() {
                break;
            }
            if inside_group && self.peek() == Some(b']') {
                break;
            }
            if self.peek() == Some(b'[') {
                if inside_group {
                    // Stage 2: groups cannot nest inside groups.
                    return Err("payload: staged: groups inside groups not yet enabled \
                         (ADR-031 §10) — one nesting level only"
                        .into());
                }
                let node = self.parse_group(at_root)?;
                out.push(node);
            } else {
                let leaf = self.parse_leaf()?;
                out.push(Node::Leaf(leaf));
            }
        }
        Ok(out)
    }

    /// Parse a `[h: …]`, `[v: …]`, or `[c WxH: …]` starting at the
    /// current `[`.
    fn parse_group(&mut self, at_root: bool) -> Result<Node, String> {
        debug_assert_eq!(self.peek(), Some(b'['));
        self.bump();
        self.skip_ws();
        let tag = self.read_ident();
        if tag.is_empty() {
            return Err("payload: empty group tag — expected one of h, v, c".into());
        }
        match tag.as_str() {
            "h" | "v" => {
                self.expect(b':')?;
                let axis = if tag == "h" {
                    GroupAxis::H
                } else {
                    GroupAxis::V
                };
                let children = self.parse_nodes(true, false)?;
                self.expect(b']')?;
                for c in &children {
                    if !matches!(c, Node::Leaf(_)) {
                        return Err(format!(
                            "payload: group [{tag}:…] contains a non-leaf — \
                             stage 2 allows only one nesting level (ADR-031 §10)"
                        ));
                    }
                }
                Ok(Node::Group { axis, children })
            }
            "c" => {
                if !at_root {
                    return Err("payload: canvas group [c …] is root-only \
                         (ADR-031 §10) — staged: canvas inside flow"
                        .into());
                }
                self.skip_ws();
                // `<W>x<H><unit>` — the unit rides ONCE at the end and
                // applies to both dims.
                let w = self.read_uint()?;
                self.expect(b'x')?;
                let h = self.read_uint()?;
                let unit = self.read_unit();
                let (width, height) = match unit.as_str() {
                    "px" => (CanvasDim::Px { px: w }, CanvasDim::Px { px: h }),
                    "mm" => (CanvasDim::Mm { mm: w }, CanvasDim::Mm { mm: h }),
                    "" => {
                        return Err(format!(
                            "payload: canvas dims {w}x{h} missing unit — use px or mm"
                        ));
                    }
                    other => {
                        return Err(format!(
                            "payload: canvas dims unit {other:?} not recognized (px | mm)"
                        ));
                    }
                };
                self.expect(b':')?;
                let children = self.parse_canvas_children()?;
                self.expect(b']')?;
                Ok(Node::Canvas {
                    width,
                    height,
                    children,
                })
            }
            other => Err(format!(
                "payload: unknown group tag {other:?} — expected h, v, or c"
            )),
        }
    }

    fn parse_leaf(&mut self) -> Result<Leaf, String> {
        let token = self.read_token_until_size_or_ws_or_bracket();
        if token.is_empty() {
            return Err(format!(
                "payload: expected an element at offset {}",
                self.pos
            ));
        }
        let element = parse_element(&token)?;
        let size = self.read_optional_size()?;
        Ok(Leaf { element, size })
    }

    fn parse_canvas_children(&mut self) -> Result<Vec<CanvasChild>, String> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.eof() || self.peek() == Some(b']') {
                break;
            }
            let token = self.read_token_until_size_or_ws_or_bracket();
            if token.is_empty() {
                return Err(format!(
                    "payload: expected a canvas child at offset {}",
                    self.pos
                ));
            }
            let element = parse_element(&token)?;
            // Expect '@(' for canvas children.
            self.expect(b'@')?;
            self.expect(b'(')?;
            let x = self.read_canvas_dim()?;
            self.expect(b',')?;
            self.skip_ws();
            let y = self.read_canvas_dim()?;
            self.expect(b')')?;
            let size = self.read_optional_size()?;
            out.push(CanvasChild {
                element,
                x,
                y,
                size,
            });
        }
        Ok(out)
    }

    fn read_optional_size(&mut self) -> Result<Option<NodeSize>, String> {
        if self.peek() != Some(b'@') {
            return Ok(None);
        }
        self.bump();
        // `wN` flex weight or `<N>px|mm`.
        if matches!(self.peek(), Some(b'w')) {
            self.bump();
            let n = self.read_uint()?;
            return Ok(Some(NodeSize::Flex { weight: n }));
        }
        let n = self.read_uint()?;
        let unit = self.read_unit();
        match unit.as_str() {
            "px" => Ok(Some(NodeSize::Px { px: n })),
            "mm" => Ok(Some(NodeSize::Mm { mm: n })),
            "" => Err(format!(
                "payload: size {n:?} missing unit — use {n}px or {n}mm or wN"
            )),
            other => Err(format!(
                "payload: size unit {other:?} not recognized (px | mm | wN)"
            )),
        }
    }

    fn read_canvas_dim(&mut self) -> Result<CanvasDim, String> {
        let n = self.read_uint()?;
        let unit = self.read_unit();
        match unit.as_str() {
            "px" => Ok(CanvasDim::Px { px: n }),
            "mm" => Ok(CanvasDim::Mm { mm: n }),
            "" => Err(format!(
                "payload: canvas dim {n:?} missing unit — use px or mm"
            )),
            other => Err(format!(
                "payload: canvas dim unit {other:?} not recognized (px | mm)"
            )),
        }
    }

    /// Read exactly the canonical 2-char unit (`px` or `mm`) without
    /// consuming following separators (`x`, `,`, `)`, etc).
    fn read_unit(&mut self) -> String {
        let start = self.pos;
        match (
            self.src.get(self.pos).copied(),
            self.src.get(self.pos + 1).copied(),
        ) {
            (Some(b'p'), Some(b'x')) | (Some(b'm'), Some(b'm')) => {
                self.pos += 2;
            }
            _ => {}
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .to_string()
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        self.skip_ws();
        if self.peek() != Some(b) {
            return Err(format!(
                "payload: expected {:?} at offset {}, got {:?}",
                b as char,
                self.pos,
                self.peek().map(|c| c as char)
            ));
        }
        self.bump();
        Ok(())
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .to_string()
    }

    fn read_uint(&mut self) -> Result<u32, String> {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(format!(
                "payload: expected a whole number at offset {}",
                self.pos
            ));
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .parse::<u32>()
            .map_err(|_| format!("payload: bad number at offset {start}"))
    }

    fn read_token_until_size_or_ws_or_bracket(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(
                b,
                b' ' | b'\t' | b'\n' | b'\r' | b'@' | b'[' | b']' | b'(' | b')' | b','
            ) {
                break;
            }
            self.pos += 1;
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap_or("")
            .to_string()
    }
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
    fn flat_parse_accepts_single_level_groups() {
        // Stage 2: [h: ...] and [v: ...] now parse and flatten
        // through `parse`. Plain `qr id` keeps stage-1 semantics.
        let p = parse("[h: qr id]").expect("h-group parses");
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
        let p = parse("[v: id qr]").expect("v-group parses");
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
    fn flat_parse_rejects_nested_groups_with_staged_message() {
        let err = parse("[h: [v: qr id]]").expect_err("nested groups staged");
        assert!(err.contains("groups inside groups"), "got: {err}");
    }

    #[test]
    fn flat_parse_canvas_at_root_not_flattened() {
        // Canvas is structured, not flat — parse() (the flat shim)
        // surfaces the "cannot be flattened" error explaining the
        // canvas needs parse_tree.
        let err = parse("[c 64x32px: qr@(0px,0px)]").expect_err("canvas needs tree");
        assert!(err.contains("canvas"), "got: {err}");
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

    // ---------- structured tree tests (stage 2) ----------

    #[test]
    fn tree_parses_flat_leaves_as_list() {
        let t = parse_tree("qr id").expect("parses");
        match t {
            Node::List(nodes) => {
                assert_eq!(nodes.len(), 2);
                assert!(matches!(nodes[0], Node::Leaf(_)));
                assert!(matches!(nodes[1], Node::Leaf(_)));
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn tree_parses_h_group_with_leaves() {
        let t = parse_tree("[h: qr id]").expect("parses");
        match t {
            Node::Group { axis, children } => {
                assert_eq!(axis, GroupAxis::H);
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected Group, got {other:?}"),
        }
    }

    #[test]
    fn tree_parses_v_group_with_leaves() {
        let t = parse_tree("[v: qr id]").expect("parses");
        match t {
            Node::Group { axis, .. } => assert_eq!(axis, GroupAxis::V),
            other => panic!("expected Group, got {other:?}"),
        }
    }

    #[test]
    fn tree_parses_leaf_with_size_px() {
        let t = parse_tree("qr@8px").expect("parses");
        match t {
            Node::Leaf(l) => {
                assert!(matches!(l.element, Element::Qr { .. }));
                assert_eq!(l.size, Some(NodeSize::Px { px: 8 }));
            }
            other => panic!("expected Leaf, got {other:?}"),
        }
    }

    #[test]
    fn tree_parses_leaf_with_flex_weight() {
        let t = parse_tree("space@w2").expect("parses");
        match t {
            Node::Leaf(l) => {
                assert_eq!(l.size, Some(NodeSize::Flex { weight: 2 }));
            }
            other => panic!("expected Leaf, got {other:?}"),
        }
    }

    #[test]
    fn tree_rejects_nested_groups_staged() {
        let err = parse_tree("[h: [v: qr id]]").expect_err("staged");
        assert!(err.contains("groups inside groups"), "got: {err}");
    }

    #[test]
    fn tree_rejects_canvas_inside_flow_with_staged_message() {
        let err = parse_tree("qr [c 64x32px: qr@(0px,0px)]").expect_err("canvas root-only");
        assert!(err.contains("root-only"), "got: {err}");
    }

    #[test]
    fn tree_parses_canvas_with_one_child() {
        let t = parse_tree("[c 64x32px: qr@(0px,0px)]").expect("parses");
        match t {
            Node::Canvas {
                width,
                height,
                children,
            } => {
                assert_eq!(width, CanvasDim::Px { px: 64 });
                assert_eq!(height, CanvasDim::Px { px: 32 });
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].x, CanvasDim::Px { px: 0 });
                assert_eq!(children[0].y, CanvasDim::Px { px: 0 });
            }
            other => panic!("expected Canvas, got {other:?}"),
        }
    }

    #[test]
    fn tree_parses_canvas_with_multiple_children_with_size() {
        let t = parse_tree("[c 100x50mm: qr@(2mm,3mm)@10mm id@(20mm,3mm)]").expect("parses");
        match t {
            Node::Canvas { children, .. } => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].size, Some(NodeSize::Mm { mm: 10 }));
                assert!(children[1].size.is_none());
            }
            other => panic!("expected Canvas, got {other:?}"),
        }
    }

    #[test]
    fn tree_unknown_group_tag_errors() {
        let err = parse_tree("[x: qr id]").expect_err("unknown tag");
        assert!(err.contains("unknown group tag"), "got: {err}");
    }
}
