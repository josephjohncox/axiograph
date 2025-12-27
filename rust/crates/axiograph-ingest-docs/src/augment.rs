//! Proposals augmentation (higher-level discovery loop).
//!
//! `proposals.json` is intentionally **generic** and **domain-agnostic**.
//! This module adds a deterministic “augmentation” pass that:
//!
//! - improves routing hints (`schema_hint`) when missing,
//! - derives additional *structure* (e.g. roles for `Mention` entities),
//! - and emits a trace describing what changed (safe alternative to “chain-of-thought logs”).
//!
//! The output remains in the **evidence plane**:
//! - it is untrusted,
//! - reviewable,
//! - and intended to be promoted explicitly into candidate `.axi` modules.
//!
//! In other words:
//!   proposals → augment (heuristics/LLM) → proposals' → promote → candidate `.axi`
//!
//! LLM-driven augmentation is intentionally implemented at the CLI layer so this crate
//! stays usable in restricted environments.

use crate::{EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const PROPOSALS_AUGMENT_TRACE_VERSION_V1: u32 = 1;

/// Options for deterministic proposal augmentation.
#[derive(Debug, Clone)]
pub struct AugmentOptionsV1 {
    /// If true, fill missing per-proposal `schema_hint` using lightweight heuristics.
    pub infer_schema_hints: bool,
    /// If true, turn `Mention.role` into an explicit `Role` entity + `HasRole` edges.
    pub add_mention_role_entities: bool,
    /// If true, link TODO entities to symbols they mention in the same file.
    pub add_todo_mentions_symbol: bool,
    /// Safety valve: maximum number of *new* proposals to add.
    pub max_new_proposals: usize,
    /// If true, an inferred hint may overwrite an existing one.
    pub overwrite_schema_hints: bool,
}

impl Default for AugmentOptionsV1 {
    fn default() -> Self {
        Self {
            infer_schema_hints: true,
            add_mention_role_entities: true,
            add_todo_mentions_symbol: true,
            max_new_proposals: 25_000,
            overwrite_schema_hints: false,
        }
    }
}

/// Trace for an augmentation run (what was added/changed and why).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalsAugmentTraceV1 {
    pub version: u32,
    pub trace_id: String,
    pub generated_at: String,
    pub source: ProposalSourceV1,
    pub summary: AugmentSummaryV1,
    pub actions: Vec<AugmentActionV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AugmentSummaryV1 {
    pub proposals_in: usize,
    pub proposals_out: usize,
    pub schema_hints_set: usize,
    pub proposals_added: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AugmentActionV1 {
    SetSchemaHint {
        proposal_id: String,
        old_hint: Option<String>,
        new_hint: String,
        public_rationale: String,
    },
    AddProposal {
        proposal_id: String,
        public_rationale: String,
    },
}

/// Deterministically augment a proposals file (no LLM).
///
/// This function is intentionally pure with respect to time: callers supply
/// `trace_id` and `generated_at` so tests can be deterministic.
pub fn augment_proposals_v1(
    input: &ProposalsFileV1,
    trace_id: String,
    generated_at: String,
    options: &AugmentOptionsV1,
) -> Result<(ProposalsFileV1, ProposalsAugmentTraceV1)> {
    let mut out = input.clone();
    out.generated_at = generated_at.clone();

    let proposals_in = out.proposals.len();

    let mut actions: Vec<AugmentActionV1> = Vec::new();
    let mut schema_hints_set = 0usize;
    let mut proposals_added = 0usize;

    // Track existing proposal IDs to keep the file stable when repeatedly augmented.
    let mut existing_ids: HashSet<String> = HashSet::new();
    for p in &out.proposals {
        existing_ids.insert(proposal_meta(p).proposal_id.clone());
    }

    if options.infer_schema_hints {
        for p in &mut out.proposals {
            let old = proposal_meta(p).schema_hint.clone();
            if old.is_some() && !options.overwrite_schema_hints {
                continue;
            }

            let inferred = infer_schema_hint_for_proposal(p, input.schema_hint.as_deref());
            let Some(new_hint) = inferred else { continue };

            if old.as_deref() == Some(new_hint.as_str()) {
                continue;
            }

            let proposal_id = proposal_meta(p).proposal_id.clone();
            let meta = proposal_meta_mut(p);
            meta.schema_hint = Some(new_hint.clone());
            schema_hints_set += 1;
            actions.push(AugmentActionV1::SetSchemaHint {
                proposal_id,
                old_hint: old,
                new_hint,
                public_rationale:
                    "Filled missing schema hint using proposal metadata/attributes (heuristic)."
                        .to_string(),
            });
        }
    }

    // Derived proposals (new entities/relations) are appended, so we keep the original
    // ordering stable and make the augmentation diff easy to review.
    let mut new_proposals: Vec<ProposalV1> = Vec::new();

    if options.add_mention_role_entities {
        let mut roles: BTreeMap<String, RoleAggregate> = BTreeMap::new();
        for p in &out.proposals {
            let ProposalV1::Entity {
                meta,
                entity_type,
                entity_id,
                attributes,
                ..
            } = p
            else {
                continue;
            };
            if entity_type != "Mention" {
                continue;
            }
            let Some(role) = attributes
                .get("role")
                .cloned()
                .filter(|s| !s.trim().is_empty())
            else {
                continue;
            };
            let agg = roles.entry(role).or_insert_with(RoleAggregate::default);
            agg.mentions.push(entity_id.clone());
            agg.confidence = agg.confidence.max(meta.confidence);
            agg.evidence.extend(meta.evidence.iter().cloned());
            if agg.schema_hint.is_none() {
                agg.schema_hint = meta.schema_hint.clone();
            } else if agg.schema_hint != meta.schema_hint {
                // Mixed domains → keep it explicit by dropping the hint.
                agg.schema_hint = None;
            }
        }

        for (role, agg) in roles {
            if proposals_added >= options.max_new_proposals {
                break;
            }

            let role_entity_id = format!("role::{}", sanitize_id(&role));
            if existing_ids.contains(&role_entity_id) {
                continue;
            }

            let mut attrs = HashMap::new();
            attrs.insert("role".to_string(), role.clone());
            attrs.insert(
                "derived_from".to_string(),
                "augment_mentions_roles_v1".to_string(),
            );

            let evidence = dedup_evidence(agg.evidence, 8);

            let role_entity = ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: role_entity_id.clone(),
                    confidence: agg.confidence.max(0.70).min(0.95),
                    evidence: evidence.clone(),
                    public_rationale: format!(
                        "Observed mention role `{}` across {} extracted mentions; representing it explicitly as an entity.",
                        role,
                        agg.mentions.len()
                    ),
                    metadata: HashMap::new(),
                    schema_hint: agg.schema_hint.clone(),
                },
                entity_id: role_entity_id.clone(),
                entity_type: "Role".to_string(),
                name: role.clone(),
                attributes: attrs,
                description: None,
            };

            existing_ids.insert(role_entity_id.clone());
            proposals_added += 1;
            actions.push(AugmentActionV1::AddProposal {
                proposal_id: role_entity_id.clone(),
                public_rationale: "Derived Role entity from Mention.role (heuristic).".to_string(),
            });
            new_proposals.push(role_entity);

            for mention_id in agg.mentions {
                if proposals_added >= options.max_new_proposals {
                    break;
                }
                let rel_id = format!(
                    "rel::has_role::{}::{}",
                    sanitize_id(&mention_id),
                    sanitize_id(&role_entity_id)
                );
                if existing_ids.contains(&rel_id) {
                    continue;
                }

                let rel = ProposalV1::Relation {
                    meta: ProposalMetaV1 {
                        proposal_id: rel_id.clone(),
                        confidence: 0.85,
                        evidence: evidence.clone(),
                        public_rationale: format!("Mention `{}` has role `{}`.", mention_id, role),
                        metadata: HashMap::new(),
                        schema_hint: agg.schema_hint.clone(),
                    },
                    relation_id: rel_id.clone(),
                    rel_type: "HasRole".to_string(),
                    source: mention_id,
                    target: role_entity_id.clone(),
                    attributes: HashMap::new(),
                };
                existing_ids.insert(rel_id.clone());
                proposals_added += 1;
                actions.push(AugmentActionV1::AddProposal {
                    proposal_id: rel_id,
                    public_rationale: "Derived HasRole edge from Mention.role (heuristic)."
                        .to_string(),
                });
                new_proposals.push(rel);
            }
        }
    }

    if options.add_todo_mentions_symbol && proposals_added < options.max_new_proposals {
        // file -> [symbol ids]
        let mut symbols_by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
        // symbol id -> display name
        let mut symbol_name: HashMap<String, String> = HashMap::new();
        // file -> [todo ids]
        let mut todos_by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
        // entity id -> meta (for evidence/schema hint)
        let mut entity_meta: HashMap<String, ProposalMetaV1> = HashMap::new();
        // relation id -> meta
        let mut relation_meta: HashMap<String, ProposalMetaV1> = HashMap::new();

        for p in &out.proposals {
            match p {
                ProposalV1::Entity {
                    meta,
                    entity_id,
                    name,
                    entity_type,
                    ..
                } => {
                    entity_meta.insert(entity_id.clone(), meta.clone());
                    if entity_type == "Symbol" {
                        symbol_name.insert(entity_id.clone(), name.clone());
                    }
                }
                ProposalV1::Relation {
                    meta,
                    relation_id,
                    rel_type,
                    source,
                    target,
                    ..
                } => {
                    relation_meta.insert(relation_id.clone(), meta.clone());
                    if rel_type == "DefinesSymbol" {
                        symbols_by_file
                            .entry(source.clone())
                            .or_default()
                            .push(target.clone());
                    } else if rel_type == "HasTodo" {
                        todos_by_file
                            .entry(source.clone())
                            .or_default()
                            .push(target.clone());
                    }
                }
            }
        }

        // For each TODO, check whether it mentions any symbol defined in the same file.
        for (file_id, todo_ids) in todos_by_file {
            let Some(symbols) = symbols_by_file.get(&file_id) else {
                continue;
            };
            for todo_id in todo_ids {
                if proposals_added >= options.max_new_proposals {
                    break;
                }
                let Some(todo_text) = out.proposals.iter().find_map(|p| match p {
                    ProposalV1::Entity {
                        entity_id,
                        name,
                        entity_type,
                        ..
                    } if entity_id == &todo_id && entity_type == "Todo" => Some(name.clone()),
                    _ => None,
                }) else {
                    continue;
                };

                let todo_meta = entity_meta.get(&todo_id).cloned();

                for symbol_id in symbols {
                    if proposals_added >= options.max_new_proposals {
                        break;
                    }
                    let Some(sym_name) = symbol_name.get(symbol_id) else {
                        continue;
                    };
                    if !text_mentions_identifier(&todo_text, sym_name) {
                        continue;
                    }

                    let rel_id = format!(
                        "rel::todo_mentions_symbol::{}::{}",
                        sanitize_id(&todo_id),
                        sanitize_id(symbol_id)
                    );
                    if existing_ids.contains(&rel_id) {
                        continue;
                    }

                    let mut evidence: Vec<EvidencePointer> = Vec::new();
                    let mut schema_hint = None;
                    if let Some(m) = todo_meta.as_ref() {
                        evidence.extend(m.evidence.iter().cloned());
                        schema_hint = m.schema_hint.clone();
                    }
                    // Add a little more provenance if present.
                    if let Some(def_meta) = relation_meta.get(&format!(
                        "rel::defines::{}::{}",
                        sanitize_id(&file_id),
                        sanitize_id(symbol_id)
                    )) {
                        evidence.extend(def_meta.evidence.iter().cloned());
                        if schema_hint.is_none() {
                            schema_hint = def_meta.schema_hint.clone();
                        }
                    }
                    evidence = dedup_evidence(evidence, 10);

                    let rel = ProposalV1::Relation {
                        meta: ProposalMetaV1 {
                            proposal_id: rel_id.clone(),
                            confidence: 0.70,
                            evidence,
                            public_rationale: format!(
                                "TODO text mentions `{}`; linking to symbol defined in the same file {}.",
                                sym_name, file_id
                            ),
                            metadata: {
                                let mut m = HashMap::new();
                                m.insert("file".to_string(), file_id.clone());
                                m
                            },
                            schema_hint,
                        },
                        relation_id: rel_id.clone(),
                        rel_type: "TodoMentionsSymbol".to_string(),
                        source: todo_id.clone(),
                        target: symbol_id.clone(),
                        attributes: HashMap::new(),
                    };
                    existing_ids.insert(rel_id.clone());
                    proposals_added += 1;
                    actions.push(AugmentActionV1::AddProposal {
                        proposal_id: rel_id,
                        public_rationale: "Linked Todo→Symbol by string mention (heuristic)."
                            .to_string(),
                    });
                    new_proposals.push(rel);
                }
            }
        }
    }

    out.proposals.extend(new_proposals);

    let proposals_out = out.proposals.len();
    Ok((
        out,
        ProposalsAugmentTraceV1 {
            version: PROPOSALS_AUGMENT_TRACE_VERSION_V1,
            trace_id,
            generated_at,
            source: input.source.clone(),
            summary: AugmentSummaryV1 {
                proposals_in,
                proposals_out,
                schema_hints_set,
                proposals_added,
            },
            actions,
        },
    ))
}

#[derive(Debug, Clone, Default)]
struct RoleAggregate {
    mentions: Vec<String>,
    evidence: Vec<EvidencePointer>,
    confidence: f64,
    schema_hint: Option<String>,
}

fn proposal_meta(p: &ProposalV1) -> &ProposalMetaV1 {
    match p {
        ProposalV1::Entity { meta, .. } => meta,
        ProposalV1::Relation { meta, .. } => meta,
    }
}

fn proposal_meta_mut(p: &mut ProposalV1) -> &mut ProposalMetaV1 {
    match p {
        ProposalV1::Entity { meta, .. } => meta,
        ProposalV1::Relation { meta, .. } => meta,
    }
}

fn normalize_hint(s: &str) -> String {
    s.trim().to_lowercase().replace('-', "_")
}

fn canonical_hint(s: &str) -> Option<String> {
    let norm = normalize_hint(s);
    if norm.is_empty() {
        return None;
    }
    match norm.as_str() {
        // Match `promotion.rs` domain routing synonyms.
        "economicflows" | "economic_flows" | "economics" | "economy" => {
            Some("economic_flows".to_string())
        }
        "machinistlearning" | "machinist_learning" | "machining" | "learning" => {
            Some("machinist_learning".to_string())
        }
        "schemaevolution" | "schema_evolution" | "ontology" | "migrations" | "migration"
        | "schema" => Some("schema_evolution".to_string()),
        other => Some(other.to_string()),
    }
}

fn infer_schema_hint_for_proposal(p: &ProposalV1, file_hint: Option<&str>) -> Option<String> {
    let meta = proposal_meta(p);
    if let Some(h) = meta.schema_hint.as_deref() {
        return canonical_hint(h);
    }

    // Prefer explicit per-proposal domain metadata.
    if let Some(d) = meta.metadata.get("domain").map(|s| s.as_str()) {
        if let Some(h) = canonical_hint(d) {
            return Some(h);
        }
    }

    match p {
        ProposalV1::Entity { attributes, .. } => {
            if let Some(d) = attributes.get("domain").map(|s| s.as_str()) {
                if let Some(h) = canonical_hint(d) {
                    return Some(h);
                }
            }
        }
        ProposalV1::Relation { .. } => {}
    }

    // Fall back to file-level hint if present.
    file_hint.and_then(canonical_hint)
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

fn dedup_evidence(mut ev: Vec<EvidencePointer>, max: usize) -> Vec<EvidencePointer> {
    let mut seen: BTreeSet<(String, Option<String>, Option<String>)> = BTreeSet::new();
    ev.retain(|e| seen.insert((e.chunk_id.clone(), e.locator.clone(), e.span_id.clone())));
    ev.truncate(max);
    ev
}

fn text_mentions_identifier(text: &str, ident: &str) -> bool {
    if ident.trim().is_empty() {
        return false;
    }
    // A simple boundary-ish check that avoids substring noise for common names.
    // Example: `PathDB` should not match `PathDBExport`.
    let needle = ident;
    let hay = text;
    if let Some(i) = hay.find(needle) {
        let before = hay[..i].chars().rev().next();
        let after = hay[i + needle.len()..].chars().next();
        let ok_before = before
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true);
        let ok_after = after
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true);
        return ok_before && ok_after;
    }
    false
}
