//! ADR-027 §Tier 3 — cross-adapter **parity**: the real
//! `GithubPrProposalSink` (CSV-over-git substrate, driven by a recording
//! HTTP fake) vs. `TableSink` (in-memory relational substrate).
//!
//! Spike #189 D2: this is the proof that had never run. It asserts the
//! substrate-independence ADR-019/027 *assert* — that the same proposal,
//! applied from the same base, reaches the same logical registry state
//! regardless of whether the backend is CSV-on-git or a relational table —
//! and it agrees on accept-vs-reject. Header changes / conflict shape /
//! error text are declared substrate-visible (the re-scoped parity
//! contract in `port_tests::proposal_sink_parity`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use base64::Engine;

use part_registry_domain::{
    Diff, DiffEdit, DiffRow, IdentitySource, KeyId, Operator, OperatorId, PartId, Proposal,
    RequestId,
};
use part_registry_port_tests::{proposal_sink_parity, RegistryState};
use part_registry_transport::ProposalSink;
use part_registry_transport_github_pr::{
    CheckRunsResponse, CreatePullRequest, CreateRefRequest, GetContentsResponse, GitRefObject,
    GitRefResponse, GithubPrConfig, GithubPrHttp, GithubPrProposalSink, HttpError, PullResponse,
    PullReview, PutContentsRequest,
};

// Representative registry header for the parity corpus. The CSV adapter is
// header-bound (it only emits columns in this header); the corpus keeps
// every field within these columns so the only differences under test are
// the *merge semantics*, not CSV's rectangular-ness (which is a separate,
// substrate-visible property — adding a new column needs a HeaderChange).
const HEADER: &[&str] = &["id", "status", "type", "vendor", "notes"];

// -------------------------------------------------------------------
// Recording GitHub HTTP fake — returns a fixed base CSV and captures the
// CSV that `submit` would PUT, so we can read the CSV adapter's resulting
// state without a network or a real merge.
// -------------------------------------------------------------------

struct RecordingHttp {
    base_csv: String,
    captured: Arc<Mutex<Option<String>>>,
}

impl GithubPrHttp for RecordingHttp {
    fn get_branch_ref(&self, _o: &str, _r: &str, _b: &str) -> Result<GitRefResponse, HttpError> {
        Ok(GitRefResponse {
            ref_: "refs/heads/main".into(),
            object: GitRefObject {
                sha: "main-sha".into(),
                type_: "commit".into(),
            },
        })
    }
    fn create_ref(
        &self,
        _o: &str,
        _r: &str,
        body: &CreateRefRequest,
    ) -> Result<GitRefResponse, HttpError> {
        Ok(GitRefResponse {
            ref_: body.ref_.clone(),
            object: GitRefObject {
                sha: "branch-sha".into(),
                type_: "commit".into(),
            },
        })
    }
    fn get_contents(
        &self,
        _o: &str,
        _r: &str,
        p: &str,
        _ref_: &str,
    ) -> Result<Option<GetContentsResponse>, HttpError> {
        // Seed only the registry file; other files are absent.
        if p.contains("registry") && !self.base_csv.is_empty() {
            Ok(Some(GetContentsResponse {
                sha: "base-sha".into(),
                content: b64(&self.base_csv),
                encoding: "base64".into(),
                path: p.to_owned(),
            }))
        } else {
            Ok(None)
        }
    }
    fn put_contents(
        &self,
        _o: &str,
        _r: &str,
        p: &str,
        body: &PutContentsRequest,
    ) -> Result<(), HttpError> {
        if p.contains("registry") {
            let csv = String::from_utf8(
                base64::engine::general_purpose::STANDARD
                    .decode(body.content.replace('\n', ""))
                    .expect("valid base64 PUT content"),
            )
            .expect("utf8 CSV");
            *self.captured.lock().unwrap() = Some(csv);
        }
        Ok(())
    }
    fn create_pull(
        &self,
        _o: &str,
        _r: &str,
        _body: &CreatePullRequest,
    ) -> Result<PullResponse, HttpError> {
        Ok(PullResponse {
            number: 1,
            html_url: "https://github.com/exo-pet/exopet-registry/pull/1".into(),
            state: "open".into(),
            merged_at: None,
            requested_reviewers: vec![],
        })
    }
    fn get_pull(&self, _o: &str, _r: &str, n: u64) -> Result<PullResponse, HttpError> {
        Ok(PullResponse {
            number: n,
            html_url: format!("https://github.com/exo-pet/exopet-registry/pull/{n}"),
            state: "open".into(),
            merged_at: None,
            requested_reviewers: vec![],
        })
    }
    fn get_check_runs(
        &self,
        _o: &str,
        _r: &str,
        _ref_: &str,
    ) -> Result<CheckRunsResponse, HttpError> {
        Ok(CheckRunsResponse {
            total_count: 0,
            check_runs: vec![],
        })
    }
    fn get_reviews(&self, _o: &str, _r: &str, _n: u64) -> Result<Vec<PullReview>, HttpError> {
        Ok(vec![])
    }
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

fn config() -> GithubPrConfig {
    GithubPrConfig {
        data_repo_owner: "exo-pet".into(),
        data_repo_name: "exopet-registry".into(),
        base_branch: "main".into(),
        branch_prefix: "proposal/".into(),
        commit_author_name: "bot".into(),
        commit_author_email: "bot@example".into(),
    }
}

fn operator() -> Operator {
    Operator {
        id: OperatorId("github:tester".into()),
        display_name: "Tester".into(),
        source: IdentitySource::GitHubOAuth,
        verified_at: None,
        claims: BTreeMap::new(),
        pubkey: Some(KeyId("k1".into())),
    }
}

fn proposal(diff: Diff, n: u128) -> Proposal {
    let actions = diff.classify();
    Proposal {
        diff,
        batch_label: None,
        author: operator(),
        signatures: vec![],
        change_classification: actions,
        message: format!("parity proposal {n}"),
        request_id: RequestId(uuid::Uuid::from_u128(n)),
    }
}

fn pid(s: &str) -> PartId {
    PartId::new(s).expect("valid part id")
}

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

// -------------------------------------------------------------------
// State <-> CSV helpers
// -------------------------------------------------------------------

fn state_to_csv(base: &RegistryState) -> String {
    let mut out = String::new();
    out.push_str(&HEADER.join(","));
    out.push('\n');
    for (_id, row) in base {
        let line: Vec<String> = HEADER
            .iter()
            .map(|h| row.get(*h).cloned().unwrap_or_default())
            .collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

fn csv_to_state(csv: &str) -> RegistryState {
    let mut lines = csv.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split(',').collect();
    let mut state = RegistryState::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let cells: Vec<&str> = line.split(',').collect();
        let mut row = BTreeMap::new();
        for (i, h) in header.iter().enumerate() {
            row.insert(
                (*h).to_owned(),
                cells.get(i).copied().unwrap_or("").to_owned(),
            );
        }
        let id = row.get("id").cloned().unwrap_or_default();
        if !id.is_empty() {
            state.insert(id, row);
        }
    }
    state
}

// -------------------------------------------------------------------
// apply_* closures: drive a real ProposalSink from a common base and
// return its resulting RegistryState.
// -------------------------------------------------------------------

fn apply_github(base: &RegistryState, p: &Proposal) -> Result<RegistryState, String> {
    let captured = Arc::new(Mutex::new(None));
    let http = RecordingHttp {
        base_csv: state_to_csv(base),
        captured: captured.clone(),
    };
    let sink = GithubPrProposalSink::new(http, config());
    sink.submit(p.clone()).map_err(|e| e.to_string())?;
    let csv = captured
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "github adapter never PUT registry.csv".to_string())?;
    Ok(csv_to_state(&csv))
}

fn apply_table(base: &RegistryState, p: &Proposal) -> Result<RegistryState, String> {
    let sink = part_registry_transport_table::TableSink::with_base(base.clone());
    sink.submit(p.clone()).map_err(|e| e.to_string())?;
    Ok(sink.state())
}

fn base_state() -> RegistryState {
    let mut s = RegistryState::new();
    s.insert(
        "ABCDEFGHJKMNPQ".into(),
        fields(&[
            ("id", "ABCDEFGHJKMNPQ"),
            ("status", "unbound"),
            ("type", "Sensor"),
            ("vendor", "Acme"),
        ]),
    );
    s
}

// -------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------

#[test]
fn github_pr_and_table_agree_on_clean_corpus() {
    let base = base_state();

    // P1: add a new row. P2: bind the existing row. P3: void it.
    let add = Diff {
        adds: vec![DiffRow {
            id: Some(pid("BCDEFGHJKMNPQR")),
            fields: fields(&[("status", "unbound"), ("type", "Cable")]),
        }],
        ..Default::default()
    };
    let bind = Diff {
        edits: vec![DiffEdit {
            id: pid("ABCDEFGHJKMNPQ"),
            before: fields(&[("status", "unbound")]),
            after: fields(&[("status", "bound"), ("notes", "bound now")]),
            changed_keys: vec!["status".into(), "notes".into()],
        }],
        ..Default::default()
    };
    let void = Diff {
        edits: vec![DiffEdit {
            id: pid("ABCDEFGHJKMNPQ"),
            before: fields(&[("status", "unbound")]),
            after: fields(&[("status", "void"), ("notes", "broken")]),
            changed_keys: vec!["status".into(), "notes".into()],
        }],
        ..Default::default()
    };

    // P4: clear an existing column (empty `after`). The two substrates
    // handle this differently at the byte level — the table *removes* the
    // column, the CSV adapter writes an empty cell — but they must agree at
    // the *logical* level the parity contract is scoped to (2nd-round
    // review finding: previously undodged because no corpus proposal set an
    // empty `after`).
    let clear = Diff {
        edits: vec![DiffEdit {
            id: pid("ABCDEFGHJKMNPQ"),
            before: fields(&[("vendor", "Acme")]),
            after: fields(&[("vendor", "")]),
            changed_keys: vec!["vendor".into()],
        }],
        ..Default::default()
    };

    let corpus = vec![
        proposal(add, 1),
        proposal(bind, 2),
        proposal(void, 3),
        proposal(clear, 4),
    ];

    // The substrate-independence proof: same proposals, same base, same
    // logical state on a CSV-over-git backend and a relational backend.
    proposal_sink_parity(&base, &corpus, apply_github, apply_table);
}

#[test]
fn github_pr_and_table_agree_on_rejecting_a_registry_delete() {
    // ADR-012: registry never deletes. Both substrates must reject the
    // same illegal diff — accept/reject agreement is a parity property.
    let base = base_state();
    let delete = Diff {
        deletes: vec![DiffRow {
            id: Some(pid("ABCDEFGHJKMNPQ")),
            fields: BTreeMap::new(),
        }],
        ..Default::default()
    };
    let corpus = vec![proposal(delete, 9)];
    proposal_sink_parity(&base, &corpus, apply_github, apply_table);
}

#[test]
fn row_to_container_routing_is_substrate_visible_leak3() {
    // Spike #189, 2nd-round review finding: the `Diff` does NOT carry which
    // container a row targets — the CSV adapter re-derives it from row shape
    // via `classify_row` (leak #3). A print-log-shaped row therefore routes
    // to `print_log.csv` in the CSV adapter but lands in the single table in
    // `TableSink`. This is a *real divergence*, so it is deliberately kept
    // OUT of `proposal_sink_parity` (which would correctly panic on it) and
    // asserted here as the known, documented substrate-visible boundary.
    // Follow-up: put container routing in the contract (the `Diff`), not in
    // a per-adapter field-shape heuristic.
    let base = base_state();
    let print_row = Diff {
        adds: vec![DiffRow {
            id: Some(pid("BCDEFGHJKMNPQR")),
            fields: fields(&[("printed_at", "2026-06-01T00:00:00Z"), ("layout", "horz")]),
        }],
        ..Default::default()
    };
    let p = proposal(print_row, 11);

    let gh = apply_github(&base, &p);
    let tbl = apply_table(&base, &p).expect("table apply ok");

    // CSV adapter: the row is routed away from registry.csv, so registry
    // never gains it (no registry PUT, or an unchanged-registry PUT).
    let gh_has_row = gh
        .as_ref()
        .map(|s| s.contains_key("BCDEFGHJKMNPQR"))
        .unwrap_or(false);
    assert!(
        !gh_has_row,
        "leak #3: CSV adapter routes the print-log-shaped row away from \
         registry.csv via classify_row"
    );
    // Table adapter: no container routing exists in the contract, so the
    // row lands in its one table.
    assert!(
        tbl.contains_key("BCDEFGHJKMNPQR"),
        "leak #3: table adapter has no routing — the row lands in its table"
    );
}
