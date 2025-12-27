use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_ingest_docs::{
    promote_proposals_to_candidates_v1, EvidencePointer, PromoteOptionsV1, PromotionDomainV1,
    ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1, PROPOSALS_VERSION_V1,
};

fn meta(id: &str, confidence: f64, schema_hint: &str) -> ProposalMetaV1 {
    ProposalMetaV1 {
        proposal_id: id.to_string(),
        confidence,
        evidence: vec![EvidencePointer {
            chunk_id: "chunk_0".to_string(),
            locator: Some("doc.txt".to_string()),
            span_id: None,
        }],
        public_rationale: "test".to_string(),
        metadata: std::collections::HashMap::new(),
        schema_hint: Some(schema_hint.to_string()),
    }
}

#[test]
fn promotion_emits_candidate_modules_for_three_canonical_domains() {
    let proposals = vec![
        // -----------------------------------------------------------------
        // MachinistLearning: map Claim → Concept/Guideline/Tacit
        // -----------------------------------------------------------------
        ProposalV1::Entity {
            meta: meta("p_claim_def", 0.75, "machining"),
            entity_id: "claim_def".to_string(),
            entity_type: "Claim".to_string(),
            name: "Work hardening".to_string(),
            attributes: {
                let mut m = std::collections::HashMap::new();
                m.insert("fact_type".to_string(), "Definition".to_string());
                m.insert(
                    "statement".to_string(),
                    "Work hardening is when the surface becomes harder after deformation."
                        .to_string(),
                );
                m
            },
            description: None,
        },
        ProposalV1::Entity {
            meta: meta("p_claim_constraint", 0.95, "machining"),
            entity_id: "claim_constraint".to_string(),
            entity_type: "Claim".to_string(),
            name: "Titanium speed limit".to_string(),
            attributes: {
                let mut m = std::collections::HashMap::new();
                m.insert("fact_type".to_string(), "Constraint".to_string());
                m.insert(
                    "statement".to_string(),
                    "Never exceed 60 m/min cutting speed for titanium.".to_string(),
                );
                m
            },
            description: None,
        },
        ProposalV1::Entity {
            meta: meta("p_claim_heur", 0.85, "machining"),
            entity_id: "claim_heur".to_string(),
            entity_type: "Claim".to_string(),
            name: "Titanium prefers low speed".to_string(),
            attributes: {
                let mut m = std::collections::HashMap::new();
                m.insert("fact_type".to_string(), "Heuristic".to_string());
                m.insert(
                    "statement".to_string(),
                    "Titanium requires slow speeds and high coolant.".to_string(),
                );
                m
            },
            description: None,
        },
        // -----------------------------------------------------------------
        // EconomicFlows: FlowType + FlowInverse
        // -----------------------------------------------------------------
        ProposalV1::Entity {
            meta: meta("p_flow_loans", 0.9, "economics"),
            entity_id: "flow_loans".to_string(),
            entity_type: "FlowType".to_string(),
            name: "Loans".to_string(),
            attributes: std::collections::HashMap::new(),
            description: None,
        },
        ProposalV1::Entity {
            meta: meta("p_flow_repay", 0.9, "economics"),
            entity_id: "flow_repay".to_string(),
            entity_type: "FlowType".to_string(),
            name: "LoanRepayment".to_string(),
            attributes: std::collections::HashMap::new(),
            description: None,
        },
        ProposalV1::Relation {
            meta: meta("p_flow_inverse", 0.9, "economics"),
            relation_id: "rel_flow_inverse".to_string(),
            rel_type: "FlowInverse".to_string(),
            source: "flow_loans".to_string(),
            target: "flow_repay".to_string(),
            attributes: std::collections::HashMap::new(),
        },
        // -----------------------------------------------------------------
        // SchemaEvolution: MigrationTo/MigrationFrom
        // -----------------------------------------------------------------
        ProposalV1::Entity {
            meta: meta("p_schema_v5", 0.9, "schema_evolution"),
            entity_id: "schema_v5".to_string(),
            entity_type: "Schema_".to_string(),
            name: "ProductV5".to_string(),
            attributes: std::collections::HashMap::new(),
            description: None,
        },
        ProposalV1::Entity {
            meta: meta("p_mig_add", 0.9, "schema_evolution"),
            entity_id: "mig_add".to_string(),
            entity_type: "Migration".to_string(),
            name: "AddDiscounts".to_string(),
            attributes: std::collections::HashMap::new(),
            description: None,
        },
        ProposalV1::Entity {
            meta: meta("p_schema_v4", 0.9, "schema_evolution"),
            entity_id: "schema_v4".to_string(),
            entity_type: "Schema_".to_string(),
            name: "ProductV4".to_string(),
            attributes: std::collections::HashMap::new(),
            description: None,
        },
        ProposalV1::Relation {
            meta: meta("p_mig_from", 0.9, "schema_evolution"),
            relation_id: "rel_mig_from".to_string(),
            rel_type: "MigrationFrom".to_string(),
            source: "mig_add".to_string(),
            target: "schema_v4".to_string(),
            attributes: std::collections::HashMap::new(),
        },
        ProposalV1::Relation {
            meta: meta("p_mig_to", 0.9, "schema_evolution"),
            relation_id: "rel_mig_to".to_string(),
            rel_type: "MigrationTo".to_string(),
            source: "mig_add".to_string(),
            target: "schema_v5".to_string(),
            attributes: std::collections::HashMap::new(),
        },
    ];

    let file = ProposalsFileV1 {
        version: PROPOSALS_VERSION_V1,
        generated_at: "test".to_string(),
        source: ProposalSourceV1 {
            source_type: "doc".to_string(),
            locator: "doc.txt".to_string(),
        },
        schema_hint: None,
        proposals,
    };

    let result = promote_proposals_to_candidates_v1(&file, &PromoteOptionsV1::default())
        .expect("promote proposals");

    let m = result
        .candidates
        .get(&PromotionDomainV1::MachinistLearning)
        .expect("MachinistLearning candidate module");
    let e = result
        .candidates
        .get(&PromotionDomainV1::EconomicFlows)
        .expect("EconomicFlows candidate module");
    let s = result
        .candidates
        .get(&PromotionDomainV1::SchemaEvolution)
        .expect("SchemaEvolution candidate module");

    assert!(
        m.contains("instance "),
        "expected schema_v1 instance output"
    );
    assert!(
        m.contains("TacitKnowledge") || m.contains("tacitRule"),
        "expected tacit knowledge content in MachinistLearning output"
    );
    assert!(
        e.contains("instance "),
        "expected schema_v1 instance output"
    );
    assert!(
        s.contains("instance "),
        "expected schema_v1 instance output"
    );

    // Ensure each output parses via the unified `.axi` entrypoint.
    let _ = parse_axi_v1(m).expect("parse MachinistLearning candidate");
    let _ = parse_axi_v1(e).expect("parse EconomicFlows candidate");
    let _ = parse_axi_v1(s).expect("parse SchemaEvolution candidate");
}
