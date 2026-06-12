//! Bench the ADR obligations shortlist so nothing falls out of the ADRs silently.
//!
//! Rust port of the retired `tools/obligations_check.py` (ADR-017 step 9
//! strangler-fig deletion). Reads `decisions/obligations.toml` (the structured
//! "what falls out of the ADRs" feeder, per ADR-030 §8 / ADR-029 dimension 4)
//! and checks reality against it:
//!
//! - schema: every row has the fields its `status` requires
//! - satisfied rows: `satisfied_by` path(s)/glob(s) actually resolve
//! - pending rows: carry a `tracking` pointer (work isn't lost, just open)
//! - exempt rows: carry `exempt_until` + `exempt_reason`, and the date hasn't passed
//! - coverage: every in-force ADR (decisions/ADR-NNN-*.md, minus [meta].excluded)
//!   has >=1 row — so a new ADR can't land without declaring its obligations
//! - orphans: no row points at an ADR (or excluded entry) that doesn't exist
//!
//! Exit codes (mirror ADR-029): 0 ok · 1 missing/unsatisfied · 2 orphan · 3
//! expired exemption. Precedence when several apply: 3 > 2 > 1. `pending` rows
//! are reported but never fail (foundation work in flight is legitimate).
//!
//! Usage:
//!   obligations-check [--json PATH]   # also write feeder-JSON (PATH or - for stdout)
//!
//! Output contract: diagnostics go to stderr, stdout is reserved for `--json -`,
//! and every line byte-matches the retired Python gate (downstream tooling and
//! the prek hook treat the summary line + exit codes as the interface).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VALID_KIND: [&str; 7] = ["artifact", "ci", "crate", "doc", "gate", "issue", "test"];
const VALID_STATUS: [&str; 3] = ["exempt", "pending", "satisfied"];

/// Feeder-JSON citation: `satisfied_by` may be a string or a list, `tracking`
/// a string, exemptions a synthesized string — or nothing at all.
#[derive(Clone, Debug, PartialEq)]
enum Citation {
    None,
    Str(String),
    List(Vec<String>),
}

/// One feeder row for the future ADR-029 coverage joiner.
#[derive(Clone, Debug)]
struct FeederRow {
    obligation: String,
    satisfied: bool,
    citation: Citation,
    exempt_until: Option<String>,
}

/// Everything `evaluate` finds; printing and exit-code derivation are separate
/// so tests can assert on the classification directly.
#[derive(Debug, Default)]
struct Outcome {
    missing: Vec<String>,
    orphan: Vec<String>,
    expired: Vec<String>,
    pending: Vec<String>,
    feeder: Vec<FeederRow>,
    total: usize,
    in_force: usize,
}

impl Outcome {
    fn satisfied_count(&self) -> usize {
        self.feeder.iter().filter(|f| f.satisfied).count()
    }

    /// Exit-code precedence per ADR-029: 3 > 2 > 1 > 0.
    fn exit_code(&self) -> u8 {
        if !self.expired.is_empty() {
            return 3;
        }
        if !self.orphan.is_empty() {
            return 2;
        }
        if !self.missing.is_empty() {
            return 1;
        }
        0
    }
}

/// Collect ADR ids from `decisions/ADR-NNN-*.md` filenames. Mirrors the
/// Python gate's `^(ADR-\d+)-.*\.md$` filename regex.
fn adr_ids_on_disk(decisions: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(decisions) else {
        return ids;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if let Some(id) = adr_id_from_filename(name) {
            ids.insert(id);
        }
    }
    ids
}

/// `ADR-template.md` has no digit run, so it is not an ADR id.
fn adr_id_from_filename(name: &str) -> Option<String> {
    let rest = name.strip_prefix("ADR-")?;
    if !name.ends_with(".md") {
        return None;
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let after = &rest[digits.len()..];
    // The regex demands a literal dash after the digits, then `.*\.md`
    if !after.starts_with('-') || after.len() < "-.md".len() {
        return None;
    }
    Some(format!("ADR-{digits}"))
}

/// Treat as a path first (covers literal paths with no glob chars), then glob.
fn resolves(repo: &Path, pattern: &str) -> bool {
    if repo.join(pattern).exists() {
        return true;
    }
    let abs = repo.join(pattern);
    let Some(abs) = abs.to_str() else {
        return false;
    };
    match glob::glob(abs) {
        Ok(mut paths) => paths.any(|p| p.is_ok()),
        Err(_) => false,
    }
}

/// Python repr of a list of strings — `['a', 'b']` — used verbatim in the
/// unresolved-paths diagnostic so the message byte-matches the retired gate.
fn py_list_repr(items: &[String]) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push('\'');
        out.push_str(&item.replace('\\', "\\\\").replace('\'', "\\'"));
        out.push('\'');
    }
    out.push(']');
    out
}

/// `row.get(key)` as the Python gate saw it: a string stays a string, an
/// absent key reads "None", anything else falls back to its TOML rendering.
fn value_display(v: Option<&toml::Value>) -> String {
    match v {
        None => "None".into(),
        Some(toml::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn str_field(row: &toml::Table, key: &str) -> Option<String> {
    row.get(key).and_then(|v| v.as_str()).map(str::to_owned)
}

fn evaluate(repo: &Path, today: &str) -> Result<Outcome, String> {
    let obligations_path = repo.join("decisions").join("obligations.toml");
    let raw = std::fs::read_to_string(&obligations_path)
        .map_err(|e| format!("cannot read decisions/obligations.toml: {e}"))?;
    let data: toml::Table = raw
        .parse()
        .map_err(|e| format!("cannot read decisions/obligations.toml: {e}"))?;

    let on_disk = adr_ids_on_disk(&repo.join("decisions"));

    let mut excluded: BTreeMap<String, String> = BTreeMap::new();
    if let Some(entries) = data
        .get("meta")
        .and_then(|m| m.get("excluded"))
        .and_then(|e| e.as_array())
    {
        for entry in entries {
            let Some(adr) = entry.get("adr").and_then(|v| v.as_str()) else {
                continue;
            };
            let reason = entry
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            excluded.insert(adr.to_owned(), reason.to_owned());
        }
    }

    let mut out = Outcome {
        in_force: on_disk.len().saturating_sub(excluded.len()),
        ..Outcome::default()
    };

    let empty = Vec::new();
    let rows = data
        .get("obligation")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    out.total = rows.len();

    let mut seen_ids: BTreeSet<String> = BTreeSet::new();
    let mut covered_adrs: BTreeSet<String> = BTreeSet::new();

    for row_value in rows {
        let row = match row_value.as_table() {
            Some(t) => t.clone(),
            None => toml::Table::new(),
        };
        let rid = str_field(&row, "id").unwrap_or_else(|| "<no-id>".into());
        if seen_ids.contains(&rid) {
            out.missing.push(format!("{rid}: duplicate id"));
        }
        seen_ids.insert(rid.clone());

        match str_field(&row, "adr") {
            None => out.missing.push(format!("{rid}: missing 'adr'")),
            Some(adr) if !on_disk.contains(&adr) => out.orphan.push(format!(
                "{rid}: references {adr} but no decisions/{adr}-*.md exists"
            )),
            Some(adr) => {
                covered_adrs.insert(adr);
            }
        }

        let kind = str_field(&row, "kind");
        if !kind.as_deref().is_some_and(|k| VALID_KIND.contains(&k)) {
            out.missing.push(format!(
                "{rid}: kind '{}' not in {}",
                value_display(row.get("kind")),
                py_list_repr(&VALID_KIND.map(String::from)),
            ));
        }
        if str_field(&row, "statement").is_none_or(|s| s.is_empty()) {
            out.missing.push(format!("{rid}: missing 'statement'"));
        }

        let status = str_field(&row, "status");
        let mut satisfied = false;
        let mut citation = Citation::None;
        match status.as_deref() {
            Some("satisfied") => {
                let sb = row.get("satisfied_by");
                let paths: Vec<String> = match sb {
                    Some(toml::Value::String(s)) => vec![s.clone()],
                    Some(toml::Value::Array(a)) => a
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                if paths.is_empty() {
                    out.missing
                        .push(format!("{rid}: status=satisfied requires 'satisfied_by'"));
                } else {
                    let unresolved: Vec<String> = paths
                        .iter()
                        .filter(|p| !resolves(repo, p))
                        .cloned()
                        .collect();
                    if unresolved.is_empty() {
                        satisfied = true;
                    } else {
                        out.missing.push(format!(
                            "{rid}: satisfied_by does not resolve: {}",
                            py_list_repr(&unresolved),
                        ));
                    }
                }
                citation = match sb {
                    Some(toml::Value::String(s)) => Citation::Str(s.clone()),
                    Some(toml::Value::Array(_)) => Citation::List(paths),
                    _ => Citation::None,
                };
            }
            Some("pending") => match str_field(&row, "tracking") {
                Some(tracking) if !tracking.is_empty() => {
                    out.pending.push(format!("{rid}: {tracking}"));
                    citation = Citation::Str(tracking);
                }
                _ => out
                    .missing
                    .push(format!("{rid}: status=pending requires 'tracking'")),
            },
            Some("exempt") => {
                let until = str_field(&row, "exempt_until");
                let reason = str_field(&row, "exempt_reason");
                match (&until, &reason) {
                    (Some(until), Some(reason)) if !until.is_empty() && !reason.is_empty() => {
                        // ISO date strings sort lexicographically, so string
                        // comparison is date comparison — same trick as Python
                        if today > until.as_str() {
                            out.expired
                                .push(format!("{rid}: exemption expired {until} ({reason})"));
                        }
                    }
                    _ => out.missing.push(format!(
                        "{rid}: status=exempt requires 'exempt_until' + 'exempt_reason'"
                    )),
                }
                // Python set this citation unconditionally in the exempt
                // branch, rendering a missing date as the literal "None"
                citation = Citation::Str(format!(
                    "exempt until {}",
                    until.as_deref().unwrap_or("None")
                ));
            }
            _ => out.missing.push(format!(
                "{rid}: status '{}' not in {}",
                value_display(row.get("status")),
                py_list_repr(&VALID_STATUS.map(String::from)),
            )),
        }

        out.feeder.push(FeederRow {
            obligation: rid,
            satisfied,
            citation,
            exempt_until: str_field(&row, "exempt_until"),
        });
    }

    // Coverage teeth: every in-force ADR must be represented
    for adr in &on_disk {
        if excluded.contains_key(adr) || covered_adrs.contains(adr) {
            continue;
        }
        out.missing.push(format!(
            "{adr}: no obligation row references this ADR — something fell out (add a row or exclude it in [meta])"
        ));
    }

    // Orphan excluded entries
    for adr in excluded.keys() {
        if !on_disk.contains(adr) {
            out.orphan
                .push(format!("[meta].excluded {adr}: no such ADR file"));
        }
    }

    Ok(out)
}

/// Feeder-JSON, byte-compatible with Python's `json.dumps(feeder, indent=2)`
/// — insertion-ordered keys, `ensure_ascii` escaping, 2-space indent.
fn feeder_json(feeder: &[FeederRow]) -> String {
    if feeder.is_empty() {
        return "[]".into();
    }
    let mut out = String::from("[\n");
    for (i, row) in feeder.iter().enumerate() {
        out.push_str("  {\n");
        let _ = writeln!(out, "    \"dimension\": \"adr-obligation\",");
        let _ = writeln!(out, "    \"obligation\": {},", json_str(&row.obligation));
        let _ = writeln!(out, "    \"satisfied\": {},", row.satisfied);
        match &row.citation {
            Citation::None => out.push_str("    \"citation\": null,\n"),
            Citation::Str(s) => {
                let _ = writeln!(out, "    \"citation\": {},", json_str(s));
            }
            Citation::List(items) if items.is_empty() => {
                out.push_str("    \"citation\": [],\n");
            }
            Citation::List(items) => {
                out.push_str("    \"citation\": [\n");
                for (j, item) in items.iter().enumerate() {
                    let comma = if j + 1 < items.len() { "," } else { "" };
                    let _ = writeln!(out, "      {}{comma}", json_str(item));
                }
                out.push_str("    ],\n");
            }
        }
        match &row.exempt_until {
            None => out.push_str("    \"exempt_until\": null\n"),
            Some(d) => {
                let _ = writeln!(out, "    \"exempt_until\": {}", json_str(d));
            }
        }
        out.push_str(if i + 1 < feeder.len() {
            "  },\n"
        } else {
            "  }\n"
        });
    }
    out.push(']');
    out
}

/// JSON string literal with Python-default `ensure_ascii=True` escaping.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if c.is_ascii() => out.push(c),
            c => {
                // Non-ASCII: \uXXXX, as a UTF-16 surrogate pair beyond the BMP
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{:04x}", unit);
                }
            }
        }
    }
    out.push('"');
    out
}

/// Local calendar date as ISO `YYYY-MM-DD` (mirrors `datetime.date.today()`).
/// Falls back to UTC if the local offset is unavailable.
fn today_local() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    )
}

/// Repo root = nearest ancestor of the cwd carrying decisions/obligations.toml,
/// falling back to the compile-time workspace location for bare invocations.
fn find_repo_root() -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        for a in cwd.ancestors() {
            if a.join("decisions").join("obligations.toml").exists() {
                return Some(a.to_path_buf());
            }
        }
    }
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    if p.join("decisions").join("obligations.toml").exists() {
        Some(p)
    } else {
        None
    }
}

/// Parse `--json PATH` / `--json=PATH`. Unknown arguments exit 2, as argparse did.
fn parse_args() -> Result<Option<String>, String> {
    let mut json: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--json" {
            match args.next() {
                Some(path) => json = Some(path),
                None => return Err("argument --json: expected one argument".into()),
            }
        } else if let Some(path) = arg.strip_prefix("--json=") {
            json = Some(path.to_owned());
        } else {
            return Err(format!("unrecognized arguments: {arg}"));
        }
    }
    Ok(json)
}

// Diagnostics deliberately go to stderr and feeder-JSON to stdout — this
// binary IS the gate's reporting surface, exactly like the Python it replaced.
fn say(s: &str) {
    eprintln!("{s}"); // guardrails-ok: stderr report is this gate binary's program output
}

fn main() -> ExitCode {
    let json = match parse_args() {
        Ok(json) => json,
        Err(e) => {
            say(&format!(
                "usage: obligations-check [--json PATH]\nerror: {e}"
            ));
            return ExitCode::from(2);
        }
    };

    let Some(repo) = find_repo_root() else {
        say("FATAL: cannot locate the repo root (decisions/obligations.toml)");
        return ExitCode::from(1);
    };

    let outcome = match evaluate(&repo, &today_local()) {
        Ok(outcome) => outcome,
        Err(e) => {
            say(&format!("FATAL: {e}"));
            return ExitCode::from(1);
        }
    };

    say(&format!(
        "ADR obligations: {} rows · {} satisfied · {} pending · {} in-force ADRs covered",
        outcome.total,
        outcome.satisfied_count(),
        outcome.pending.len(),
        outcome.in_force,
    ));
    for (label, items) in [
        ("EXPIRED EXEMPTION", &outcome.expired),
        ("ORPHAN", &outcome.orphan),
        ("UNSATISFIED", &outcome.missing),
    ] {
        for it in items {
            say(&format!("  ✗ {label}: {it}"));
        }
    }
    if !outcome.pending.is_empty() {
        say("  pending (tracked, not a failure):");
        for it in &outcome.pending {
            say(&format!("    · {it}"));
        }
    }

    if let Some(path) = json {
        let out = feeder_json(&outcome.feeder);
        if path == "-" {
            println!("{out}"); // guardrails-ok: --json - writes the feeder to stdout by contract
        } else if let Err(e) = std::fs::write(&path, out + "\n") {
            say(&format!("FATAL: cannot write {path}: {e}"));
            return ExitCode::from(1);
        }
    }

    let code = outcome.exit_code();
    if code == 0 {
        say("OK — nothing fell out of the ADRs.");
    }
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Lay down a minimal repo: decisions/ with ADR files + obligations.toml,
    /// plus a marker file a satisfied row can resolve against.
    fn fixture(obligations: &str, adr_files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let decisions = dir.path().join("decisions");
        fs::create_dir(&decisions).expect("mkdir decisions");
        for f in adr_files {
            fs::write(decisions.join(f), "# adr\n").expect("write adr");
        }
        fs::write(decisions.join("obligations.toml"), obligations).expect("write obligations");
        fs::write(dir.path().join("marker.txt"), "x\n").expect("write marker");
        dir
    }

    const BASIC: &str = r#"
[meta]
schema_version = 1
excluded = []

[[obligation]]
id = "row-satisfied"
adr = "ADR-001"
statement = "a satisfied thing"
kind = "artifact"
status = "satisfied"
satisfied_by = "marker.txt"

[[obligation]]
id = "row-pending"
adr = "ADR-001"
statement = "an open thing"
kind = "issue"
status = "pending"
tracking = "issue #1"
"#;

    #[test]
    fn parses_fixture_and_classifies_rows() {
        let dir = fixture(BASIC, &["ADR-001-first.md", "ADR-template.md"]);
        let out = evaluate(dir.path(), "2026-06-12").expect("evaluate");
        assert_eq!(out.total, 2);
        assert_eq!(out.satisfied_count(), 1);
        assert_eq!(out.pending, vec!["row-pending: issue #1"]);
        assert!(out.missing.is_empty(), "missing: {:?}", out.missing);
        assert!(out.orphan.is_empty());
        assert!(out.expired.is_empty());
        assert_eq!(out.in_force, 1, "ADR-template.md is not an ADR id");
        assert_eq!(out.exit_code(), 0);
        assert_eq!(
            out.feeder[0].citation,
            Citation::Str("marker.txt".into()),
            "satisfied citation cites satisfied_by"
        );
    }

    #[test]
    fn expired_exemption_wins_exit_3_over_missing() {
        let toml = r#"
[[obligation]]
id = "row-exempt"
adr = "ADR-001"
statement = "temporarily waived"
kind = "gate"
status = "exempt"
exempt_until = "2026-01-01"
exempt_reason = "tooling gap"

[[obligation]]
id = "row-broken"
adr = "ADR-001"
statement = "claims satisfied but cites nothing real"
kind = "test"
status = "satisfied"
satisfied_by = "does/not/exist"
"#;
        let dir = fixture(toml, &["ADR-001-first.md"]);
        let out = evaluate(dir.path(), "2026-06-12").expect("evaluate");
        assert_eq!(
            out.expired,
            vec!["row-exempt: exemption expired 2026-01-01 (tooling gap)"]
        );
        assert_eq!(
            out.missing,
            vec!["row-broken: satisfied_by does not resolve: ['does/not/exist']"]
        );
        assert_eq!(out.exit_code(), 3, "expired exemption outranks unsatisfied");
        // Boundary day: an exemption expiring today has not yet expired
        let out_on_day = evaluate(dir.path(), "2026-01-01").expect("evaluate");
        assert!(out_on_day.expired.is_empty());
    }

    #[test]
    fn uncovered_adr_and_orphan_reference_are_caught() {
        let toml = r#"
[meta]
excluded = [ { adr = "ADR-099", reason = "gone" } ]

[[obligation]]
id = "row-orphan"
adr = "ADR-042"
statement = "points nowhere"
kind = "ci"
status = "pending"
tracking = "todo"
"#;
        let dir = fixture(toml, &["ADR-001-first.md"]);
        let out = evaluate(dir.path(), "2026-06-12").expect("evaluate");
        assert_eq!(out.exit_code(), 2);
        assert_eq!(
            out.orphan,
            vec![
                "row-orphan: references ADR-042 but no decisions/ADR-042-*.md exists",
                "[meta].excluded ADR-099: no such ADR file",
            ]
        );
        assert!(out
            .missing
            .iter()
            .any(|m| m.contains("ADR-001: no obligation row references this ADR")));
    }

    #[test]
    fn feeder_json_matches_python_dumps_shape() {
        let feeder = vec![
            FeederRow {
                obligation: "row-a".into(),
                satisfied: true,
                citation: Citation::List(vec!["x.rs".into(), "y.rs".into()]),
                exempt_until: None,
            },
            FeederRow {
                obligation: "row-b".into(),
                satisfied: false,
                citation: Citation::Str("exempt until 2026-01-01 — über".into()),
                exempt_until: Some("2026-01-01".into()),
            },
        ];
        let expected = "[\n  {\n    \"dimension\": \"adr-obligation\",\n    \"obligation\": \"row-a\",\n    \"satisfied\": true,\n    \"citation\": [\n      \"x.rs\",\n      \"y.rs\"\n    ],\n    \"exempt_until\": null\n  },\n  {\n    \"dimension\": \"adr-obligation\",\n    \"obligation\": \"row-b\",\n    \"satisfied\": false,\n    \"citation\": \"exempt until 2026-01-01 \\u2014 \\u00fcber\",\n    \"exempt_until\": \"2026-01-01\"\n  }\n]";
        assert_eq!(feeder_json(&feeder), expected);
    }
}
