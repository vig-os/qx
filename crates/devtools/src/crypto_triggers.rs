//! `crypto-triggers-survey` — ADR-029 dimension 5 (trigger watchers).
//!
//! Surveys the ADR-023 "Re-open triggers" (T1–T6) so none is silently
//! resolved. ADR-023 is the SSOT for the trigger *definitions*;
//! `decisions/crypto-reopen-triggers.toml` is the watcher registry that
//! carries each trigger's explicit status. The survey cross-checks the two:
//!
//! - every trigger id named in ADR-023 has a watcher row (none silently
//!   resolved), and every watcher row maps to an ADR-023 trigger (no
//!   orphan);
//! - each watcher status is recognised (`watching` / `fired`);
//! - any `fired` trigger is surfaced loudly (its deferred controls owe
//!   activation).
//!
//! Exit codes: 0 ok · 1 drift (missing/orphan/bad-status) · 2 a trigger has
//! fired · 3 setup error (files unreadable).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(repo) = find_repo_root() else {
        eprintln!("crypto-triggers-survey: cannot locate repo root");
        return ExitCode::from(3);
    };
    match survey(&repo) {
        Ok(report) => {
            print!("{}", report.render());
            report.exit_code()
        }
        Err(e) => {
            eprintln!("crypto-triggers-survey: {e}");
            ExitCode::from(3)
        }
    }
}

/// The outcome of a survey run.
struct Report {
    /// id → status, for triggers present in BOTH the ADR and the registry.
    watched: BTreeMap<String, String>,
    /// In ADR-023 but missing a watcher row (silently resolved risk).
    missing: BTreeSet<String>,
    /// In the registry but not in ADR-023 (orphan / stale watcher).
    orphan: BTreeSet<String>,
    /// Watcher rows with an unrecognised status.
    bad_status: Vec<String>,
    /// Triggers whose status is `fired`.
    fired: Vec<String>,
}

impl Report {
    fn drifted(&self) -> bool {
        !self.missing.is_empty() || !self.orphan.is_empty() || !self.bad_status.is_empty()
    }

    fn exit_code(&self) -> ExitCode {
        if self.drifted() {
            ExitCode::from(1)
        } else if !self.fired.is_empty() {
            ExitCode::from(2)
        } else {
            ExitCode::SUCCESS
        }
    }

    fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "crypto re-open triggers: {} watched\n",
            self.watched.len()
        ));
        for (id, status) in &self.watched {
            s.push_str(&format!("  {id}: {status}\n"));
        }
        for id in &self.missing {
            s.push_str(&format!(
                "  DRIFT: {id} is defined in ADR-023 but has no watcher row (silently resolved?)\n"
            ));
        }
        for id in &self.orphan {
            s.push_str(&format!(
                "  DRIFT: {id} has a watcher row but is not defined in ADR-023 (orphan)\n"
            ));
        }
        for msg in &self.bad_status {
            s.push_str(&format!("  DRIFT: {msg}\n"));
        }
        for id in &self.fired {
            s.push_str(&format!(
                "  FIRED: {id} — its deferred control(s) owe activation (see LOG.md)\n"
            ));
        }
        if !self.drifted() && self.fired.is_empty() {
            s.push_str("OK: every ADR-023 re-open trigger is watched.\n");
        }
        s
    }
}

/// Extract the `T<n>` trigger ids defined in ADR-023's "Re-open triggers"
/// section. The SSOT lines look like `- **T1 — …**`.
fn adr_trigger_ids(adr: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in adr.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("- **T") {
            // rest begins with the number, e.g. `1 — …`.
            let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !num.is_empty() {
                ids.insert(format!("T{num}"));
            }
        }
    }
    ids
}

fn survey(repo: &Path) -> Result<Report, String> {
    let adr_path = repo
        .join("decisions")
        .join("ADR-023-threat-model-and-crypto-mvp-scope.md");
    let adr = std::fs::read_to_string(&adr_path)
        .map_err(|e| format!("read {}: {e}", adr_path.display()))?;
    let defined = adr_trigger_ids(&adr);
    if defined.is_empty() {
        return Err("no T<n> triggers found in ADR-023 (parser drift?)".into());
    }

    let reg_path = repo.join("decisions").join("crypto-reopen-triggers.toml");
    let reg_text = std::fs::read_to_string(&reg_path)
        .map_err(|e| format!("read {}: {e}", reg_path.display()))?;
    let reg: Registry =
        toml::from_str(&reg_text).map_err(|e| format!("parse crypto-reopen-triggers.toml: {e}"))?;

    let mut watched = BTreeMap::new();
    let mut bad_status = Vec::new();
    let mut fired = Vec::new();
    let registry_ids: BTreeSet<String> = reg.trigger.iter().map(|t| t.id.clone()).collect();
    for t in &reg.trigger {
        match t.status.as_str() {
            "watching" => {}
            "fired" => fired.push(t.id.clone()),
            other => bad_status.push(format!("{} has unrecognised status `{other}`", t.id)),
        }
        if defined.contains(&t.id) {
            watched.insert(t.id.clone(), t.status.clone());
        }
    }

    let missing: BTreeSet<String> = defined.difference(&registry_ids).cloned().collect();
    let orphan: BTreeSet<String> = registry_ids.difference(&defined).cloned().collect();

    Ok(Report {
        watched,
        missing,
        orphan,
        bad_status,
        fired,
    })
}

#[derive(serde::Deserialize)]
struct Registry {
    #[serde(default)]
    trigger: Vec<Trigger>,
}

#[derive(serde::Deserialize)]
struct Trigger {
    id: String,
    status: String,
    #[serde(default)]
    #[allow(dead_code)]
    summary: String,
}

/// Locate the repo root by walking up to the `decisions/` marker.
fn find_repo_root() -> Option<PathBuf> {
    if let Ok(cwd) = std::env::current_dir() {
        for a in cwd.ancestors() {
            if a.join("decisions")
                .join("crypto-reopen-triggers.toml")
                .exists()
            {
                return Some(a.to_path_buf());
            }
        }
    }
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.join("decisions")
        .join("crypto-reopen-triggers.toml")
        .exists()
        .then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr_trigger_ids_extracts_t1_through_t6() {
        let adr = "## Re-open triggers\n\
                   - **T1 — A.** ...\n\
                   - **T2 — B.** ...\n\
                   - **T6 — F.** ...\n\
                   - not a trigger line\n";
        let ids = adr_trigger_ids(adr);
        assert!(ids.contains("T1") && ids.contains("T2") && ids.contains("T6"));
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn shipped_registry_covers_every_adr_trigger() {
        // The real survey over the shipped files must be clean (no drift,
        // nothing fired): every ADR-023 trigger is watched.
        let repo = find_repo_root().expect("repo root");
        let report = survey(&repo).expect("survey runs");
        assert!(
            !report.drifted(),
            "shipped survey drifted: {}",
            report.render()
        );
        assert!(report.fired.is_empty(), "a trigger is marked fired");
        assert_eq!(report.watched.len(), 6, "expected T1-T6 watched");
    }
}
