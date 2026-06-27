//! Persona resolution (ADR-036 §1/§2): the `personas` collection is the
//! identity registry, and accountability principals must resolve to an
//! **active** persona.
//!
//! Three engine checks key off one [`PersonaIndex`]:
//!
//! - **audit-operator FK** — every audit `operator` must resolve to a
//!   declared persona (ADR-036 §1: "the audit operator field is a typed
//!   FK into [personas]").
//! - **CODEOWNERS principals** — every `@login` named in CODEOWNERS must
//!   resolve to an *active* persona (ADR-036 §2 CI cross-check).
//! - **merge approvers** — every approver login on a merged PR must
//!   resolve to an *active* persona (same resolver, fed the host's
//!   approver list at CI time).
//!
//! Resolution is pure and host-free; the only host glue is *fetching* the
//! approver logins (the resolver itself is fully testable offline).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::record::{RecordIssue, Severity};

/// A persona's lifecycle status as seen by the resolver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonaStatus {
    Active,
    /// `suspended` or `revoked` — present in the registry but not active.
    Inactive,
}

/// An index over a `personas` collection: persona id + github_login →
/// status. Built once, queried by every accountability check.
#[derive(Debug, Default)]
pub struct PersonaIndex {
    by_id: BTreeMap<String, PersonaStatus>,
    by_login: BTreeMap<String, (String, PersonaStatus)>,
}

impl PersonaIndex {
    /// Build the index from the `personas` collection's records. A record
    /// is keyed by its envelope `id`; `github_login` (when present) gives
    /// the host-login alias. `status == "active"` is the only active
    /// state; everything else (suspended/revoked/missing) is inactive.
    pub fn from_records(records: &[Map<String, Value>]) -> Self {
        let mut idx = PersonaIndex::default();
        for rec in records {
            let Some(id) = rec.get("id").and_then(Value::as_str) else {
                continue;
            };
            let status = match rec.get("status").and_then(Value::as_str) {
                Some("active") => PersonaStatus::Active,
                _ => PersonaStatus::Inactive,
            };
            idx.by_id.insert(id.to_string(), status);
            if let Some(login) = rec.get("github_login").and_then(Value::as_str) {
                if !login.is_empty() {
                    idx.by_login
                        .insert(login.to_string(), (id.to_string(), status));
                }
            }
        }
        idx
    }

    /// Resolve a principal — a persona id (`persona:slug`) or a bare
    /// github login — to its persona id, returning the resolution status.
    /// `None` means the principal is unknown to the registry.
    pub fn resolve(&self, principal: &str) -> Option<(&str, PersonaStatus)> {
        if let Some((id, status)) = self.by_id.get_key_value(principal) {
            return Some((id.as_str(), *status));
        }
        // Accept a leading `@` on host logins (CODEOWNERS syntax).
        let login = principal.strip_prefix('@').unwrap_or(principal);
        self.by_login
            .get(login)
            .map(|(id, status)| (id.as_str(), *status))
    }
}

/// Resolve one accountability principal to an **active** persona, pushing
/// an error issue (under `path`) when it is unknown or inactive.
fn require_active(
    idx: &PersonaIndex,
    principal: &str,
    path: &str,
    role: &str,
    out: &mut Vec<RecordIssue>,
) {
    match idx.resolve(principal) {
        Some((_, PersonaStatus::Active)) => {}
        Some((id, PersonaStatus::Inactive)) => out.push(RecordIssue {
            path: path.to_string(),
            message: format!(
                "{role} `{principal}` resolves to persona `{id}` but it is not active \
                 (suspended/revoked) — accountability requires an active persona"
            ),
            severity: Severity::Error,
        }),
        None => out.push(RecordIssue {
            path: path.to_string(),
            message: format!(
                "{role} `{principal}` does not resolve to any declared persona \
                 (ADR-036 §2: unknown principal blocks, it does not silently grant)"
            ),
            severity: Severity::Error,
        }),
    }
}

/// Audit-operator FK (ADR-036 §1): every audit `operator` must resolve to
/// a declared persona. A revoked persona still *resolves* (the FK holds —
/// the act happened while they were valid), so this is referential
/// integrity, not an active-state gate; use [`approver_resolution_issues`]
/// for the active-state accountability gate.
pub fn audit_operator_fk_issues(idx: &PersonaIndex, operators: &[&str]) -> Vec<RecordIssue> {
    let mut out = Vec::new();
    for op in operators {
        if idx.resolve(op).is_none() {
            out.push(RecordIssue {
                path: "audit.operator".to_string(),
                message: format!(
                    "audit operator `{op}` is not a declared persona \
                     (ADR-036 §1: the operator field is a typed FK into personas)"
                ),
                severity: Severity::Error,
            });
        }
    }
    out
}

/// CODEOWNERS principals (ADR-036 §2): every `@login` named on a code-path
/// line must resolve to an active persona. Comment lines (`#…`) and the
/// path glob are skipped; only the principals are resolved.
pub fn codeowners_principal_issues(idx: &PersonaIndex, codeowners: &str) -> Vec<RecordIssue> {
    let mut out = Vec::new();
    for raw in codeowners.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `…path  @owner-a @team/x` — principals are the `@`-prefixed
        // tokens after the path glob.
        for principal in line.split_whitespace().filter(|t| t.starts_with('@')) {
            // Team handles such as org-slash-team are host groups rather
            // than personas, so only individual logins resolve. A team is
            // skipped here and its members resolve host-side at review time.
            if principal.contains('/') {
                continue;
            }
            require_active(
                idx,
                principal,
                "codeowners",
                "CODEOWNERS principal",
                &mut out,
            );
        }
    }
    out
}

/// Merge approvers (ADR-036 §2): every approver login on a merged PR must
/// resolve to an active persona. The login list is the host's approver
/// set (fetched at CI time); resolution is this pure function.
pub fn approver_resolution_issues(
    idx: &PersonaIndex,
    approver_logins: &[&str],
) -> Vec<RecordIssue> {
    let mut out = Vec::new();
    for login in approver_logins {
        require_active(idx, login, "approver", "merge approver", &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn personas() -> Vec<Map<String, Value>> {
        ["active", "suspended", "revoked"]
            .iter()
            .map(|status| {
                let mut m = Map::new();
                m.insert("id".into(), Value::String(format!("persona:{status}-one")));
                m.insert("status".into(), Value::String((*status).into()));
                m.insert("github_login".into(), Value::String(format!("{status}gh")));
                m
            })
            .collect()
    }

    #[test]
    fn resolves_by_id_and_by_login() {
        let idx = PersonaIndex::from_records(&personas());
        assert!(matches!(
            idx.resolve("persona:active-one"),
            Some((_, PersonaStatus::Active))
        ));
        assert!(matches!(
            idx.resolve("@activegh"),
            Some(("persona:active-one", PersonaStatus::Active))
        ));
        assert!(matches!(
            idx.resolve("suspendedgh"),
            Some((_, PersonaStatus::Inactive))
        ));
        assert!(idx.resolve("ghost").is_none());
    }

    #[test]
    fn audit_operator_fk_rejects_unknown_only() {
        let idx = PersonaIndex::from_records(&personas());
        // Known (even revoked) operators satisfy the FK; only unknowns fail.
        assert!(audit_operator_fk_issues(&idx, &["persona:revoked-one", "@activegh"]).is_empty());
        let issues = audit_operator_fk_issues(&idx, &["github:nobody"]);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("typed FK into personas"));
    }

    #[test]
    fn codeowners_principals_must_be_active() {
        let idx = PersonaIndex::from_records(&personas());
        let owners = "# audit path\n\
                      /collections/personas.jsonl  @activegh @suspendedgh\n\
                      *  @org/admins @activegh\n";
        let issues = codeowners_principal_issues(&idx, owners);
        // The suspended login yields one error, the team handle is
        // skipped, and the active login passes — so exactly one issue.
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("not active"));
    }

    #[test]
    fn approvers_must_resolve_to_active_persona() {
        let idx = PersonaIndex::from_records(&personas());
        assert!(approver_resolution_issues(&idx, &["activegh"]).is_empty());
        let issues = approver_resolution_issues(&idx, &["revokedgh", "stranger"]);
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().any(|i| i.message.contains("not active")));
        assert!(issues
            .iter()
            .any(|i| i.message.contains("does not resolve")));
    }
}
