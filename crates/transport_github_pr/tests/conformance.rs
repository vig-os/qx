//! ADR-027 §Tier 1 — `ProposalSink` conformance for
//! `GithubPrProposalSink`. The shared
//! `port_tests::proposal_sink_conformance` body is still a stub
//! (foundation scaffold); this file wires the adapter into it so the
//! drift surface exists and additional adapter-specific assertions
//! that match the documented contract land here.

use std::collections::BTreeMap;
use std::sync::Mutex;

use part_registry_domain::{
    Diff, DiffRow, IdentitySource, KeyId, Operator, OperatorId, PartId, Proposal, ProposalRef,
    RequestId,
};
use part_registry_port_tests::proposal_sink_conformance;
use part_registry_transport::ProposalSink;
use part_registry_transport_github_pr::{
    CheckRun, CheckRunsResponse, CreatePullRequest, CreateRefRequest, GetContentsResponse,
    GitRefObject, GitRefResponse, GithubPrConfig, GithubPrHttp, GithubPrProposalSink, HttpError,
    PullResponse, PullReview, PutContentsRequest,
};

#[derive(Default)]
struct StaticHttp {
    pull_state: Mutex<String>,
    pull_merged: Mutex<bool>,
}

impl GithubPrHttp for StaticHttp {
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
        _p: &str,
        _ref_: &str,
    ) -> Result<Option<GetContentsResponse>, HttpError> {
        Ok(None)
    }
    fn put_contents(
        &self,
        _o: &str,
        _r: &str,
        _p: &str,
        _body: &PutContentsRequest,
    ) -> Result<(), HttpError> {
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
        let state = self.pull_state.lock().unwrap().clone();
        let state = if state.is_empty() {
            "open".to_owned()
        } else {
            state
        };
        Ok(PullResponse {
            number: n,
            html_url: format!("https://github.com/exo-pet/exopet-registry/pull/{}", n),
            state,
            merged_at: if *self.pull_merged.lock().unwrap() {
                Some("2026-05-11T12:00:00Z".into())
            } else {
                None
            },
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
            check_runs: Vec::<CheckRun>::new(),
        })
    }
    fn get_reviews(&self, _o: &str, _r: &str, _n: u64) -> Result<Vec<PullReview>, HttpError> {
        Ok(vec![])
    }
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
    let mut claims = BTreeMap::new();
    claims.insert("github_login".into(), "tester".into());
    Operator {
        id: OperatorId("github:tester".into()),
        display_name: "Tester".into(),
        source: IdentitySource::GitHubOAuth,
        verified_at: None,
        claims,
        pubkey: Some(KeyId("k1".into())),
    }
}

fn empty_proposal() -> Proposal {
    let mut fields = BTreeMap::new();
    fields.insert("status".into(), "unbound".into());
    fields.insert("minted_at".into(), "2026-05-11T12:00:00Z".into());
    let diff = Diff {
        adds: vec![DiffRow {
            id: Some(PartId::new("ABCDEFGHJKMNPQ").unwrap()),
            fields,
        }],
        ..Default::default()
    };
    let actions = diff.classify();
    Proposal {
        diff,
        batch_label: None,
        author: operator(),
        signatures: vec![],
        change_classification: actions,
        message: "conformance test".into(),
        request_id: RequestId(uuid::Uuid::from_u128(1)),
    }
}

#[test]
fn github_pr_sink_passes_generic_conformance() {
    let sink = GithubPrProposalSink::new(StaticHttp::default(), config());
    proposal_sink_conformance(&sink, empty_proposal());
}

#[test]
fn github_pr_sink_submit_returns_well_formed_proposal_ref() {
    let sink = GithubPrProposalSink::new(StaticHttp::default(), config());
    let r = sink.submit(empty_proposal()).expect("submit ok");
    assert_eq!(r.adapter, "github_pr");
    assert!(r.url.contains("/pull/"));
    assert!(r.local_id.as_deref().unwrap().starts_with("proposal/"));
}

#[test]
fn github_pr_sink_status_round_trips_open_state() {
    let http = StaticHttp::default();
    *http.pull_state.lock().unwrap() = "open".to_owned();
    let sink = GithubPrProposalSink::new(http, config());
    let pref = ProposalRef {
        url: "https://github.com/exo-pet/exopet-registry/pull/99".into(),
        local_id: Some("proposal/anything".into()),
        adapter: "github_pr".into(),
    };
    let s = sink.status(&pref).expect("status ok");
    // No checks, no requested reviewers → Open.
    assert!(matches!(s, part_registry_domain::ProposalStatus::Open));
}
