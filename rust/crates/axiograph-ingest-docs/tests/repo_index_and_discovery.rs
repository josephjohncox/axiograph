use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn repo_index_extracts_edges_and_suggests_links() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let root = std::env::temp_dir().join(format!("axiograph_ingest_docs_test_{unique}"));
    std::fs::create_dir_all(&root).unwrap();

    let a_rs = root.join("a.rs");
    let b_rs = root.join("b.rs");

    std::fs::write(
        &a_rs,
        r#"
use std::fmt;

pub struct Foo {}

fn helper() {}
"#,
    )
    .unwrap();

    std::fs::write(
        &b_rs,
        r#"
use crate::Foo;

fn uses() {
    // Refer to Foo (defined elsewhere)
    let _x = Foo {};
}
"#,
    )
    .unwrap();

    let options = axiograph_ingest_docs::RepoIndexOptions {
        max_files: 10,
        max_file_bytes: 64 * 1024,
        lines_per_chunk: 50,
        ..Default::default()
    };

    let result = axiograph_ingest_docs::index_repo(&root, &options).unwrap();

    // Sanity: chunks exist.
    assert!(!result.extraction.chunks.is_empty());

    // Sanity: we extracted at least one definition edge for Foo.
    let has_foo_def = result.edges.iter().any(|e| match e {
        axiograph_ingest_docs::RepoEdgeV1::DefinesSymbol { file, symbol, .. } => {
            file == "a.rs" && symbol == "Foo"
        }
        _ => false,
    });
    assert!(has_foo_def, "expected DefinesSymbol edge for Foo in a.rs");

    // Suggest mentions links; expect b.rs -> Foo with defined_in = a.rs.
    let trace = axiograph_ingest_docs::suggest_mentions_symbol_trace_v1(
        &result.extraction.chunks,
        &result.edges,
        1000,
        "trace_test".to_string(),
        "0".to_string(),
    )
    .unwrap();

    let has_b_mentions_foo = trace.proposals.iter().any(|p| match p {
        axiograph_ingest_docs::DiscoveryProposalV1::Relation {
            from, to, metadata, ..
        } => {
            from == "b.rs"
                && to == "Foo"
                && metadata.get("defined_in").map(|s| s.as_str()) == Some("a.rs")
        }
        _ => false,
    });
    assert!(
        has_b_mentions_foo,
        "expected proposal b.rs MentionsSymbol Foo (defined_in=a.rs)"
    );

    // Repo edges → proposals should include File/Symbol entities + DefinesSymbol relation.
    let proposals = axiograph_ingest_docs::proposals_from_repo_edges_v1(
        &result.edges,
        Some("repo".to_string()),
    );

    let has_file_entity = proposals.iter().any(|p| match p {
        axiograph_ingest_docs::ProposalV1::Entity {
            entity_id,
            entity_type,
            ..
        } => entity_type == "File" && entity_id == "file::a_rs",
        _ => false,
    });
    assert!(has_file_entity, "expected File entity for a.rs");

    let has_defines_relation = proposals.iter().any(|p| match p {
        axiograph_ingest_docs::ProposalV1::Relation {
            rel_type,
            source,
            target,
            ..
        } => rel_type == "DefinesSymbol" && source == "file::a_rs" && target == "symbol::Foo",
        _ => false,
    });
    assert!(
        has_defines_relation,
        "expected DefinesSymbol relation file::a_rs -> symbol::Foo"
    );

    // Cleanup.
    std::fs::remove_dir_all(&root).ok();
}
