//! Runtime-layer support summaries for anchored query answers.
//!
//! The support basis in this tranche is proof-native: `query_result_v3` witness
//! rows, specifically `QueryAtomWitnessV3::Path` witnesses and their
//! `axi_fact_id` path steps. Context and evidence links remain best-effort
//! attachment-layer enrichments resolved from PathDB after support extraction.
//! This module strengthens the `support_summary` contract without changing the
//! trusted kernel or certificate family.

use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet};

use axiograph_pathdb::axi_meta::{ATTR_AXI_FACT_ID, REL_AXI_FACT_IN_CONTEXT};
use axiograph_pathdb::certificate::{
    CertificatePayloadV2, CertificateV2, QueryAtomWitnessV3, QueryResultProofV3,
    ReachabilityProofV3,
};
use axiograph_pathdb::{AcceptedAxiAnchor, PathDB};
use serde::{Deserialize, Serialize};

use crate::trust_contract::TrustContractV1;

pub const SUPPORT_SUMMARY_VERSION_V2: &str = "support_summary_v2";

const SUPPORT_CERTIFICATE_KIND_QUERY_RESULT_V3: &str = "query_result_v3";
const SUPPORT_SOURCE_INTERNAL_RUNTIME_CERTIFICATE: &str = "internal_runtime_certificate";
const SUPPORT_SOURCE_RESPONSE_CERTIFICATE: &str = "response_certificate";
const SUPPORT_KIND_QUERY_RESULT_V3_PATH_STEP: &str = "query_result_v3_path_step";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportBasisV1 {
    pub certificate_kind: String,
    pub source: String,
    pub certificate_emitted_to_client: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportCoverageV1 {
    pub rows_total: usize,
    pub rows_with_support: usize,
    pub witnesses_total: usize,
    pub path_witnesses_supported: usize,
    pub unsupported_witness_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SupportRowRefV1 {
    pub row_index: usize,
    pub disjunct: u32,
}

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
    pub support_kind: String,
    pub witness_rows: Vec<SupportRowRefV1>,
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
    pub basis: SupportBasisV1,
    pub coverage: SupportCoverageV1,
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
    certificate_emitted_to_client: bool,
    certificate_verified: Option<bool>,
) -> Option<EvidenceSupportSummaryV1> {
    let CertificatePayloadV2::QueryResultV3 { proof } = &cert.payload else {
        return None;
    };
    let (supported_facts, coverage) = collect_fact_supports(db, proof);
    Some(EvidenceSupportSummaryV1 {
        version: SUPPORT_SUMMARY_VERSION_V2.to_string(),
        accepted_axi_anchor,
        trust,
        basis: SupportBasisV1 {
            certificate_kind: SUPPORT_CERTIFICATE_KIND_QUERY_RESULT_V3.to_string(),
            source: if certificate_emitted_to_client {
                SUPPORT_SOURCE_RESPONSE_CERTIFICATE.to_string()
            } else {
                SUPPORT_SOURCE_INTERNAL_RUNTIME_CERTIFICATE.to_string()
            },
            certificate_emitted_to_client,
            certificate_verified,
        },
        coverage,
        supported_facts,
        notes: vec![
            "support summaries are runtime/report-layer artifacts grounded in query_result_v3 path witnesses; contexts and evidence links are attachment-layer enrichments resolved from PathDB".to_string(),
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
        false,
        None,
        meta,
    );
    let support_summary = evidence_support_summary_from_certificate(
        db,
        accepted_axi_anchor,
        trust,
        certified.certificate(),
        false,
        None,
    );

    Ok((result, support_summary))
}

fn collect_fact_supports(
    db: &PathDB,
    proof: &QueryResultProofV3,
) -> (Vec<FactSupportRefV1>, SupportCoverageV1) {
    let mut fact_rows: BTreeMap<String, BTreeSet<SupportRowRefV1>> = BTreeMap::new();
    let mut rows_with_support = 0usize;
    let mut witnesses_total = 0usize;
    let mut path_witnesses_supported = 0usize;
    let mut unsupported_witness_kinds = BTreeSet::new();

    for (row_index, row) in proof.rows.iter().enumerate() {
        let mut row_has_supported_witness = false;
        witnesses_total += row.witnesses.len();

        for witness in &row.witnesses {
            match witness {
                QueryAtomWitnessV3::Path { proof } => {
                    path_witnesses_supported += 1;
                    row_has_supported_witness = true;
                    let mut axi_fact_ids = BTreeSet::new();
                    collect_axi_fact_ids(proof, &mut axi_fact_ids);
                    let row_ref = SupportRowRefV1 {
                        row_index,
                        disjunct: row.disjunct,
                    };
                    for axi_fact_id in axi_fact_ids {
                        fact_rows
                            .entry(axi_fact_id)
                            .or_default()
                            .insert(row_ref.clone());
                    }
                }
                QueryAtomWitnessV3::Type { .. } => {
                    unsupported_witness_kinds.insert("type".to_string());
                }
                QueryAtomWitnessV3::AttrEq { .. } => {
                    unsupported_witness_kinds.insert("attr_eq".to_string());
                }
            }
        }

        if row_has_supported_witness {
            rows_with_support += 1;
        }
    }

    let supported_facts = fact_rows
        .into_iter()
        .map(|(axi_fact_id, witness_rows)| {
            resolve_fact_support(db, axi_fact_id, witness_rows.into_iter().collect())
        })
        .collect();

    (
        supported_facts,
        SupportCoverageV1 {
            rows_total: proof.rows.len(),
            rows_with_support,
            witnesses_total,
            path_witnesses_supported,
            unsupported_witness_kinds: unsupported_witness_kinds.into_iter().collect(),
        },
    )
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

fn resolve_fact_support(
    db: &PathDB,
    axi_fact_id: String,
    witness_rows: Vec<SupportRowRefV1>,
) -> FactSupportRefV1 {
    let Some(attr_id) = db.interner.id_of(ATTR_AXI_FACT_ID) else {
        return FactSupportRefV1 {
            axi_fact_id,
            fact_entity_id: None,
            relation_name: None,
            support_kind: SUPPORT_KIND_QUERY_RESULT_V3_PATH_STEP.to_string(),
            witness_rows,
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
            support_kind: SUPPORT_KIND_QUERY_RESULT_V3_PATH_STEP.to_string(),
            witness_rows,
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
            support_kind: SUPPORT_KIND_QUERY_RESULT_V3_PATH_STEP.to_string(),
            witness_rows,
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
        support_kind: SUPPORT_KIND_QUERY_RESULT_V3_PATH_STEP.to_string(),
        witness_rows,
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
            false,
            None,
        )
        .expect("support summary");

        assert_eq!(summary.accepted_axi_anchor, anchor);
        let value = serde_json::to_value(&summary).expect("serialize support summary");
        assert_eq!(value["basis"]["certificate_kind"], "query_result_v3");
        assert_eq!(value["basis"]["certificate_emitted_to_client"], false);
        assert_eq!(value["coverage"]["rows_total"], 1);
        assert_eq!(value["coverage"]["rows_with_support"], 1);
        assert!(value["coverage"]["path_witnesses_supported"]
            .as_u64()
            .is_some_and(|count| count >= 1));
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
        assert!(value["supported_facts"]
            .as_array()
            .is_some_and(|facts| facts.iter().any(|fact| {
                fact["support_kind"].as_str() == Some("query_result_v3_path_step")
                    && fact["witness_rows"].as_array().is_some_and(|rows| {
                        rows.iter().any(|row| {
                            row["row_index"].as_u64() == Some(0)
                                && row["disjunct"].as_u64() == Some(0)
                        })
                    })
            })));

        match &certified.certificate().payload {
            CertificatePayloadV2::QueryResultV3 { proof } => {
                assert!(!proof.rows.is_empty());
            }
            other => panic!("expected query_result_v3 payload, got {other:?}"),
        }
    }

    #[test]
    fn evidence_support_summary_reports_unsupported_non_path_witness_kinds() {
        let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {
    (child=Alice, parent=Bob)
  }
"#;
        let digest = AxiDigest::from_axi_text(axi);
        let anchor = AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new("accepted:unsupported-witness-test"),
            digest,
        );
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, axi)
            .expect("import demo axi");
        db.build_indexes();

        let meta = MetaPlaneIndex::from_db(&db).expect("meta plane");
        let q: crate::query_ir::QueryIrV1 = serde_json::from_str(
            r#"{
              "version": 1,
              "select": ["?p"],
              "where": [
                { "kind": "type", "term": "?p", "type": "Person" },
                { "kind": "edge", "left": "Alice", "path": "S.Parent", "right": "?p" }
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
            anchor,
            sample_trust(),
            certified.certificate(),
            false,
            None,
        )
        .expect("support summary");
        let value = serde_json::to_value(&summary).expect("serialize support summary");

        assert!(value["coverage"]["witnesses_total"]
            .as_u64()
            .is_some_and(|count| count >= 2));
        assert!(value["coverage"]["unsupported_witness_kinds"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("type"))));
    }

    #[test]
    fn evidence_support_summary_serializes_anchor_and_supported_facts() {
        let summary = EvidenceSupportSummaryV1 {
            version: SUPPORT_SUMMARY_VERSION_V2.to_string(),
            accepted_axi_anchor: AcceptedAxiAnchor::new(
                AcceptedSnapshotId::new("accepted:test"),
                AxiDigest::new("fnv1a64:test"),
            ),
            trust: sample_trust(),
            basis: SupportBasisV1 {
                certificate_kind: SUPPORT_CERTIFICATE_KIND_QUERY_RESULT_V3.to_string(),
                source: SUPPORT_SOURCE_INTERNAL_RUNTIME_CERTIFICATE.to_string(),
                certificate_emitted_to_client: false,
                certificate_verified: None,
            },
            coverage: SupportCoverageV1 {
                rows_total: 1,
                rows_with_support: 1,
                witnesses_total: 1,
                path_witnesses_supported: 1,
                unsupported_witness_kinds: Vec::new(),
            },
            supported_facts: vec![FactSupportRefV1 {
                axi_fact_id: "factfnv1a64:test".to_string(),
                fact_entity_id: Some(42),
                relation_name: Some("Fam.Parent".to_string()),
                support_kind: SUPPORT_KIND_QUERY_RESULT_V3_PATH_STEP.to_string(),
                witness_rows: vec![SupportRowRefV1 {
                    row_index: 0,
                    disjunct: 0,
                }],
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
        assert_eq!(value["basis"]["certificate_kind"], "query_result_v3");
        assert_eq!(value["coverage"]["rows_total"], 1);
        assert_eq!(
            value["supported_facts"][0]["support_kind"],
            "query_result_v3_path_step"
        );
        assert_eq!(
            value["supported_facts"][0]["witness_rows"][0]["row_index"],
            0
        );
        assert_eq!(
            value["supported_facts"][0]["evidence"][0]["entity"]["entity_type"],
            "DocChunk"
        );
    }
}
