use std::collections::HashMap;

#[test]
fn proposals_from_extracted_facts_emits_claim_and_mentions() {
    let mut extracted_entities = HashMap::new();
    extracted_entities.insert("material".to_string(), "titanium".to_string());
    extracted_entities.insert("tool_material".to_string(), "carbide".to_string());

    let fact = axiograph_ingest_docs::ExtractedFact {
        fact_id: "f1".to_string(),
        domain: "machining".to_string(),
        statement: "for titanium use carbide".to_string(),
        fact_type: axiograph_ingest_docs::FactType::Recommendation,
        confidence: 0.8,
        source_chunk_id: "chunk_1".to_string(),
        evidence_span: "for titanium use carbide".to_string(),
        extracted_entities,
    };

    let proposals = axiograph_ingest_docs::proposals_from_extracted_facts_v1(
        &[fact],
        Some("doc.txt".to_string()),
        Some("machining".to_string()),
    );

    let has_claim = proposals.iter().any(|p| match p {
        axiograph_ingest_docs::ProposalV1::Entity {
            entity_id,
            entity_type,
            ..
        } => entity_type == "Claim" && entity_id == "claim::f1",
        _ => false,
    });
    assert!(has_claim, "expected Claim entity claim::f1");

    let has_mentions_relation = proposals.iter().any(|p| match p {
        axiograph_ingest_docs::ProposalV1::Relation {
            rel_type, source, ..
        } => rel_type == "Mentions" && source == "claim::f1",
        _ => false,
    });
    assert!(
        has_mentions_relation,
        "expected Mentions relation from claim::f1"
    );
}
