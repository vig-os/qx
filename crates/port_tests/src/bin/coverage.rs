//! Architectural coverage validator per ADR-029.
//!
//! Validates that workspace-level architectural obligations are met:
//! crate coverage, port conformance, SOUP inventory, ADR status.
//!
//! Exit codes:
//!   0 — all checks pass
//!   1 — at least one check failed

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Warn,
    Fail,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Pass => write!(f, "PASS"),
            Status::Warn => write!(f, "WARN"),
            Status::Fail => write!(f, "FAIL"),
        }
    }
}

#[derive(Debug)]
struct CheckResult {
    dimension: &'static str,
    status: Status,
    details: String,
}

// ---------------------------------------------------------------------------
// SOUP inventory schema (subset needed for coverage check)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SoupInventory {
    #[allow(dead_code)]
    schema_version: u32,
    #[serde(rename = "crate", default)]
    crates: Vec<SoupCrate>,
}

#[derive(Debug, Deserialize)]
struct SoupCrate {
    name: String,
    class: u32,
    #[serde(default)]
    validation_harness: Option<String>,
}

// ---------------------------------------------------------------------------
// Workspace Cargo.toml parsing
// ---------------------------------------------------------------------------

fn find_repo_root() -> PathBuf {
    // Walk up from the binary's location or CWD to find the workspace root
    // (the directory containing the top-level Cargo.toml with [workspace]).
    let mut dir = std::env::current_dir().expect("cannot determine CWD");
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            if let Ok(contents) = std::fs::read_to_string(&candidate) {
                if contents.contains("[workspace]") {
                    return dir;
                }
            }
        }
        if !dir.pop() {
            eprintln!("error: could not find workspace root (no Cargo.toml with [workspace])");
            process::exit(2);
        }
    }
}

fn parse_workspace_members(root: &Path) -> Vec<String> {
    let cargo_toml = root.join("Cargo.toml");
    let contents = std::fs::read_to_string(&cargo_toml).unwrap_or_else(|e| {
        eprintln!("error: cannot read {}: {e}", cargo_toml.display());
        process::exit(2);
    });
    let doc: toml::Value = toml::from_str(&contents).unwrap_or_else(|e| {
        eprintln!("error: cannot parse {}: {e}", cargo_toml.display());
        process::exit(2);
    });
    doc.get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Dimension 1: Crate coverage
// ---------------------------------------------------------------------------

/// Port crate names and the prefix their adapters share.
const PORT_CRATES: &[(&str, &str)] = &[
    ("storage", "storage_"),
    ("identity", "identity_"),
    ("transport", "transport_"),
    ("signing", "signing_"),
];

fn check_crate_coverage(root: &Path, members: &[String]) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let mut missing_entry = Vec::new();
    let mut ports_without_adapter: Vec<String> = Vec::new();

    for member in members {
        let crate_dir = root.join(member);
        let has_lib = crate_dir.join("src/lib.rs").exists();
        let has_main = crate_dir.join("src/main.rs").exists();
        if !has_lib && !has_main {
            missing_entry.push(member.clone());
        }
    }

    if missing_entry.is_empty() {
        results.push(CheckResult {
            dimension: "Crate entry points",
            status: Status::Pass,
            details: format!(
                "all {} crates have src/lib.rs or src/main.rs",
                members.len()
            ),
        });
    } else {
        results.push(CheckResult {
            dimension: "Crate entry points",
            status: Status::Fail,
            details: format!("missing entry point: {}", missing_entry.join(", ")),
        });
    }

    // Extract just the crate directory name (last path component) for matching.
    let crate_names: Vec<&str> = members
        .iter()
        .filter_map(|m| m.rsplit('/').next())
        .collect();

    for (port, prefix) in PORT_CRATES {
        let adapters: Vec<&&str> = crate_names
            .iter()
            .filter(|name| name.starts_with(prefix) && **name != *port)
            .collect();
        if adapters.is_empty() {
            ports_without_adapter.push((*port).to_string());
        }
    }

    if ports_without_adapter.is_empty() {
        results.push(CheckResult {
            dimension: "Port adapters",
            status: Status::Pass,
            details: "every port crate has at least one adapter".into(),
        });
    } else {
        results.push(CheckResult {
            dimension: "Port adapters",
            status: Status::Fail,
            details: format!(
                "ports without adapters: {}",
                ports_without_adapter.join(", ")
            ),
        });
    }

    results
}

// ---------------------------------------------------------------------------
// Dimension 2: Port conformance
// ---------------------------------------------------------------------------

fn check_port_conformance(root: &Path, members: &[String]) -> Vec<CheckResult> {
    let crate_names: Vec<&str> = members
        .iter()
        .filter_map(|m| m.rsplit('/').next())
        .collect();

    let mut missing_conformance = Vec::new();

    for (port, prefix) in PORT_CRATES {
        let adapters: Vec<&&str> = crate_names
            .iter()
            .filter(|name| name.starts_with(prefix) && **name != *port)
            .collect();

        for adapter in adapters {
            let conformance_path = root
                .join("crates")
                .join(adapter)
                .join("tests/conformance.rs");
            if !conformance_path.exists() {
                missing_conformance.push((*adapter).to_string());
            }
        }
    }

    if missing_conformance.is_empty() {
        vec![CheckResult {
            dimension: "Port conformance",
            status: Status::Pass,
            details: "all adapter crates have tests/conformance.rs".into(),
        }]
    } else {
        vec![CheckResult {
            dimension: "Port conformance",
            status: Status::Fail,
            details: format!(
                "missing tests/conformance.rs: {}",
                missing_conformance.join(", ")
            ),
        }]
    }
}

// ---------------------------------------------------------------------------
// Dimension 3: SOUP coverage
// ---------------------------------------------------------------------------

fn check_soup_coverage(root: &Path) -> Vec<CheckResult> {
    let inventory_path = root.join("soup/inventory.toml");
    if !inventory_path.exists() {
        return vec![CheckResult {
            dimension: "SOUP coverage",
            status: Status::Warn,
            details: "soup/inventory.toml not found; SOUP checks skipped".into(),
        }];
    }

    let contents = match std::fs::read_to_string(&inventory_path) {
        Ok(c) => c,
        Err(e) => {
            return vec![CheckResult {
                dimension: "SOUP coverage",
                status: Status::Warn,
                details: format!("cannot read soup/inventory.toml: {e}"),
            }];
        }
    };

    let inventory: SoupInventory = match toml::from_str(&contents) {
        Ok(inv) => inv,
        Err(e) => {
            return vec![CheckResult {
                dimension: "SOUP coverage",
                status: Status::Fail,
                details: format!("cannot parse soup/inventory.toml: {e}"),
            }];
        }
    };

    let class3: Vec<&SoupCrate> = inventory.crates.iter().filter(|c| c.class == 3).collect();

    if class3.is_empty() {
        return vec![CheckResult {
            dimension: "SOUP coverage",
            status: Status::Pass,
            details: "no Class 3 SOUP entries".into(),
        }];
    }

    let missing: Vec<&str> = class3
        .iter()
        .filter(|c| {
            c.validation_harness
                .as_ref()
                .is_none_or(|h| h.trim().is_empty())
        })
        .map(|c| c.name.as_str())
        .collect();

    if missing.is_empty() {
        vec![CheckResult {
            dimension: "SOUP coverage",
            status: Status::Pass,
            details: format!(
                "all {} Class 3 SOUP entries have a validation_harness",
                class3.len()
            ),
        }]
    } else {
        vec![CheckResult {
            dimension: "SOUP coverage",
            status: Status::Fail,
            details: format!(
                "Class 3 entries missing validation_harness: {}",
                missing.join(", ")
            ),
        }]
    }
}

// ---------------------------------------------------------------------------
// Dimension 4: ADR coverage
// ---------------------------------------------------------------------------

fn check_adr_coverage(root: &Path) -> Vec<CheckResult> {
    let decisions_dir = root.join("decisions");
    if !decisions_dir.is_dir() {
        return vec![CheckResult {
            dimension: "ADR coverage",
            status: Status::Warn,
            details: "decisions/ directory not found".into(),
        }];
    }

    let pattern = decisions_dir.join("ADR-*.md").to_string_lossy().to_string();

    let paths: Vec<PathBuf> = glob::glob(&pattern)
        .unwrap_or_else(|e| {
            eprintln!("error: bad glob pattern: {e}");
            process::exit(2);
        })
        .filter_map(|r| r.ok())
        .filter(|p| {
            // Exclude the template
            p.file_name().is_some_and(|n| n != "ADR-template.md")
        })
        .collect();

    if paths.is_empty() {
        return vec![CheckResult {
            dimension: "ADR coverage",
            status: Status::Warn,
            details: "no ADR-*.md files found".into(),
        }];
    }

    let mut missing_status = Vec::new();
    let mut status_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0;

    for path in &paths {
        total += 1;
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                missing_status.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                );
                continue;
            }
        };

        // Parse the status from frontmatter-style markdown list.
        // Format: `- Status: <value>` in the first ~20 lines.
        let status_val = contents.lines().take(20).find_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix("- Status:")
                .map(|rest| rest.trim().to_string())
        });

        match status_val {
            Some(s) if !s.is_empty() => {
                let key = s.to_lowercase();
                *status_counts.entry(key).or_insert(0) += 1;
            }
            _ => {
                missing_status.push(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }
    }

    let counts_str = status_counts
        .iter()
        .map(|(k, v)| format!("{v} {k}"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut results = Vec::new();

    if missing_status.is_empty() {
        results.push(CheckResult {
            dimension: "ADR status fields",
            status: Status::Pass,
            details: format!("{total} ADRs scanned; {counts_str}"),
        });
    } else {
        results.push(CheckResult {
            dimension: "ADR status fields",
            status: Status::Fail,
            details: format!(
                "ADRs missing status: {}; rest: {counts_str}",
                missing_status.join(", ")
            ),
        });
    }

    results
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

fn print_markdown_table(results: &[CheckResult]) {
    println!();
    println!("## Architectural Coverage Matrix");
    println!();
    println!("| Dimension | Status | Details |");
    println!("|-----------|--------|---------|");
    for r in results {
        println!("| {} | {} | {} |", r.dimension, r.status, r.details);
    }
    println!();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let root = find_repo_root();
    let members = parse_workspace_members(&root);

    let mut all_results: Vec<CheckResult> = Vec::new();

    // 1. Crate coverage
    all_results.extend(check_crate_coverage(&root, &members));

    // 2. Port conformance
    all_results.extend(check_port_conformance(&root, &members));

    // 3. SOUP coverage
    all_results.extend(check_soup_coverage(&root));

    // 4. ADR coverage
    all_results.extend(check_adr_coverage(&root));

    print_markdown_table(&all_results);

    let has_fail = all_results.iter().any(|r| r.status == Status::Fail);
    if has_fail {
        eprintln!("coverage: FAIL — see table above for details");
        process::exit(1);
    } else {
        let warn_count = all_results
            .iter()
            .filter(|r| r.status == Status::Warn)
            .count();
        if warn_count > 0 {
            eprintln!("coverage: PASS with {warn_count} warning(s)");
        } else {
            eprintln!("coverage: PASS");
        }
    }
}
