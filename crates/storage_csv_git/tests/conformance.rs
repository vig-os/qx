//! ADR-027 §Tier 1 conformance test entry point. Today the body is a
//! placeholder — `port_tests::repository_conformance` is itself a
//! scaffold. Once both are fleshed out (foundation phase), uncomment
//! the call and supply a tempdir-backed `CsvGitRepository`.

#[test]
fn csv_git_conforms() {
    // let repo = part_registry_storage_csv_git::CsvGitRepository::new(
    //     std::env::temp_dir().join("part-registry-conformance"),
    // );
    // part_registry_port_tests::repository_conformance(repo);
}
