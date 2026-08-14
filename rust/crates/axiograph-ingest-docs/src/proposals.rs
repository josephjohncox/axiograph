//! Generic proposals.json output (Evidence/Proposals schema).
//!
//! The purpose of `proposals.json` is to take a RAG-shaped ingestion pipeline
//! (parse → chunk → retrieve) and extend it into a *structured KG-shaped* pipeline:
//!
//! - extract entities/relations/claims as **proposals**,
//! - attach confidence + provenance + evidence pointers,
//! - and later reconcile/promote proposals into canonical `.axi` modules.
//!
//! This file format is deliberately domain-agnostic. Domain-specific ingestion can add
//! `schema_hint` and/or extra metadata, but consumers should be able to understand the core
//! proposal shapes without knowing the domain schema.

use crate::{EvidencePointer, ExtractedFact, FactType, RepoEdgeV1};
use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PROPOSALS_VERSION_V1: u32 = 1;
pub const MAX_PROPOSALS_V1: usize = 100_000;
const MAX_PROPOSAL_NESTED_ITEMS: usize = 1_000_000;
const MAX_PROPOSAL_EVIDENCE: usize = 1_024;
const MAX_PROPOSAL_MAP_ENTRIES: usize = 4_096;

/// Top-level proposals file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalsFileV1 {
    pub version: u32,
    /// ISO-8601 timestamp (recommended) or unix seconds as a string.
    pub generated_at: String,
    pub source: ProposalSourceV1,
    /// Optional hint for downstream reconciliation (“machining”, “schema_v1”, etc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hint: Option<String>,
    pub proposals: Vec<ProposalV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProposalSourceV1 {
    /// e.g. `doc`, `confluence`, `conversation`, `repo`, `ingest_dir`
    pub source_type: String,
    /// path/url/root identifier for the run
    pub locator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalMetaV1 {
    pub proposal_id: String,
    pub confidence: f64,
    pub evidence: Vec<EvidencePointer>,
    /// Short public rationale, not raw model hidden reasoning.
    pub public_rationale: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Optional hint for downstream reconciliation (“EconomicFlows”, “MachinistLearning”, etc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ProposalV1 {
    Entity {
        #[serde(flatten)]
        meta: ProposalMetaV1,
        entity_id: String,
        entity_type: String,
        name: String,
        #[serde(default)]
        attributes: HashMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    Relation {
        #[serde(flatten)]
        meta: ProposalMetaV1,
        relation_id: String,
        rel_type: String,
        source: String,
        target: String,
        #[serde(default)]
        attributes: HashMap<String, String>,
    },
}

pub fn validate_proposals_file_v1(file: &ProposalsFileV1) -> Result<()> {
    if file.version != PROPOSALS_VERSION_V1 {
        return Err(anyhow!(
            "unsupported proposals version {} (expected {PROPOSALS_VERSION_V1})",
            file.version
        ));
    }
    if file.proposals.len() > MAX_PROPOSALS_V1 {
        return Err(anyhow!(
            "proposal count {} exceeds {MAX_PROPOSALS_V1}",
            file.proposals.len()
        ));
    }
    if file.generated_at.len() > 1024
        || file.source.source_type.len() > 1024
        || file.source.locator.len() > 1024 * 1024
    {
        return Err(anyhow!("proposal source metadata exceeds byte limits"));
    }
    if file.source.source_type.trim().is_empty() || file.source.locator.trim().is_empty() {
        return Err(anyhow!(
            "proposal source type and locator must be non-empty"
        ));
    }

    let mut nested_items = 0_usize;
    let mut proposal_ids = HashSet::with_capacity(file.proposals.len());
    let mut entity_ids = HashSet::new();
    let mut relation_ids = HashSet::new();
    for proposal in &file.proposals {
        let (meta, attributes) = match proposal {
            ProposalV1::Entity {
                meta,
                entity_id,
                entity_type,
                name,
                attributes,
                ..
            } => {
                if entity_id.trim().is_empty()
                    || entity_type.trim().is_empty()
                    || name.trim().is_empty()
                {
                    return Err(anyhow!(
                        "entity proposal id, type, and name must be non-empty"
                    ));
                }
                if !entity_ids.insert(entity_id.as_str()) {
                    return Err(anyhow!("duplicate entity id `{entity_id}`"));
                }
                (meta, attributes)
            }
            ProposalV1::Relation {
                meta,
                relation_id,
                rel_type,
                source,
                target,
                attributes,
            } => {
                if relation_id.trim().is_empty()
                    || rel_type.trim().is_empty()
                    || source.trim().is_empty()
                    || target.trim().is_empty()
                {
                    return Err(anyhow!(
                        "relation proposal id, type, source, and target must be non-empty"
                    ));
                }
                if !relation_ids.insert(relation_id.as_str()) {
                    return Err(anyhow!("duplicate relation id `{relation_id}`"));
                }
                (meta, attributes)
            }
        };
        if meta.proposal_id.trim().is_empty()
            || !meta.confidence.is_finite()
            || !(0.0..=1.0).contains(&meta.confidence)
        {
            return Err(anyhow!(
                "proposal id must be non-empty and confidence finite in [0, 1]"
            ));
        }
        if !proposal_ids.insert(meta.proposal_id.as_str()) {
            return Err(anyhow!("duplicate proposal id `{}`", meta.proposal_id));
        }
        if meta
            .evidence
            .iter()
            .any(|evidence| evidence.chunk_id.trim().is_empty())
        {
            return Err(anyhow!("proposal evidence chunk id must be non-empty"));
        }
        if meta.evidence.len() > MAX_PROPOSAL_EVIDENCE
            || meta.metadata.len() > MAX_PROPOSAL_MAP_ENTRIES
            || attributes.len() > MAX_PROPOSAL_MAP_ENTRIES
        {
            return Err(anyhow!("proposal nested collection exceeds hard limit"));
        }
        nested_items = nested_items
            .checked_add(meta.evidence.len())
            .and_then(|count| count.checked_add(meta.metadata.len()))
            .and_then(|count| count.checked_add(attributes.len()))
            .ok_or_else(|| anyhow!("proposal nested item count overflow"))?;
    }
    if nested_items > MAX_PROPOSAL_NESTED_ITEMS {
        return Err(anyhow!(
            "proposal nested item count {nested_items} exceeds {MAX_PROPOSAL_NESTED_ITEMS}"
        ));
    }
    Ok(())
}

/// Convert document-extracted facts into generic proposals.
///
/// This creates:
///
/// - one `Claim` entity per extracted fact
/// - one `Mention` entity per extracted fact field (e.g. material/tool/speed)
/// - `Mentions` relations from claim → mention
pub fn proposals_from_extracted_facts_v1(
    facts: &[ExtractedFact],
    evidence_locator: Option<String>,
    schema_hint: Option<String>,
) -> Vec<ProposalV1> {
    let mut out = Vec::new();

    for fact in facts {
        let claim_id = format!("claim::{}", sanitize_id(&fact.fact_id));

        let mut claim_attrs = HashMap::new();
        claim_attrs.insert("statement".to_string(), fact.statement.clone());
        claim_attrs.insert("domain".to_string(), fact.domain.clone());
        claim_attrs.insert(
            "fact_type".to_string(),
            fact_type_to_string(&fact.fact_type),
        );
        claim_attrs.insert("source_chunk_id".to_string(), fact.source_chunk_id.clone());
        claim_attrs.insert("evidence_span".to_string(), fact.evidence_span.clone());

        for (k, v) in &fact.extracted_entities {
            claim_attrs.insert(format!("field_{}", sanitize_key(k)), v.clone());
        }

        let evidence = vec![EvidencePointer {
            chunk_id: fact.source_chunk_id.clone(),
            locator: evidence_locator.clone(),
            span_id: None,
        }];

        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), fact.domain.clone());
        metadata.insert(
            "fact_type".to_string(),
            fact_type_to_string(&fact.fact_type),
        );

        out.push(ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: claim_id.clone(),
                confidence: fact.confidence,
                evidence: evidence.clone(),
                public_rationale: fact.evidence_span.clone(),
                metadata,
                schema_hint: schema_hint.clone(),
            },
            entity_id: claim_id.clone(),
            entity_type: "Claim".to_string(),
            name: truncate_for_name(&fact.statement, 80),
            attributes: claim_attrs,
            description: None,
        });

        for (k, v) in &fact.extracted_entities {
            let mention_id = format!(
                "mention::{}::{}",
                sanitize_id(&fact.fact_id),
                sanitize_id(k)
            );

            let mut mention_attrs = HashMap::new();
            mention_attrs.insert("role".to_string(), k.clone());
            mention_attrs.insert("value".to_string(), v.clone());
            mention_attrs.insert("domain".to_string(), fact.domain.clone());

            out.push(ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: mention_id.clone(),
                    confidence: fact.confidence * 0.95,
                    evidence: evidence.clone(),
                    public_rationale: format!("Extracted field `{k}` = `{v}`."),
                    metadata: HashMap::new(),
                    schema_hint: schema_hint.clone(),
                },
                entity_id: mention_id.clone(),
                entity_type: "Mention".to_string(),
                name: v.clone(),
                attributes: mention_attrs,
                description: None,
            });

            let rel_id = format!(
                "rel::mentions::{}::{}",
                sanitize_id(&claim_id),
                sanitize_id(&mention_id)
            );
            let mut rel_attrs = HashMap::new();
            rel_attrs.insert("role".to_string(), k.clone());

            out.push(ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: rel_id.clone(),
                    confidence: fact.confidence * 0.95,
                    evidence: evidence.clone(),
                    public_rationale: format!("Claim mentions `{v}`."),
                    metadata: HashMap::new(),
                    schema_hint: schema_hint.clone(),
                },
                relation_id: rel_id,
                rel_type: "Mentions".to_string(),
                source: claim_id.clone(),
                target: mention_id,
                attributes: rel_attrs,
            });
        }
    }

    out
}

/// Convert repo edges into generic proposals (File/Symbol/Module graph).
pub fn proposals_from_repo_edges_v1(
    edges: &[RepoEdgeV1],
    schema_hint: Option<String>,
) -> Vec<ProposalV1> {
    // Deduplicate file/symbol/module entities by id.
    let mut entities: BTreeMap<String, ProposalV1> = BTreeMap::new();
    let mut non_deduped: Vec<ProposalV1> = Vec::new();

    for edge in edges {
        match edge {
            RepoEdgeV1::DefinesSymbol {
                file,
                symbol,
                symbol_kind,
                language,
                confidence,
                source_chunk_id,
                evidence_span,
            } => {
                let file_id = format!("file::{}", sanitize_id(file));
                let symbol_id = format!("symbol::{}", sanitize_id(symbol));

                entities
                    .entry(file_id.clone())
                    .or_insert_with(|| ProposalV1::Entity {
                        meta: ProposalMetaV1 {
                            proposal_id: file_id.clone(),
                            confidence: 0.95,
                            evidence: Vec::new(),
                            public_rationale: "File observed during repo indexing.".to_string(),
                            metadata: HashMap::new(),
                            schema_hint: schema_hint.clone(),
                        },
                        entity_id: file_id.clone(),
                        entity_type: "File".to_string(),
                        name: file.clone(),
                        attributes: {
                            let mut m = HashMap::new();
                            m.insert("language".to_string(), language.clone());
                            m
                        },
                        description: None,
                    });

                entities
                    .entry(symbol_id.clone())
                    .or_insert_with(|| ProposalV1::Entity {
                        meta: ProposalMetaV1 {
                            proposal_id: symbol_id.clone(),
                            confidence: 0.95,
                            evidence: Vec::new(),
                            public_rationale: "Symbol observed during repo indexing.".to_string(),
                            metadata: HashMap::new(),
                            schema_hint: schema_hint.clone(),
                        },
                        entity_id: symbol_id.clone(),
                        entity_type: "Symbol".to_string(),
                        name: symbol.clone(),
                        attributes: {
                            let mut m = HashMap::new();
                            m.insert("kind".to_string(), symbol_kind.clone());
                            m
                        },
                        description: None,
                    });

                let evidence = vec![EvidencePointer {
                    chunk_id: source_chunk_id.clone(),
                    locator: Some(file.clone()),
                    span_id: None,
                }];

                let rel_id = format!(
                    "rel::defines::{}::{}",
                    sanitize_id(&file_id),
                    sanitize_id(&symbol_id)
                );
                let mut rel_attrs = HashMap::new();
                rel_attrs.insert("symbol_kind".to_string(), symbol_kind.clone());
                rel_attrs.insert("language".to_string(), language.clone());

                non_deduped.push(ProposalV1::Relation {
                    meta: ProposalMetaV1 {
                        proposal_id: rel_id.clone(),
                        confidence: *confidence,
                        evidence,
                        public_rationale: evidence_span.clone(),
                        metadata: HashMap::new(),
                        schema_hint: schema_hint.clone(),
                    },
                    relation_id: rel_id,
                    rel_type: "DefinesSymbol".to_string(),
                    source: file_id,
                    target: symbol_id,
                    attributes: rel_attrs,
                });
            }
            RepoEdgeV1::ImportsModule {
                file,
                module_path,
                language,
                confidence,
                source_chunk_id,
                evidence_span,
            } => {
                let file_id = format!("file::{}", sanitize_id(file));
                let module_id = format!("module::{}", sanitize_id(module_path));

                entities
                    .entry(file_id.clone())
                    .or_insert_with(|| ProposalV1::Entity {
                        meta: ProposalMetaV1 {
                            proposal_id: file_id.clone(),
                            confidence: 0.95,
                            evidence: Vec::new(),
                            public_rationale: "File observed during repo indexing.".to_string(),
                            metadata: HashMap::new(),
                            schema_hint: schema_hint.clone(),
                        },
                        entity_id: file_id.clone(),
                        entity_type: "File".to_string(),
                        name: file.clone(),
                        attributes: {
                            let mut m = HashMap::new();
                            m.insert("language".to_string(), language.clone());
                            m
                        },
                        description: None,
                    });

                entities
                    .entry(module_id.clone())
                    .or_insert_with(|| ProposalV1::Entity {
                        meta: ProposalMetaV1 {
                            proposal_id: module_id.clone(),
                            confidence: 0.95,
                            evidence: Vec::new(),
                            public_rationale: "Module observed during repo indexing.".to_string(),
                            metadata: HashMap::new(),
                            schema_hint: schema_hint.clone(),
                        },
                        entity_id: module_id.clone(),
                        entity_type: "Module".to_string(),
                        name: module_path.clone(),
                        attributes: HashMap::new(),
                        description: None,
                    });

                let evidence = vec![EvidencePointer {
                    chunk_id: source_chunk_id.clone(),
                    locator: Some(file.clone()),
                    span_id: None,
                }];

                let rel_id = format!(
                    "rel::imports::{}::{}",
                    sanitize_id(&file_id),
                    sanitize_id(&module_id)
                );
                let mut rel_attrs = HashMap::new();
                rel_attrs.insert("language".to_string(), language.clone());

                non_deduped.push(ProposalV1::Relation {
                    meta: ProposalMetaV1 {
                        proposal_id: rel_id.clone(),
                        confidence: *confidence,
                        evidence,
                        public_rationale: evidence_span.clone(),
                        metadata: HashMap::new(),
                        schema_hint: schema_hint.clone(),
                    },
                    relation_id: rel_id,
                    rel_type: "ImportsModule".to_string(),
                    source: file_id,
                    target: module_id,
                    attributes: rel_attrs,
                });
            }
            RepoEdgeV1::Todo {
                file,
                language,
                confidence,
                source_chunk_id,
                evidence_span,
            } => {
                let file_id = format!("file::{}", sanitize_id(file));
                let todo_id = format!(
                    "todo::{}::{}",
                    sanitize_id(file),
                    sanitize_id(source_chunk_id)
                );

                entities
                    .entry(file_id.clone())
                    .or_insert_with(|| ProposalV1::Entity {
                        meta: ProposalMetaV1 {
                            proposal_id: file_id.clone(),
                            confidence: 0.95,
                            evidence: Vec::new(),
                            public_rationale: "File observed during repo indexing.".to_string(),
                            metadata: HashMap::new(),
                            schema_hint: schema_hint.clone(),
                        },
                        entity_id: file_id.clone(),
                        entity_type: "File".to_string(),
                        name: file.clone(),
                        attributes: {
                            let mut m = HashMap::new();
                            m.insert("language".to_string(), language.clone());
                            m
                        },
                        description: None,
                    });

                let evidence = vec![EvidencePointer {
                    chunk_id: source_chunk_id.clone(),
                    locator: Some(file.clone()),
                    span_id: None,
                }];

                non_deduped.push(ProposalV1::Entity {
                    meta: ProposalMetaV1 {
                        proposal_id: todo_id.clone(),
                        confidence: *confidence,
                        evidence: evidence.clone(),
                        public_rationale: evidence_span.clone(),
                        metadata: HashMap::new(),
                        schema_hint: schema_hint.clone(),
                    },
                    entity_id: todo_id.clone(),
                    entity_type: "Todo".to_string(),
                    name: evidence_span.clone(),
                    attributes: HashMap::new(),
                    description: None,
                });

                let rel_id = format!(
                    "rel::has_todo::{}::{}",
                    sanitize_id(&file_id),
                    sanitize_id(&todo_id)
                );
                non_deduped.push(ProposalV1::Relation {
                    meta: ProposalMetaV1 {
                        proposal_id: rel_id.clone(),
                        confidence: *confidence,
                        evidence,
                        public_rationale: "TODO appears in file.".to_string(),
                        metadata: HashMap::new(),
                        schema_hint: schema_hint.clone(),
                    },
                    relation_id: rel_id,
                    rel_type: "HasTodo".to_string(),
                    source: file_id,
                    target: todo_id,
                    attributes: HashMap::new(),
                });
            }
        }
    }

    let mut out = Vec::new();
    out.extend(entities.into_values());
    out.extend(non_deduped);
    out
}

fn fact_type_to_string(ft: &FactType) -> String {
    match ft {
        FactType::Recommendation => "Recommendation",
        FactType::Observation => "Observation",
        FactType::Causation => "Causation",
        FactType::Parameter => "Parameter",
        FactType::Comparison => "Comparison",
        FactType::Definition => "Definition",
        FactType::Procedure => "Procedure",
        FactType::Constraint => "Constraint",
        FactType::Heuristic => "Heuristic",
    }
    .to_string()
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(160)
        .collect()
}

fn sanitize_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect()
}

fn truncate_for_name(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.len() <= max {
        return s.to_string();
    }

    let mut out = s.chars().take(max).collect::<String>();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(proposal_id: &str) -> ProposalMetaV1 {
        ProposalMetaV1 {
            proposal_id: proposal_id.to_string(),
            confidence: 1.0,
            evidence: Vec::new(),
            public_rationale: "test".to_string(),
            metadata: HashMap::new(),
            schema_hint: None,
        }
    }

    fn file(proposals: Vec<ProposalV1>) -> ProposalsFileV1 {
        ProposalsFileV1 {
            version: PROPOSALS_VERSION_V1,
            generated_at: "0".to_string(),
            source: ProposalSourceV1 {
                source_type: "test".to_string(),
                locator: "unit".to_string(),
            },
            schema_hint: None,
            proposals,
        }
    }

    fn entity(proposal_id: &str, entity_id: &str) -> ProposalV1 {
        ProposalV1::Entity {
            meta: meta(proposal_id),
            entity_id: entity_id.to_string(),
            entity_type: "Node".to_string(),
            name: entity_id.to_string(),
            attributes: HashMap::new(),
            description: None,
        }
    }

    #[test]
    fn required_identifiers_and_evidence_chunk_ids_are_nonempty() {
        let error = validate_proposals_file_v1(&file(vec![entity("proposal", "")]))
            .expect_err("empty entity id must reject");
        assert!(error.to_string().contains("must be non-empty"));

        let mut invalid = entity("proposal", "entity");
        let ProposalV1::Entity { meta, .. } = &mut invalid else {
            unreachable!();
        };
        meta.evidence.push(EvidencePointer {
            chunk_id: "  ".to_string(),
            locator: None,
            span_id: None,
        });
        let error = validate_proposals_file_v1(&file(vec![invalid]))
            .expect_err("empty evidence chunk id must reject");
        assert!(error.to_string().contains("evidence chunk id"));
    }

    #[test]
    fn duplicate_proposal_and_carrier_ids_reject() {
        let error = validate_proposals_file_v1(&file(vec![
            entity("same", "first"),
            entity("same", "second"),
        ]))
        .expect_err("duplicate proposal id must reject");
        assert!(error.to_string().contains("duplicate proposal id"));

        let error = validate_proposals_file_v1(&file(vec![
            entity("first", "same"),
            entity("second", "same"),
        ]))
        .expect_err("duplicate entity id must reject");
        assert!(error.to_string().contains("duplicate entity id"));
    }
}
