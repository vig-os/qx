//! ADR-027 §Tier 1 — `ProposalSink` conformance for `TableSink`.
//! Wires the non-CSV adapter into the generic
//! `port_tests::proposal_sink_conformance` suite (spike #189).

use std::collections::BTreeMap;

use part_registry_domain::{
    Diff, DiffRow, IdentitySource, Operator, OperatorId, PartId, Proposal, RequestId,
};
use part_registry_port_tests::proposal_sink_conformance;
use part_registry_transport_table::TableSink;

fn sample() -> Proposal {
    let diff = Diff {
        adds: vec![DiffRow {
            id: Some(PartId::new("ABCDEFGHJKMNPQ").unwrap()),
            fields: [("status".to_owned(), "unbound".to_owned())]
                .into_iter()
                .collect::<BTreeMap<_, _>>(),
        }],
        ..Default::default()
    };
    let actions = diff.classify();
    Proposal {
        diff,
        batch_label: None,
        author: Operator {
            id: OperatorId("github:tester".into()),
            display_name: "Tester".into(),
            source: IdentitySource::GitConfig,
            verified_at: None,
            claims: BTreeMap::new(),
            pubkey: None,
        },
        signatures: vec![],
        change_classification: actions,
        message: "table conformance".into(),
        request_id: RequestId(uuid::Uuid::from_u128(7)),
    }
}

#[test]
fn table_sink_passes_generic_conformance() {
    let sink = TableSink::new();
    proposal_sink_conformance(&sink, sample());
}
