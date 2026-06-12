//! Regression test: regenerate every `examples/` SVG from fixed IDs and
//! assert byte-for-byte equality with the committed files.
//!
//! This catches silent rendering drift when codec internals change.
//! Run `REGEN_EXAMPLES=1 cargo test -p part-registry-codec --test examples_regression`
//! to overwrite the committed files after an intentional change.

use std::fs;
use std::path::PathBuf;

use part_registry_codec::{recommend_format, render_label, Layout};

/// One row in the gallery table (mirrors `examples/README.md`).
struct ExampleSpec {
    /// Subdirectory under `examples/`, e.g. `"horz-s11"`.
    dir: &'static str,
    /// Fixed canonical ID baked into the committed SVG.
    id: &'static str,
    /// Label layout.
    layout: Layout,
    /// Short-side size in mm.
    size_mm: f64,
}

/// The full gallery. IDs match the committed examples. Format is
/// auto-selected via `recommend_format` (same logic the CLI uses).
fn specs() -> Vec<ExampleSpec> {
    vec![
        ExampleSpec {
            dir: "horz-s11",
            id: "8Z6GCDGBH5KT",
            layout: Layout::Horz,
            size_mm: 11.0,
        },
        ExampleSpec {
            dir: "horz-pt12",
            id: "B7GA48WNWSRW",
            layout: Layout::Horz,
            size_mm: 9.0,
        },
        ExampleSpec {
            dir: "horz-pt24",
            id: "MZZYATA434YX",
            layout: Layout::Horz,
            size_mm: 18.0,
        },
        ExampleSpec {
            dir: "vert-s6",
            id: "9ZDA2QH2TTX3",
            layout: Layout::Vert,
            size_mm: 6.0,
        },
        ExampleSpec {
            dir: "vert-s8",
            id: "E8UX8V5YFW5K",
            layout: Layout::Vert,
            size_mm: 8.0,
        },
        ExampleSpec {
            dir: "vert-pt12",
            id: "Q2EFNXXWGHTV",
            layout: Layout::Vert,
            size_mm: 9.0,
        },
        ExampleSpec {
            dir: "vert-pt24",
            id: "67RNWVJX9DCW",
            layout: Layout::Vert,
            size_mm: 18.0,
        },
        ExampleSpec {
            dir: "flag-d4",
            id: "PFBT73FXAVF2",
            layout: Layout::Flag {
                cable_od_mm: 4.0,
                no_markers: false,
                alignment_line: false,
            },
            size_mm: 11.0,
        },
        ExampleSpec {
            dir: "flag-d8",
            id: "MEQSUTE87XZW",
            layout: Layout::Flag {
                cable_od_mm: 8.0,
                no_markers: false,
                alignment_line: false,
            },
            size_mm: 11.0,
        },
        ExampleSpec {
            dir: "flag-d12",
            id: "29MVECM74RQK",
            layout: Layout::Flag {
                cable_od_mm: 12.0,
                no_markers: false,
                alignment_line: false,
            },
            size_mm: 11.0,
        },
    ]
}

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR`
/// until we find `examples/`.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/codec -> repo root (two levels up)
    let root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("cannot find workspace root from CARGO_MANIFEST_DIR");
    assert!(
        root.join("examples").is_dir(),
        "expected examples/ at workspace root {}",
        root.display()
    );
    root.to_path_buf()
}

#[test]
fn examples_match_committed_svgs() {
    let root = workspace_root();
    let regen = std::env::var("REGEN_EXAMPLES")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    let mut mismatches: Vec<String> = Vec::new();

    for spec in specs() {
        let (format, _warning) = recommend_format(spec.size_mm);
        let svg = render_label(spec.id, spec.layout, spec.size_mm, format, false)
            .unwrap_or_else(|e| panic!("render_label failed for {}: {e}", spec.dir));

        let dir = root.join("examples").join(spec.dir);
        let path = dir.join(format!("{}.svg", spec.id));

        if regen {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&path, &svg).unwrap();
            eprintln!("  regenerated {}", path.display());
            continue;
        }

        let committed = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                mismatches.push(format!("{}: file missing or unreadable: {e}", spec.dir));
                continue;
            }
        };
        if committed != svg {
            mismatches.push(format!(
                "{}: SVG differs (committed {} bytes, generated {} bytes)",
                spec.dir,
                committed.len(),
                svg.len()
            ));
        }
    }

    if !mismatches.is_empty() {
        panic!(
            "Example SVGs out of date — run \
             `REGEN_EXAMPLES=1 cargo test -p part-registry-codec --test examples_regression` \
             to update.\n\nMismatches:\n  {}",
            mismatches.join("\n  ")
        );
    }
}
