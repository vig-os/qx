//! End-to-end test for the `label` CLI per foundation issue #32.
//!
//! Wires a tempdir-backed `CsvGitRepository` seeded with two parts;
//! invokes `run_label`; asserts on:
//!
//! - SVGs written to `out_dir`
//! - `print_log.csv` rows appended via `Repository::append_print_event`
//! - format auto-selection by size
//! - per-layout dimension parity with `label.py`

mod common;

use qx_cli::{run_label, FormatArg, LabelArgs, LayoutArg, StatusArg};
use qx_codec::TextFormat;
use qx_storage::PrintEventFilter;

const FIXED_ID_A: &str = "K7M3PQ9RT5VAXY";
const FIXED_ID_B: &str = "ABCDEFGHJKMNPQ";

#[test]
fn label_renders_svg_per_id_and_logs_print_event() {
    let rows = vec![
        (FIXED_ID_A, "unbound", "B-test"),
        (FIXED_ID_B, "unbound", "B-test"),
    ];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![FIXED_ID_A.into(), FIXED_ID_B.into()],
        batch: None,
        status: None,
        layout: LayoutArg::Horz,
        size: Some(11.0),
        tape: None,
        format: FormatArg::Auto,
        cable_od: None,
        out_dir: None,
        copies: 1,
        no_log: false,
        operator: Some("gerchowl".into()),
        output_mode: "dk-continuous-auto-cut".into(),
        micro: false,
    };

    let out = run_label(&args, &wiring).expect("label succeeds");
    assert_eq!(out.rendered.len(), 2);
    assert!(out.logged);
    assert_eq!(out.size_mm, 11.0);
    assert_eq!(out.format, TextFormat::FourFourFour);

    for r in &out.rendered {
        assert!(r.path.exists(), "svg should exist at {:?}", r.path);
        assert!(r.svg.starts_with("<svg "));
        assert!(r.svg.contains("width=\"22.000mm\""));
        assert!(r.svg.contains("height=\"11.000mm\""));
        // The 4/4/4 text rows must appear in the SVG.
        let id = r.id.as_str();
        assert!(r.svg.contains(&id[0..4]));
        assert!(r.svg.contains(&id[4..8]));
        assert!(r.svg.contains(&id[8..12]));
    }

    // print_log.csv has two rows.
    let events = wiring
        .repo
        .list_print_events(&PrintEventFilter::default())
        .unwrap();
    assert_eq!(events.len(), 2);
    for e in &events {
        assert_eq!(e.layout, "horz");
        assert_eq!(e.size_mm, 11.0);
        assert_eq!(e.printed_by.0 .0, "gerchowl");
        assert_eq!(e.output_mode, "dk-continuous-auto-cut");
        assert_eq!(e.copies, 1);
    }
}

#[test]
fn label_no_log_skips_print_event_append() {
    let rows = vec![(FIXED_ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![FIXED_ID_A.into()],
        batch: None,
        status: None,
        layout: LayoutArg::Horz,
        size: Some(11.0),
        tape: None,
        format: FormatArg::Auto,
        cable_od: None,
        out_dir: None,
        copies: 1,
        no_log: true,
        operator: Some("op".into()),
        output_mode: "test".into(),
        micro: false,
    };

    let out = run_label(&args, &wiring).unwrap();
    assert!(!out.logged);
    let events = wiring
        .repo
        .list_print_events(&PrintEventFilter::default())
        .unwrap();
    assert!(events.is_empty());
}

#[test]
fn label_flag_layout_requires_cable_od() {
    let rows = vec![(FIXED_ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![FIXED_ID_A.into()],
        batch: None,
        status: None,
        layout: LayoutArg::Flag,
        size: Some(11.0),
        tape: None,
        format: FormatArg::Auto,
        cable_od: None,
        out_dir: None,
        copies: 1,
        no_log: true,
        operator: Some("op".into()),
        output_mode: "test".into(),
        micro: false,
    };
    let err = run_label(&args, &wiring).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("flag"));
    assert!(msg.contains("cable-od"));
}

#[test]
fn label_status_filter_selects_matching_rows() {
    let rows = vec![
        (FIXED_ID_A, "unbound", "B-test"),
        (FIXED_ID_B, "bound", "B-test"),
    ];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![],
        batch: None,
        status: Some(StatusArg::Unbound),
        layout: LayoutArg::Horz,
        size: Some(11.0),
        tape: None,
        format: FormatArg::Auto,
        cable_od: None,
        out_dir: None,
        copies: 1,
        no_log: true,
        operator: Some("op".into()),
        output_mode: "test".into(),
        micro: false,
    };
    let out = run_label(&args, &wiring).unwrap();
    assert_eq!(out.rendered.len(), 1);
    assert_eq!(out.rendered[0].id.as_str(), FIXED_ID_A);
}

#[test]
fn label_tape_preset_resolves_size() {
    let rows = vec![(FIXED_ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![FIXED_ID_A.into()],
        batch: None,
        status: None,
        layout: LayoutArg::Horz,
        size: None,
        tape: Some("pt-12".into()),
        format: FormatArg::Auto,
        cable_od: None,
        out_dir: None,
        copies: 1,
        no_log: true,
        operator: Some("op".into()),
        output_mode: "test".into(),
        micro: false,
    };
    let out = run_label(&args, &wiring).unwrap();
    assert_eq!(out.size_mm, 9.0);
}

#[test]
fn label_vertical_layout_produces_1to2_aspect() {
    let rows = vec![(FIXED_ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![FIXED_ID_A.into()],
        batch: None,
        status: None,
        layout: LayoutArg::Vert,
        size: Some(8.0),
        tape: None,
        format: FormatArg::Auto,
        cable_od: None,
        out_dir: None,
        copies: 1,
        no_log: true,
        operator: Some("op".into()),
        output_mode: "test".into(),
        micro: false,
    };
    let out = run_label(&args, &wiring).unwrap();
    let svg = &out.rendered[0].svg;
    assert!(svg.contains("width=\"8.000mm\""));
    assert!(svg.contains("height=\"16.000mm\""));
}

#[test]
fn label_flag_layout_renders_with_wrap_zone() {
    let rows = vec![(FIXED_ID_A, "unbound", "B-test")];
    let (_tmp, wiring, _store) = common::seeded_wiring(&rows);
    let args = LabelArgs {
        ids: vec![FIXED_ID_A.into()],
        batch: None,
        status: None,
        layout: LayoutArg::Flag,
        size: Some(11.0),
        tape: None,
        format: FormatArg::Auto,
        cable_od: Some(6.0),
        out_dir: None,
        copies: 1,
        no_log: true,
        operator: Some("op".into()),
        output_mode: "test".into(),
        micro: false,
    };
    let out = run_label(&args, &wiring).unwrap();
    let svg = &out.rendered[0].svg;
    assert!(svg.contains("stroke-dasharray"));
    assert!(svg.contains(">wrap d6"));
}
