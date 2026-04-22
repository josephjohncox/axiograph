use anyhow::Result;
use std::collections::BTreeSet;

use axiograph_pathdb::axi_meta::{ATTR_AXI_FACT_ID, REL_AXI_FACT_IN_CONTEXT};
use axiograph_pathdb::certificate::{
    CertificatePayloadV2, CertificateV2, QueryAtomWitnessV3, QueryResultProofV3,
    ReachabilityProofV3,
};
use axiograph_pathdb::{AcceptedAxiAnchor, PathDB};
use serde::{Deserialize, Serialize};

use crate::trust_contract::TrustContractV1;

pub const EVIDENCE_SUPPORT_SUMMARY_VERSION_V1: &str = "evidence_support_summary_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportEntityRefV1 {
    pub entity_id: u32,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRefV1 {
    pub via: String,
    pub entity: SupportEntityRefV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FactSupportRefV1 {
    pub axi_fact_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact_entity_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<SupportEntityRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidenceRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unresolved_chunk_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceSupportSummaryV1 {
    pub version: String,
    pub accepted_axi_anchor: AcceptedAxiAnchor,
    pub trust: TrustContractV1,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_facts: Vec<FactSupportRefV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

pub fn evidence_support_summary_from_certificate(
    db: &PathDB,
    accepted_axi_anchor: AcceptedAxiAnchor,
    trust: TrustContractV1,
    cert: &CertificateV2,
) -> Option<EvidenceSupportSummaryV1> {
    let CertificatePayloadV2::QueryResultV3 { proof } = &cert.payload else {
        return None;
    };
    Some(EvidenceSupportSummaryV1 {
        version: EVIDENCE_SUPPORT_SUMMARY_VERSION_V1.to_string(),
        accepted_axi_anchor,
        trust,
        supported_facts: collect_fact_supports(db, proof),
        notes: vec![
            "support summaries are runtime artifacts derived from anchored query witnesses, context edges, and evidence chunk links".to_string(),
            "they are outside the trusted-kernel/certificate boundary and do not claim completeness or ontology closure".to_string(),
        ],
    })
}

pub fn execute_anchored_query_with_support_summary(
    prepared: &mut crate::query_ir::PreparedQueryV1,
    db: &PathDB,
    meta: Option<&axiograph_pathdb::axi_semantics::MetaPlaneIndex>,
    accepted_axi_anchor: AcceptedAxiAnchor,
) -> Result<(crate::axql::AxqlResult, Option<EvidenceSupportSummaryV1>)> {
    let certifiability = prepared.certifiability();
    let validated = prepared
        .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
        .execute_answer(db, meta)?;
    let result = validated.result().clone();

    if !certifiability.is_certifiable() {
        return Ok((result, None));
    }

    let certified = prepared
        .bind_accepted_axi_anchor(accepted_axi_anchor.clone())
        .certify_answer(validated, db, meta)?;
    let trust = crate::trust_contract::query_trust_contract_with_meta(
        prepared.as_query(),
        &certifiability,
        true,
        None,
        meta,
    );
    let support_summary = evidence_support_summary_from_certificate(
        db,
        accepted_axi_anchor,
        trust,
        certified.certificate(),
    );

    Ok((result, support_summary))
}

fn collect_fact_supports(db: &PathDB, proof: &QueryResultProofV3) -> Vec<FactSupportRefV1> {
    let mut axi_fact_ids = BTreeSet::new();
    for row in &proof.rows {
        for witness in &row.witnesses {
            if let QueryAtomWitnessV3::Path { proof } = witness {
                collect_axi_fact_ids(proof, &mut axi_fact_ids);
            }
        }
    }
    axi_fact_ids
        .into_iter()
        .map(|axi_fact_id| resolve_fact_support(db, axi_fact_id))
        .collect()
}

fn collect_axi_fact_ids(proof: &ReachabilityProofV3, out: &mut BTreeSet<String>) {
    match proof {
        ReachabilityProofV3::Reflexive { .. } => {}
        ReachabilityProofV3::Step {
            axi_fact_id, rest, ..
        } => {
            out.insert(axi_fact_id.clone());
            collect_axi_fact_ids(rest, out);
        }
    }
}

fn resolve_fact_support(db: &PathDB, axi_fact_id: String) -> FactSupportRefV1 {
    let Some(attr_id) = db.interner.id_of(ATTR_AXI_FACT_ID) else {
        return FactSupportRefV1 {
            axi_fact_id,
            fact_entity_id: None,
            relation_name: None,
            contexts: Vec::new(),
            evidence: Vec::new(),
            unresolved_chunk_ids: Vec::new(),
            notes: vec![format!(
                "`{ATTR_AXI_FACT_ID}` is not interned in this snapshot"
            )],
        };
    };
    let Some(value_id) = db.interner.id_of(&axi_fact_id) else {
        return FactSupportRefV1 {
            axi_fact_id,
            fact_entity_id: None,
            relation_name: None,
            contexts: Vec::new(),
            evidence: Vec::new(),
            unresolved_chunk_ids: Vec::new(),
            notes: vec!["fact id is not interned in this snapshot".to_string()],
        };
    };
    let matches_bitmap = db.entities.entities_with_attr_value(attr_id, value_id);
    let mut matches = matches_bitmap.iter();
    let Some(fact_entity_id) = matches.next() else {
        return FactSupportRefV1 {
            axi_fact_id,
            fact_entity_id: None,
            relation_name: None,
            contexts: Vec::new(),
            evidence: Vec::new(),
            unresolved_chunk_ids: Vec::new(),
            notes: vec!["no fact node in this snapshot carries the anchored fact id".to_string()],
        };
    };
    let fact_view = db.get_entity(fact_entity_id);
    let relation_name = fact_view
        .as_ref()
        .and_then(|view| view.attrs.get("axi_relation").cloned());
    let contexts = db
        .follow_one(fact_entity_id, REL_AXI_FACT_IN_CONTEXT)
        .iter()
        .filter_map(|id| support_entity_from_id(db, id))
        .collect::<Vec<_>>();

    let mut evidence = db
        .follow_one(fact_entity_id, "has_evidence_chunk")
        .iter()
        .filter_map(|id| support_entity_from_id(db, id))
        .map(|entity| EvidenceRefV1 {
            via: "has_evidence_chunk".to_string(),
            entity,
        })
        .collect::<Vec<_>>();

    let mut unresolved_chunk_ids = Vec::new();
    if let Some(view) = fact_view.as_ref() {
        let mut attr_chunk_ids = view
            .attrs
            .iter()
            .filter(|(key, _)| key.starts_with("evidence_") && key.ends_with("_chunk_id"))
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();
        attr_chunk_ids.sort();
        attr_chunk_ids.dedup();
        for chunk_id in attr_chunk_ids {
            if let Some(entity) = resolve_doc_chunk_by_chunk_id(db, &chunk_id) {
                if !evidence
                    .iter()
                    .any(|ev| ev.entity.entity_id == entity.entity_id)
                {
                    evidence.push(EvidenceRefV1 {
                        via: "evidence_chunk_id_attr".to_string(),
                        entity,
                    });
                }
            } else {
                unresolved_chunk_ids.push(chunk_id);
            }
        }
    }

    FactSupportRefV1 {
        axi_fact_id,
        fact_entity_id: Some(fact_entity_id),
        relation_name,
        contexts,
        evidence,
        unresolved_chunk_ids,
        notes: Vec::new(),
    }
}

fn support_entity_from_id(db: &PathDB, entity_id: u32) -> Option<SupportEntityRefV1> {
    let view = db.get_entity(entity_id)?;
    Some(SupportEntityRefV1 {
        entity_id,
        entity_type: view.entity_type,
        name: view.attrs.get("name").cloned(),
        chunk_id: view.attrs.get("chunk_id").cloned(),
    })
}

fn resolve_doc_chunk_by_chunk_id(db: &PathDB, chunk_id: &str) -> Option<SupportEntityRefV1> {
    let attr_id = db.interner.id_of("chunk_id")?;
    let value_id = db.interner.id_of(chunk_id)?;
    for entity_id in db
        .entities
        .entities_with_attr_value(attr_id, value_id)
        .iter()
    {
        let entity = support_entity_from_id(db, entity_id)?;
        if entity.entity_type == "DocChunk" {
            return Some(entity);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_pathdb::axi_semantics::MetaPlaneIndex;
    use axiograph_pathdb::certificate::CertificatePayloadV2;
    use axiograph_pathdb::{AcceptedSnapshotId, AxiDigest};

    fn sample_trust() -> TrustContractV1 {
        TrustContractV1 {
            trust_class: "certifiable".to_string(),
            soundness: "certificate_available_but_not_emitted".to_string(),
            coverage: "full_query".to_string(),
            scope: crate::trust_contract::TrustScopeV1 {
                anchor: "snapshot_scoped".to_string(),
                context: "single_context".to_string(),
            },
            reasons: Vec::new(),
            certifiable_disjuncts: None,
            execution_only_disjuncts: None,
            semantic_coverage: None,
            semantic_claims: Vec::new(),
            gaps: Vec::new(),
        }
    }

    #[test]
    fn evidence_support_summary_tracks_context_and_evidence_chunks() {
        let axi = std::fs::read_to_string(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../..")
                .join("examples/manufacturing/SupplyChainModalitiesHoTT.axi"),
        )
        .expect("read supply chain modal axi");
        let digest = AxiDigest::from_axi_text(&axi);
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:supply-chain-modal"),
            digest.clone(),
        );
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, &axi)
            .expect("import supply chain modal axi");
        db.build_indexes();

        let doc_chunk = db.add_entity(
            "DocChunk",
            vec![("name", "doc_policy_0"), ("chunk_id", "doc_policy_0")],
        );
        let Some(facts) = db.find_by_type("EvidenceSupports") else {
            panic!("expected EvidenceSupports fact nodes");
        };
        let fact_id = facts.iter().next().expect("one evidence support fact");
        let mut checked = axiograph_pathdb::CheckedDbMut::new(&mut db).expect("checked db");
        checked
            .add_edge_checked("has_evidence_chunk", fact_id, doc_chunk, 1.0, Vec::new())
            .expect("attach evidence chunk");

        let meta = MetaPlaneIndex::from_db(&db).expect("meta plane");
        let q: crate::query_ir::QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["?p"],
              "where": [
                {
                  "kind": "fact",
                  "fact": "?f",
                  "relation": "SupplyChainModal.EvidenceSupports",
                  "fields": {
                    "ev": "ERPEvent_0",
                    "prop": "?p",
                    "ctx": "Observed"
                  }
                }
              ],
              "limit": 10
            }"#,
        )
        .expect("parse query");
        let mut prepared = q
            .prepare_with_meta(&db, Some(&meta))
            .expect("prepare query");
        let validated = prepared
            .bind_accepted_axi_anchor(anchor.clone())
            .execute_answer(&db, Some(&meta))
            .expect("execute anchored answer");
        let certified = prepared
            .bind_accepted_axi_anchor(anchor.clone())
            .certify_answer(validated, &db, Some(&meta))
            .expect("certify anchored answer");

        let summary = evidence_support_summary_from_certificate(
            &db,
            anchor.clone(),
            sample_trust(),
            certified.certificate(),
        )
        .expect("support summary");

        assert_eq!(summary.accepted_axi_anchor, anchor);
        assert!(!summary.supported_facts.is_empty());
        assert!(summary.supported_facts.iter().any(|fact| {
            fact.contexts
                .iter()
                .any(|ctx| ctx.name.as_deref() == Some("Observed"))
                && fact
                    .evidence
                    .iter()
                    .any(|ev| ev.entity.entity_type == "DocChunk")
        }));

        match &certified.certificate().payload {
            CertificatePayloadV2::QueryResultV3 { proof } => {
                assert!(!proof.rows.is_empty());
            }
            other => panic!("expected query_result_v3 payload, got {other:?}"),
        }
    }

    #[test]
    fn evidence_support_summary_serializes_anchor_and_supported_facts() {
        let summary = EvidenceSupportSummaryV1 {
            version: EVIDENCE_SUPPORT_SUMMARY_VERSION_V1.to_string(),
            accepted_axi_anchor: AcceptedAxiAnchor::new(
                AcceptedSnapshotId::new("accepted:test"),
                AxiDigest::new("fnv1a64:test"),
            ),
            trust: sample_trust(),
            supported_facts: vec![FactSupportRefV1 {
                axi_fact_id: "factfnv1a64:test".to_string(),
                fact_entity_id: Some(42),
                relation_name: Some("Fam.Parent".to_string()),
                contexts: vec![SupportEntityRefV1 {
                    entity_id: 7,
                    entity_type: "Context".to_string(),
                    name: Some("Observed".to_string()),
                    chunk_id: None,
                }],
                evidence: vec![EvidenceRefV1 {
                    via: "has_evidence_chunk".to_string(),
                    entity: SupportEntityRefV1 {
                        entity_id: 9,
                        entity_type: "DocChunk".to_string(),
                        name: Some("doc_policy_0".to_string()),
                        chunk_id: Some("doc_policy_0".to_string()),
                    },
                }],
                unresolved_chunk_ids: Vec::new(),
                notes: Vec::new(),
            }],
            notes: vec!["runtime-only support summary".to_string()],
        };

        let value = serde_json::to_value(&summary).expect("serialize support summary");
        assert_eq!(
            value["accepted_axi_anchor"]["accepted_snapshot_id"],
            "accepted:test"
        );
        assert_eq!(value["accepted_axi_anchor"]["axi_digest"], "fnv1a64:test");
        assert_eq!(
            value["supported_facts"][0]["axi_fact_id"],
            "factfnv1a64:test"
        );
        assert_eq!(
            value["supported_facts"][0]["evidence"][0]["entity"]["entity_type"],
            "DocChunk"
        );
    }
}
