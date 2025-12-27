//! Proposals → candidate domain `.axi` modules (promotion stage).
//!
//! This is the bridge from “GraphRAG-shaped ingestion artifacts” (`proposals.json`)
//! into Axiograph’s **explicit** accepted-knowledge workflow:
//!
//! 1. Ingestion emits *untrusted* proposals with evidence pointers (`proposals.json`).
//! 2. This module performs:
//!    - **entity resolution** (merge duplicates, record conflicts),
//!    - **schema mapping** into a small set of canonical domain modules.
//! 3. The output is a set of *candidate* `.axi` files intended for **human review**.
//! 4. Promotion into canonical `.axi` is explicit (manual merge / policy gate).
//!
//! Today we target the canonical example modules:
//! - `EconomicFlows` (axi_schema_v1)
//! - `MachinistLearning` (axi_schema_v1)
//! - `SchemaEvolution` (axi_schema_v1)

#![allow(unused_imports)]

use crate::{EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const PROMOTION_TRACE_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromotionDomainV1 {
    EconomicFlows,
    MachinistLearning,
    SchemaEvolution,
}

impl PromotionDomainV1 {
    pub const fn canonical_module_name(self) -> &'static str {
        match self {
            PromotionDomainV1::EconomicFlows => "EconomicFlows",
            PromotionDomainV1::MachinistLearning => "MachinistLearning",
            PromotionDomainV1::SchemaEvolution => "SchemaEvolution",
        }
    }

    pub const fn candidate_module_name(self) -> &'static str {
        match self {
            PromotionDomainV1::EconomicFlows => "EconomicFlows_Proposals",
            PromotionDomainV1::MachinistLearning => "MachinistLearning_Proposals",
            PromotionDomainV1::SchemaEvolution => "SchemaEvolution_Proposals",
        }
    }

    pub const fn default_output_file(self) -> &'static str {
        match self {
            PromotionDomainV1::EconomicFlows => "EconomicFlows.proposals.axi",
            PromotionDomainV1::MachinistLearning => "MachinistLearning.proposals.axi",
            PromotionDomainV1::SchemaEvolution => "SchemaEvolution.proposals.axi",
        }
    }
}

impl std::fmt::Display for PromotionDomainV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.canonical_module_name())
    }
}

/// Options for the promotion stage.
#[derive(Debug, Clone)]
pub struct PromoteOptionsV1 {
    /// Drop proposals below this confidence threshold.
    pub min_confidence: f64,
    /// Domains to emit (others are ignored).
    pub domains: BTreeSet<PromotionDomainV1>,
}

impl Default for PromoteOptionsV1 {
    fn default() -> Self {
        Self {
            min_confidence: 0.0,
            domains: BTreeSet::from([
                PromotionDomainV1::EconomicFlows,
                PromotionDomainV1::MachinistLearning,
                PromotionDomainV1::SchemaEvolution,
            ]),
        }
    }
}

/// Result of promoting a `proposals.json` file into candidate domain modules.
#[derive(Debug, Clone)]
pub struct PromoteResultV1 {
    pub candidates: BTreeMap<PromotionDomainV1, String>,
    pub trace: PromotionTraceV1,
}

/// Promotion trace: what we emitted (and what we skipped), so promotion can be explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionTraceV1 {
    pub version: u32,
    pub generated_at: String,
    pub source: ProposalSourceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_hint: Option<String>,
    pub domain_summaries: Vec<DomainSummaryV1>,
    pub conflicts: Vec<EntityConflictV1>,
    pub unmapped: Vec<UnmappedProposalV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainSummaryV1 {
    pub domain: PromotionDomainV1,
    pub candidates_emitted: usize,
    pub proposals_consumed: usize,
    pub output_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityConflictV1 {
    pub domain: PromotionDomainV1,
    pub entity_type: String,
    pub name_key: String,
    pub attribute: String,
    pub chosen: String,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmappedProposalV1 {
    pub domain: PromotionDomainV1,
    pub proposal_id: String,
    pub kind: String,
    pub reason: String,
}

// =============================================================================
// Entry point
// =============================================================================

pub fn promote_proposals_to_candidates_v1(
    file: &ProposalsFileV1,
    options: &PromoteOptionsV1,
) -> Result<PromoteResultV1> {
    let mut candidates: BTreeMap<PromotionDomainV1, String> = BTreeMap::new();
    let mut domain_summaries: Vec<DomainSummaryV1> = Vec::new();
    let mut conflicts: Vec<EntityConflictV1> = Vec::new();
    let mut unmapped: Vec<UnmappedProposalV1> = Vec::new();

    for domain in &options.domains {
        let domain = *domain;

        let domain_proposals: Vec<ProposalV1> = file
            .proposals
            .iter()
            .cloned()
            .filter(|p| proposal_confidence(p) >= options.min_confidence)
            .filter(|p| proposal_domain(p, file.schema_hint.as_deref()) == Some(domain))
            .collect();

        let (axi, summary, dom_conflicts, dom_unmapped) =
            promote_domain(domain, &domain_proposals, file)?;

        if !axi.trim().is_empty() {
            candidates.insert(domain, axi);
        }
        domain_summaries.push(DomainSummaryV1 {
            domain,
            candidates_emitted: summary.candidates_emitted,
            proposals_consumed: domain_proposals.len(),
            output_file: domain.default_output_file().to_string(),
        });
        conflicts.extend(dom_conflicts);
        unmapped.extend(dom_unmapped);
    }

    Ok(PromoteResultV1 {
        candidates,
        trace: PromotionTraceV1 {
            version: PROMOTION_TRACE_VERSION_V1,
            generated_at: file.generated_at.clone(),
            source: file.source.clone(),
            schema_hint: file.schema_hint.clone(),
            domain_summaries,
            conflicts,
            unmapped,
        },
    })
}

// =============================================================================
// Domain routing
// =============================================================================

fn normalize_hint(s: &str) -> String {
    s.trim().to_lowercase().replace('-', "_")
}

fn proposal_domain(p: &ProposalV1, file_hint: Option<&str>) -> Option<PromotionDomainV1> {
    let hint = match p {
        ProposalV1::Entity { meta, .. } => meta.schema_hint.as_deref().or(file_hint),
        ProposalV1::Relation { meta, .. } => meta.schema_hint.as_deref().or(file_hint),
    }?;

    match normalize_hint(hint).as_str() {
        "economicflows" | "economic_flows" | "economics" | "economy" => {
            Some(PromotionDomainV1::EconomicFlows)
        }
        "machinistlearning" | "machinist_learning" | "machining" | "learning" => {
            Some(PromotionDomainV1::MachinistLearning)
        }
        "schemaevolution" | "schema_evolution" | "ontology" | "migrations" | "migration" => {
            Some(PromotionDomainV1::SchemaEvolution)
        }
        _ => None,
    }
}

fn proposal_confidence(p: &ProposalV1) -> f64 {
    match p {
        ProposalV1::Entity { meta, .. } => meta.confidence,
        ProposalV1::Relation { meta, .. } => meta.confidence,
    }
}

// =============================================================================
// Entity resolution (per-domain)
// =============================================================================

#[derive(Debug, Clone)]
struct ResolvedEntity {
    /// Stable within a single promotion run: `(entity_type, normalized_name)` key.
    key: String,
    entity_type: String,
    display_name: String,
    description: Option<String>,
    confidence: f64,
    attributes: HashMap<String, String>,
    evidence: Vec<EvidencePointer>,
    proposal_ids: Vec<String>,
    raw_entity_ids: Vec<String>,
    raw_names: Vec<String>,
}

#[derive(Debug, Clone)]
struct ResolutionIndex {
    by_entity_id: HashMap<String, String>, // entity_id -> resolved key
    entities: BTreeMap<String, ResolvedEntity>, // key -> merged
}

fn normalize_name_key(s: &str) -> String {
    let mut out = String::new();
    let mut prev_underscore = false;
    for c in s.trim().chars() {
        let c = c.to_ascii_lowercase();
        let is_ok = c.is_ascii_alphanumeric();
        if is_ok {
            out.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn sanitize_ident(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if out.is_empty() {
        out.push('X');
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn merge_evidence(mut a: Vec<EvidencePointer>, b: &[EvidencePointer]) -> Vec<EvidencePointer> {
    let mut seen: HashSet<(String, Option<String>, Option<String>)> = HashSet::new();
    for e in &a {
        seen.insert((e.chunk_id.clone(), e.locator.clone(), e.span_id.clone()));
    }
    for e in b {
        let key = (e.chunk_id.clone(), e.locator.clone(), e.span_id.clone());
        if seen.insert(key) {
            a.push(e.clone());
        }
    }
    a
}

fn resolve_entities(
    domain: PromotionDomainV1,
    proposals: &[ProposalV1],
) -> (ResolutionIndex, Vec<EntityConflictV1>) {
    let mut by_entity_id: HashMap<String, String> = HashMap::new();
    let mut entities: BTreeMap<String, ResolvedEntity> = BTreeMap::new();
    let mut conflicts: Vec<EntityConflictV1> = Vec::new();

    for p in proposals {
        let ProposalV1::Entity {
            meta,
            entity_id,
            entity_type,
            name,
            attributes,
            description,
        } = p
        else {
            continue;
        };

        let key_name = normalize_name_key(name);
        let key = format!("{}::{}", entity_type, key_name);
        by_entity_id.insert(entity_id.clone(), key.clone());

        match entities.get_mut(&key) {
            None => {
                entities.insert(
                    key.clone(),
                    ResolvedEntity {
                        key,
                        entity_type: entity_type.clone(),
                        display_name: name.clone(),
                        description: description.clone(),
                        confidence: meta.confidence,
                        attributes: attributes.clone(),
                        evidence: meta.evidence.clone(),
                        proposal_ids: vec![meta.proposal_id.clone()],
                        raw_entity_ids: vec![entity_id.clone()],
                        raw_names: vec![name.clone()],
                    },
                );
            }
            Some(existing) => {
                existing.proposal_ids.push(meta.proposal_id.clone());
                existing.raw_entity_ids.push(entity_id.clone());
                existing.raw_names.push(name.clone());
                existing.evidence = merge_evidence(existing.evidence.clone(), &meta.evidence);

                if meta.confidence > existing.confidence {
                    existing.confidence = meta.confidence;
                    existing.display_name = name.clone();
                    if description.is_some() {
                        existing.description = description.clone();
                    }
                }

                for (k, v) in attributes {
                    match existing.attributes.get(k) {
                        None => {
                            existing.attributes.insert(k.clone(), v.clone());
                        }
                        Some(prev) if prev == v => {}
                        Some(prev) => {
                            // Reconcile by confidence: keep existing unless this proposal is higher-confidence.
                            if meta.confidence >= existing.confidence {
                                conflicts.push(EntityConflictV1 {
                                    domain,
                                    entity_type: entity_type.clone(),
                                    name_key: key_name.clone(),
                                    attribute: k.clone(),
                                    chosen: v.clone(),
                                    rejected: vec![prev.clone()],
                                });
                                existing.attributes.insert(k.clone(), v.clone());
                            } else {
                                conflicts.push(EntityConflictV1 {
                                    domain,
                                    entity_type: entity_type.clone(),
                                    name_key: key_name.clone(),
                                    attribute: k.clone(),
                                    chosen: prev.clone(),
                                    rejected: vec![v.clone()],
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    (
        ResolutionIndex {
            by_entity_id,
            entities,
        },
        conflicts,
    )
}

fn resolve_entity_ref(
    raw: &str,
    index: &ResolutionIndex,
    fallback_by_name: &BTreeMap<String, String>,
) -> Option<String> {
    if let Some(key) = index.by_entity_id.get(raw) {
        return Some(key.clone());
    }
    // Fallback: treat `raw` as a name; only if unambiguous.
    let name_key = normalize_name_key(raw);
    fallback_by_name.get(&name_key).cloned()
}

// =============================================================================
// Domain promotion
// =============================================================================

struct DomainEmitSummary {
    candidates_emitted: usize,
}

fn promote_domain(
    domain: PromotionDomainV1,
    proposals: &[ProposalV1],
    file: &ProposalsFileV1,
) -> Result<(
    String,
    DomainEmitSummary,
    Vec<EntityConflictV1>,
    Vec<UnmappedProposalV1>,
)> {
    let (resolved, conflicts) = resolve_entities(domain, proposals);

    // If a raw name uniquely maps to one resolved entity, we can resolve endpoints by name.
    let mut fallback_by_name: BTreeMap<String, String> = BTreeMap::new();
    let mut collisions: HashSet<String> = HashSet::new();
    for entity in resolved.entities.values() {
        let nk = normalize_name_key(&entity.display_name);
        if fallback_by_name.contains_key(&nk) {
            collisions.insert(nk);
        } else {
            fallback_by_name.insert(nk, entity.key.clone());
        }
    }
    for nk in collisions {
        fallback_by_name.remove(&nk);
    }

    match domain {
        PromotionDomainV1::MachinistLearning => {
            let (axi, count, unmapped) = emit_machinist_learning(file, proposals, &resolved)?;
            Ok((
                axi,
                DomainEmitSummary {
                    candidates_emitted: count,
                },
                conflicts,
                unmapped,
            ))
        }
        PromotionDomainV1::EconomicFlows => {
            let (axi, count, unmapped) =
                emit_economic_flows(file, proposals, &resolved, &fallback_by_name)?;
            Ok((
                axi,
                DomainEmitSummary {
                    candidates_emitted: count,
                },
                conflicts,
                unmapped,
            ))
        }
        PromotionDomainV1::SchemaEvolution => {
            let (axi, count, unmapped) =
                emit_schema_evolution(file, proposals, &resolved, &fallback_by_name)?;
            Ok((
                axi,
                DomainEmitSummary {
                    candidates_emitted: count,
                },
                conflicts,
                unmapped,
            ))
        }
    }
}

// =============================================================================
// MachinistLearning (axi_schema_v1): emit an `instance ... of MachiningLearning:` patch
// =============================================================================

fn confidence_ident(confidence: f64) -> String {
    // Keep it stable and identifier-safe: 0.953 → Conf_0_953
    let s = format!("{confidence:.3}").replace('.', "_");
    format!("Conf_{s}")
}

fn text_ident(base: &str, suffix: &str) -> String {
    format!("Text_{}_{}", base, suffix)
}

fn add_concept_candidate(
    builder: &mut SchemaInstanceBuilder,
    header_lines: &mut Vec<String>,
    ident: &str,
    description: &str,
    difficulty: &str,
    prerequisites: &[String],
) {
    builder.add_object("Concept", ident.to_string());
    builder.add_relation_tuple(
        "conceptDifficulty",
        tuple(&[("concept", ident), ("value", &sanitize_ident(difficulty))]),
    );

    let desc_id = text_ident(ident, "description");
    builder.add_object("Text", desc_id.clone());
    builder.add_relation_tuple(
        "conceptDescription",
        tuple(&[("concept", ident), ("text", &desc_id)]),
    );

    if !description.trim().is_empty() {
        header_lines.push(format!("-- Concept {ident} description:"));
        for line in description.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            header_lines.push(format!("--   {line}"));
        }
    }

    for prereq in prerequisites {
        if prereq.is_empty() {
            continue;
        }
        builder.add_object("Concept", prereq.clone());
        builder.add_relation_tuple("requires", tuple(&[("concept", ident), ("prereq", prereq)]));
    }
}

fn add_guideline_candidate(
    builder: &mut SchemaInstanceBuilder,
    header_lines: &mut Vec<String>,
    ident: &str,
    title: &str,
    explanation: &str,
    severity: &str,
    visual: Option<&str>,
) {
    builder.add_object("SafetyGuideline", ident.to_string());
    builder.add_relation_tuple(
        "guidelineSeverity",
        tuple(&[("guideline", ident), ("value", &sanitize_ident(severity))]),
    );

    let expl_id = text_ident(ident, "explanation");
    builder.add_object("Text", expl_id.clone());
    builder.add_relation_tuple(
        "guidelineExplanation",
        tuple(&[("guideline", ident), ("text", &expl_id)]),
    );

    header_lines.push(format!("-- SafetyGuideline {ident}: {title}"));
    if !explanation.trim().is_empty() {
        for line in explanation.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            header_lines.push(format!("--   {line}"));
        }
    }

    if let Some(v) = visual {
        let vis_id = text_ident(ident, "visual");
        builder.add_object("Text", vis_id.clone());
        builder.add_relation_tuple(
            "guidelineVisualExample",
            tuple(&[("guideline", ident), ("text", &vis_id)]),
        );
        header_lines.push(format!("--   visualExample: {v}"));
    }
}

fn add_tacit_candidate(
    builder: &mut SchemaInstanceBuilder,
    header_lines: &mut Vec<String>,
    ident: &str,
    rule: &str,
    source: &str,
    confidence: f64,
) {
    builder.add_object("TacitKnowledge", ident.to_string());

    let rule_id = text_ident(ident, "rule");
    builder.add_object("Text", rule_id.clone());
    builder.add_relation_tuple("tacitRule", tuple(&[("tacit", ident), ("text", &rule_id)]));

    let src_id = text_ident(ident, "source");
    builder.add_object("Text", src_id.clone());
    builder.add_relation_tuple("tacitSource", tuple(&[("tacit", ident), ("text", &src_id)]));

    let conf_id = confidence_ident(confidence);
    builder.add_object("Confidence", conf_id.clone());
    builder.add_relation_tuple(
        "tacitConfidence",
        tuple(&[("tacit", ident), ("value", &conf_id)]),
    );

    header_lines.push(format!(
        "-- TacitKnowledge {ident} (confidence={confidence:.3}):"
    ));
    if !rule.trim().is_empty() {
        header_lines.push("--   rule:".to_string());
        for line in rule.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            header_lines.push(format!("--     {line}"));
        }
    }
    if !source.trim().is_empty() {
        header_lines.push(format!("--   source: {source}"));
    }
}

fn add_example_candidate(
    builder: &mut SchemaInstanceBuilder,
    header_lines: &mut Vec<String>,
    ident: &str,
    description: &str,
) {
    builder.add_object("Example", ident.to_string());

    let desc_id = text_ident(ident, "description");
    builder.add_object("Text", desc_id.clone());
    builder.add_relation_tuple(
        "exampleDescription",
        tuple(&[("example", ident), ("text", &desc_id)]),
    );

    header_lines.push(format!("-- Example {ident}:"));
    if !description.trim().is_empty() {
        for line in description.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            header_lines.push(format!("--   {line}"));
        }
    }
}

fn emit_machinist_learning(
    file: &ProposalsFileV1,
    proposals: &[ProposalV1],
    resolved: &ResolutionIndex,
) -> Result<(String, usize, Vec<UnmappedProposalV1>)> {
    let mut builder = SchemaInstanceBuilder::new(
        PromotionDomainV1::MachinistLearning.candidate_module_name(),
        "MachiningLearning",
        "ProposedMachiningLearning",
    );

    let mut header_lines: Vec<String> = Vec::new();
    header_lines.push("-- GENERATED (candidate) — proposals → MachinistLearning".to_string());
    header_lines.push("-- This file is NOT canonical. Promotion must be explicit.".to_string());
    header_lines.push(
        "-- To promote: review and merge into `examples/learning/MachinistLearning.axi`."
            .to_string(),
    );
    header_lines.push(format!(
        "-- Source: {} ({})",
        file.source.locator, file.source.source_type
    ));
    header_lines.push(format!("-- generated_at: {}", file.generated_at));

    let mut emitted = 0usize;
    let mut unmapped: Vec<UnmappedProposalV1> = Vec::new();

    let mut used_names: HashSet<String> = HashSet::new();

    // Deterministic order: by resolved entity key.
    for entity in resolved.entities.values() {
        let name_key = sanitize_ident(&entity.display_name);
        let base_ident = if name_key.is_empty() {
            "X".to_string()
        } else {
            name_key
        };
        let mut ident = base_ident.clone();
        let mut i = 2usize;
        while used_names.contains(&ident) {
            ident = format!("{base_ident}_{i}");
            i += 1;
        }
        used_names.insert(ident.clone());

        match entity.entity_type.as_str() {
            "TacitKnowledge" => {
                let rule = entity
                    .attributes
                    .get("rule")
                    .cloned()
                    .or_else(|| entity.description.clone())
                    .unwrap_or_else(|| entity.display_name.clone());
                let source = entity
                    .attributes
                    .get("source")
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                add_tacit_candidate(
                    &mut builder,
                    &mut header_lines,
                    &ident,
                    &rule,
                    &source,
                    entity.confidence,
                );
                emitted += 1;
            }
            "SafetyGuideline" => {
                let title = entity
                    .attributes
                    .get("title")
                    .cloned()
                    .unwrap_or_else(|| entity.display_name.clone());
                let explanation = entity
                    .attributes
                    .get("explanation")
                    .cloned()
                    .or_else(|| entity.description.clone())
                    .unwrap_or_else(|| "No explanation provided.".to_string());
                let severity = entity
                    .attributes
                    .get("severity")
                    .cloned()
                    .unwrap_or_else(|| infer_severity(entity.confidence));
                let visual = entity.attributes.get("visualExample").map(String::as_str);
                add_guideline_candidate(
                    &mut builder,
                    &mut header_lines,
                    &ident,
                    &title,
                    &explanation,
                    &severity,
                    visual,
                );
                emitted += 1;
            }
            "Concept" => {
                let description = entity
                    .attributes
                    .get("description")
                    .cloned()
                    .or_else(|| entity.description.clone())
                    .unwrap_or_else(|| "No description provided.".to_string());
                let difficulty = entity
                    .attributes
                    .get("difficulty")
                    .cloned()
                    .unwrap_or_else(|| "Beginner".to_string());
                let prerequisites = entity
                    .attributes
                    .get("prerequisites")
                    .map(|s| split_list_idents(s))
                    .unwrap_or_default();
                add_concept_candidate(
                    &mut builder,
                    &mut header_lines,
                    &ident,
                    &description,
                    &difficulty,
                    &prerequisites,
                );
                emitted += 1;
            }
            "Example" => {
                let description = entity
                    .attributes
                    .get("description")
                    .cloned()
                    .or_else(|| entity.description.clone())
                    .unwrap_or_else(|| entity.display_name.clone());
                add_example_candidate(&mut builder, &mut header_lines, &ident, &description);
                emitted += 1;
            }
            "Claim" => {
                // Claims are generic; map based on `fact_type`.
                // - Definition → Concept
                // - Constraint → SafetyGuideline
                // - otherwise → tacit knowledge
                let fact_type = entity
                    .attributes
                    .get("fact_type")
                    .cloned()
                    .or_else(|| entity.attributes.get("field_fact_type").cloned())
                    .unwrap_or_else(|| "Unknown".to_string());
                let statement = entity
                    .attributes
                    .get("statement")
                    .cloned()
                    .unwrap_or_else(|| entity.display_name.clone());

                match fact_type.as_str() {
                    "Definition" => {
                        add_concept_candidate(
                            &mut builder,
                            &mut header_lines,
                            &ident,
                            &statement,
                            "Beginner",
                            &[],
                        );
                        emitted += 1;
                    }
                    "Constraint" => {
                        let severity = infer_severity(entity.confidence);
                        add_guideline_candidate(
                            &mut builder,
                            &mut header_lines,
                            &ident,
                            &entity.display_name,
                            &statement,
                            &severity,
                            None,
                        );
                        emitted += 1;
                    }
                    _ => {
                        add_tacit_candidate(
                            &mut builder,
                            &mut header_lines,
                            &ident,
                            &statement,
                            "unknown",
                            entity.confidence,
                        );
                        emitted += 1;
                    }
                }
            }
            _ => {
                // Unknown to the MachinistLearning schema; keep in trace only.
            }
        }
    }

    // Unmapped proposals (domain-specific). Record anything we didn't use.
    // For now: relation proposals are preserved only in the trace.
    for p in proposals {
        match p {
            ProposalV1::Relation { meta, .. } => unmapped.push(UnmappedProposalV1 {
                domain: PromotionDomainV1::MachinistLearning,
                proposal_id: meta.proposal_id.clone(),
                kind: "relation".to_string(),
                reason: "machinist_learning promotion currently emits only entity-like candidates; relation proposals kept for later mapping".to_string(),
            }),
            ProposalV1::Entity { meta, entity_type, .. } => {
                if !matches!(
                    entity_type.as_str(),
                    "TacitKnowledge" | "SafetyGuideline" | "Concept" | "Example" | "Claim"
                ) {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::MachinistLearning,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "entity".to_string(),
                        reason: format!("unmapped entity_type `{entity_type}` for MachinistLearning promotion"),
                    });
                }
            }
        }
    }

    if emitted == 0 {
        return Ok((String::new(), 0, unmapped));
    }

    Ok((builder.render(&header_lines), emitted, unmapped))
}

fn infer_severity(confidence: f64) -> String {
    if confidence >= 0.92 {
        "Critical".to_string()
    } else if confidence >= 0.80 {
        "Warning".to_string()
    } else if confidence >= 0.65 {
        "Advisory".to_string()
    } else {
        "Info".to_string()
    }
}

fn split_list_idents(raw: &str) -> Vec<String> {
    raw.split([',', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| sanitize_ident(s))
        .collect()
}

// =============================================================================
// EconomicFlows (axi_schema_v1): emit an `instance ... of Economy:` patch
// =============================================================================

struct SchemaInstanceBuilder {
    module_name: String,
    schema_name: String,
    instance_name: String,
    objects: BTreeMap<String, BTreeSet<String>>,
    relations: BTreeMap<String, BTreeSet<String>>,
}

impl SchemaInstanceBuilder {
    fn new(module_name: &str, schema_name: &str, instance_name: &str) -> Self {
        Self {
            module_name: module_name.to_string(),
            schema_name: schema_name.to_string(),
            instance_name: instance_name.to_string(),
            objects: BTreeMap::new(),
            relations: BTreeMap::new(),
        }
    }

    fn add_object(&mut self, obj: &str, elem: String) {
        self.objects
            .entry(obj.to_string())
            .or_default()
            .insert(elem);
    }

    fn add_relation_tuple(&mut self, rel: &str, tuple: String) {
        self.relations
            .entry(rel.to_string())
            .or_default()
            .insert(tuple);
    }

    fn render(&self, header_lines: &[String]) -> String {
        let mut out = String::new();
        for line in header_lines {
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&format!("module {}\n\n", self.module_name));
        out.push_str(&format!(
            "instance {} of {}:\n",
            self.instance_name, self.schema_name
        ));

        for (obj, elems) in &self.objects {
            out.push_str(&render_set_assignment(obj, elems));
        }

        for (rel, tuples) in &self.relations {
            out.push_str(&render_set_assignment(rel, tuples));
        }

        out
    }
}

fn render_set_assignment(name: &str, items: &BTreeSet<String>) -> String {
    if items.is_empty() {
        return String::new();
    }
    let items: Vec<String> = items.iter().cloned().collect();

    // Small sets go inline; larger sets go multi-line for reviewability.
    if items.len() <= 6 && items.iter().map(|s| s.len()).sum::<usize>() <= 72 {
        return format!("  {name} = {{{}}}\n", items.join(", "));
    }

    let mut out = String::new();
    out.push_str(&format!("  {name} = {{\n"));
    for (idx, item) in items.iter().enumerate() {
        if idx + 1 == items.len() {
            out.push_str(&format!("    {item}\n"));
        } else {
            out.push_str(&format!("    {item},\n"));
        }
    }
    out.push_str("  }\n");
    out
}

fn elem_name(prefix: &str, display_name: &str, used: &mut HashSet<String>) -> String {
    let base = sanitize_ident(display_name);
    let base = if base.is_empty() {
        "X".to_string()
    } else {
        base
    };
    let base = format!("{prefix}_{base}");
    let mut candidate = base.clone();
    let mut i = 2usize;
    while used.contains(&candidate) {
        candidate = format!("{base}_{i}");
        i += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn tuple(fields: &[(&str, &str)]) -> String {
    let inner = fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({inner})")
}

fn emit_economic_flows(
    file: &ProposalsFileV1,
    proposals: &[ProposalV1],
    resolved: &ResolutionIndex,
    fallback_by_name: &BTreeMap<String, String>,
) -> Result<(String, usize, Vec<UnmappedProposalV1>)> {
    let mut builder = SchemaInstanceBuilder::new(
        PromotionDomainV1::EconomicFlows.candidate_module_name(),
        "Economy",
        "ProposedEconomy",
    );

    let mut used: HashSet<String> = HashSet::new();
    let mut key_to_elem: HashMap<String, String> = HashMap::new();
    let mut emitted = 0usize;
    let mut unmapped: Vec<UnmappedProposalV1> = Vec::new();

    // Entities
    for entity in resolved.entities.values() {
        let ty = entity.entity_type.as_str();
        let name = &entity.display_name;

        let object = match ty {
            "Household" | "Firm" | "Bank" | "Government" | "ForeignSector" | "Agent" => {
                // Also populate Agent (subtyping) for convenience.
                if ty != "Agent" {
                    let elem = elem_name(ty, name, &mut used);
                    builder.add_object(ty, elem.clone());
                    builder.add_object("Agent", elem.clone());
                    key_to_elem.insert(entity.key.clone(), elem);
                    emitted += 1;
                    continue;
                }
                "Agent"
            }
            "FlowType" | "Amount" | "Time" | "Account" | "StockType" | "TransactionPath"
            | "Text" | "Instrument" | "ContractTerms" => ty,
            _ => {
                // Not part of the Economy schema; keep in trace.
                continue;
            }
        };

        let elem = elem_name(object, name, &mut used);
        builder.add_object(object, elem.clone());
        key_to_elem.insert(entity.key.clone(), elem);
        emitted += 1;
    }

    // Relations
    for p in proposals {
        let ProposalV1::Relation {
            meta,
            rel_type,
            source,
            target,
            attributes,
            ..
        } = p
        else {
            continue;
        };

        let get = |raw: &str| -> Option<String> {
            let key = resolve_entity_ref(raw, resolved, fallback_by_name)?;
            key_to_elem.get(&key).cloned()
        };

        match rel_type.as_str() {
            "FlowInverse" => {
                let Some(flow) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "FlowInverse source not resolved".to_string(),
                    });
                    continue;
                };
                let Some(inverse) = get(target) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "FlowInverse target not resolved".to_string(),
                    });
                    continue;
                };
                builder.add_relation_tuple(
                    "FlowInverse",
                    tuple(&[("flow", &flow), ("inverse", &inverse)]),
                );
                emitted += 1;
            }
            "FlowCompose" => {
                // Expect attributes: result=<FlowType>
                let Some(f1) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "FlowCompose f1 not resolved".to_string(),
                    });
                    continue;
                };
                let Some(f2) = get(target) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "FlowCompose f2 not resolved".to_string(),
                    });
                    continue;
                };
                let Some(result_raw) = attributes.get("result") else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "FlowCompose missing attribute `result`".to_string(),
                    });
                    continue;
                };
                let Some(result) = get(result_raw) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "FlowCompose result not resolved".to_string(),
                    });
                    continue;
                };
                builder.add_relation_tuple(
                    "FlowCompose",
                    tuple(&[("f1", &f1), ("f2", &f2), ("result", &result)]),
                );
                emitted += 1;
            }
            "Flow" => {
                // Expect attributes: flowType, amount, time
                let Some(from) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow `from` not resolved".to_string(),
                    });
                    continue;
                };
                let Some(to) = get(target) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow `to` not resolved".to_string(),
                    });
                    continue;
                };
                let Some(flow_type_raw) = attributes.get("flowType") else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow missing attribute `flowType`".to_string(),
                    });
                    continue;
                };
                let Some(amount_raw) = attributes.get("amount") else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow missing attribute `amount`".to_string(),
                    });
                    continue;
                };
                let Some(time_raw) = attributes.get("time") else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow missing attribute `time`".to_string(),
                    });
                    continue;
                };
                let Some(flow_type) = get(flow_type_raw) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow `flowType` not resolved".to_string(),
                    });
                    continue;
                };
                let Some(amount) = get(amount_raw) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow `amount` not resolved".to_string(),
                    });
                    continue;
                };
                let Some(time) = get(time_raw) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::EconomicFlows,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "Flow `time` not resolved".to_string(),
                    });
                    continue;
                };

                builder.add_relation_tuple(
                    "Flow",
                    tuple(&[
                        ("from", &from),
                        ("to", &to),
                        ("flowType", &flow_type),
                        ("amount", &amount),
                        ("time", &time),
                    ]),
                );
                emitted += 1;
            }
            _ => {
                // Not mapped yet.
                unmapped.push(UnmappedProposalV1 {
                    domain: PromotionDomainV1::EconomicFlows,
                    proposal_id: meta.proposal_id.clone(),
                    kind: "relation".to_string(),
                    reason: format!("unmapped EconomicFlows relation `{rel_type}`"),
                });
            }
        }
    }

    if emitted == 0 {
        return Ok((String::new(), 0, unmapped));
    }

    let header_lines = vec![
        "-- GENERATED (candidate) — proposals → EconomicFlows".to_string(),
        "-- This file is NOT canonical. Promotion must be explicit.".to_string(),
        "-- To promote: merge the instance assignments below into `examples/economics/EconomicFlows.axi`.".to_string(),
        format!("-- Source: {} ({})", file.source.locator, file.source.source_type),
        format!("-- generated_at: {}", file.generated_at),
    ];
    Ok((builder.render(&header_lines), emitted, unmapped))
}

// =============================================================================
// SchemaEvolution (axi_schema_v1): emit an `instance ... of OntologyMeta:` patch
// =============================================================================

fn emit_schema_evolution(
    file: &ProposalsFileV1,
    proposals: &[ProposalV1],
    resolved: &ResolutionIndex,
    fallback_by_name: &BTreeMap<String, String>,
) -> Result<(String, usize, Vec<UnmappedProposalV1>)> {
    let mut builder = SchemaInstanceBuilder::new(
        PromotionDomainV1::SchemaEvolution.candidate_module_name(),
        "OntologyMeta",
        "ProposedEvolution",
    );

    let mut used: HashSet<String> = HashSet::new();
    let mut key_to_elem: HashMap<String, String> = HashMap::new();
    let mut emitted = 0usize;
    let mut unmapped: Vec<UnmappedProposalV1> = Vec::new();

    for entity in resolved.entities.values() {
        let ty = entity.entity_type.as_str();
        let obj = match ty {
            "Schema_" | "Version" | "Timestamp" | "Migration" | "EquivProof" | "Instance_"
            | "DataMigration" | "ChangeType" | "Text" => ty,
            _ => continue,
        };
        let elem = elem_name(obj, &entity.display_name, &mut used);
        builder.add_object(obj, elem.clone());
        key_to_elem.insert(entity.key.clone(), elem);
        emitted += 1;
    }

    for p in proposals {
        let ProposalV1::Relation {
            meta,
            rel_type,
            source,
            target,
            attributes,
            ..
        } = p
        else {
            continue;
        };

        let get = |raw: &str| -> Option<String> {
            let key = resolve_entity_ref(raw, resolved, fallback_by_name)?;
            key_to_elem.get(&key).cloned()
        };

        match rel_type.as_str() {
            "MigrationFrom" => {
                let Some(migration) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "MigrationFrom.migration not resolved".to_string(),
                    });
                    continue;
                };
                let Some(schema) = get(target) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "MigrationFrom.source not resolved".to_string(),
                    });
                    continue;
                };
                builder.add_relation_tuple(
                    "MigrationFrom",
                    tuple(&[("migration", &migration), ("source", &schema)]),
                );
                emitted += 1;
            }
            "MigrationTo" => {
                let Some(migration) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "MigrationTo.migration not resolved".to_string(),
                    });
                    continue;
                };
                let Some(schema) = get(target) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "MigrationTo.target not resolved".to_string(),
                    });
                    continue;
                };
                builder.add_relation_tuple(
                    "MigrationTo",
                    tuple(&[("migration", &migration), ("target", &schema)]),
                );
                emitted += 1;
            }
            "ChangeInverse" => {
                let Some(change) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "ChangeInverse.change not resolved".to_string(),
                    });
                    continue;
                };
                let Some(inverse) = get(target) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "ChangeInverse.inverse not resolved".to_string(),
                    });
                    continue;
                };
                builder.add_relation_tuple(
                    "ChangeInverse",
                    tuple(&[("change", &change), ("inverse", &inverse)]),
                );
                emitted += 1;
            }
            "SchemaVersion" => {
                // Expect attributes: version, timestamp
                let Some(schema) = get(source) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "SchemaVersion.schema not resolved".to_string(),
                    });
                    continue;
                };
                let Some(version_raw) = attributes.get("version") else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "SchemaVersion missing attribute `version`".to_string(),
                    });
                    continue;
                };
                let Some(timestamp_raw) = attributes.get("timestamp") else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "SchemaVersion missing attribute `timestamp`".to_string(),
                    });
                    continue;
                };
                let Some(version) = get(version_raw) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "SchemaVersion.version not resolved".to_string(),
                    });
                    continue;
                };
                let Some(timestamp) = get(timestamp_raw) else {
                    unmapped.push(UnmappedProposalV1 {
                        domain: PromotionDomainV1::SchemaEvolution,
                        proposal_id: meta.proposal_id.clone(),
                        kind: "relation".to_string(),
                        reason: "SchemaVersion.timestamp not resolved".to_string(),
                    });
                    continue;
                };
                builder.add_relation_tuple(
                    "SchemaVersion",
                    tuple(&[
                        ("schema", &schema),
                        ("version", &version),
                        ("timestamp", &timestamp),
                    ]),
                );
                emitted += 1;
            }
            _ => unmapped.push(UnmappedProposalV1 {
                domain: PromotionDomainV1::SchemaEvolution,
                proposal_id: meta.proposal_id.clone(),
                kind: "relation".to_string(),
                reason: format!("unmapped SchemaEvolution relation `{rel_type}`"),
            }),
        }
    }

    if emitted == 0 {
        return Ok((String::new(), 0, unmapped));
    }

    let header_lines = vec![
        "-- GENERATED (candidate) — proposals → SchemaEvolution".to_string(),
        "-- This file is NOT canonical. Promotion must be explicit.".to_string(),
        "-- To promote: merge the instance assignments below into `examples/ontology/SchemaEvolution.axi`.".to_string(),
        format!("-- Source: {} ({})", file.source.locator, file.source.source_type),
        format!("-- generated_at: {}", file.generated_at),
    ];
    Ok((builder.render(&header_lines), emitted, unmapped))
}
