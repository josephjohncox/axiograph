//! Grounding Engine: Build context from KG for LLM generation

use crate::{
    AcceptedGroundedFact, AcceptedGroundingContext, AcceptedGroundingProvenanceV1, GroundedFact,
    GroundingContext, GroundingProvenanceV1, GuardrailContext, SchemaContext,
};
use anyhow::{anyhow, Result};
use axiograph_kernel::ObjectBlobIdV2;
use axiograph_pathdb::{materialization::MaterializedPathDb, PathDB, Relation, StrId};
use std::collections::{BTreeSet, HashSet};

pub const MAX_GROUNDING_FACTS: usize = 256;
pub const MAX_GROUNDING_QUERY_BYTES: usize = 16 * 1024;
pub const MAX_GROUNDING_KEYWORDS: usize = 64;
pub const MAX_GROUNDING_MATCH_VISITS: usize = 16 * 1024;
pub const MAX_GROUNDING_RELATED_EDGE_VISITS: usize = 1024;
pub const MAX_GROUNDING_GUARDRAIL_VISITS: usize = 4096;
pub const MAX_GROUNDING_SCHEMA_ENTITY_TYPES: usize = 512;
pub const MAX_GROUNDING_SCHEMA_RELATION_TYPES: usize = 512;
pub const MAX_GROUNDING_SCHEMA_CONSTRAINTS: usize = 512;
pub const MAX_GROUNDING_OUTPUT_BYTES: usize = 256 * 1024;
pub const MAX_GROUNDING_ATTRIBUTES_PER_ENTITY: usize = 128;
const MAX_GROUNDING_KEYWORD_CHARS: usize = 64;

const ATTRIBUTE_LIMIT: &str = "attribute_limit";
const OUTPUT_BYTE_LIMIT: &str = "output_byte_limit";
const KEYWORD_LENGTH_LIMIT: &str = "keyword_length_limit";
const KEYWORD_LIMIT: &str = "keyword_limit";
const TYPE_NAME_LIMIT: &str = "type_name_limit";
const ENTITY_MATCH_VISIT_LIMIT: &str = "entity_match_visit_limit";
const FACT_LIMIT: &str = "fact_limit";
const RELATED_EDGE_VISIT_LIMIT: &str = "related_edge_visit_limit";
const RELATED_OUTPUT_LIMIT: &str = "related_output_limit";
const GUARDRAIL_VISIT_LIMIT: &str = "guardrail_visit_limit";
const GUARDRAIL_OUTPUT_LIMIT: &str = "guardrail_output_limit";
const SCHEMA_ENTITY_TYPE_LIMIT: &str = "schema_entity_type_limit";
const SCHEMA_RELATION_TYPE_LIMIT: &str = "schema_relation_type_limit";
const SCHEMA_CONSTRAINT_LIMIT: &str = "schema_constraint_limit";
const GROUNDING_SEARCH_ATTRIBUTES: &[&str] = &[
    "name",
    "label",
    "text",
    "description",
    "axiograph.value",
    "axiograph.entity_key",
    "axi_fact_id",
];

#[derive(Debug, Default)]
struct GroundingWork {
    match_visits: usize,
    related_edge_visits: usize,
    guardrail_visits: usize,
    output_bytes: usize,
    truncation_reasons: BTreeSet<&'static str>,
}

impl GroundingWork {
    fn truncate(&mut self, reason: &'static str) {
        self.truncation_reasons.insert(reason);
    }

    fn charge(&mut self, bytes: usize) -> bool {
        let Some(next) = self.output_bytes.checked_add(bytes) else {
            self.truncate(OUTPUT_BYTE_LIMIT);
            return false;
        };
        if next > MAX_GROUNDING_OUTPUT_BYTES {
            self.truncate(OUTPUT_BYTE_LIMIT);
            return false;
        }
        self.output_bytes = next;
        true
    }

    fn charge_str(&mut self, value: &str) -> Option<String> {
        self.charge(value.len()).then(|| value.to_string())
    }

    fn exhausted_output(&self) -> bool {
        self.truncation_reasons.contains(OUTPUT_BYTE_LIMIT)
    }

    fn reasons(&self) -> Vec<String> {
        self.truncation_reasons
            .iter()
            .map(|reason| (*reason).to_string())
            .collect()
    }
}

fn clone_interned_charged(pathdb: &PathDB, id: StrId, work: &mut GroundingWork) -> Option<String> {
    pathdb
        .interner
        .with_lookup(id, |value| work.charge_str(value))
        .flatten()
}

fn entity_attr_id(pathdb: &PathDB, entity_id: u32, key: &str) -> Option<StrId> {
    let key_id = pathdb.interner.id_of(key)?;
    pathdb.entities.get_attr(entity_id, key_id)
}

fn preferred_label_id(pathdb: &PathDB, entity_id: u32) -> Option<StrId> {
    ["name", "label", "axiograph.value"]
        .into_iter()
        .find_map(|key| entity_attr_id(pathdb, entity_id, key))
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[derive(Clone, Copy)]
enum RelatedDirection {
    Outgoing,
    Incoming,
}

fn visit_bounded_adjacency(
    pathdb: &PathDB,
    entity_id: u32,
    work: &mut GroundingWork,
    mut visit: impl FnMut(&Relation, RelatedDirection, &mut GroundingWork) -> bool,
) {
    let outgoing_len = pathdb.relations.outgoing_any_len(entity_id);
    let outgoing_limit = MAX_GROUNDING_RELATED_EDGE_VISITS
        .saturating_sub(work.related_edge_visits)
        .min(outgoing_len);
    for relation in pathdb
        .relations
        .outgoing_any_iter(entity_id)
        .take(outgoing_limit)
    {
        work.related_edge_visits += 1;
        if !visit(relation, RelatedDirection::Outgoing, work) {
            return;
        }
    }
    if outgoing_limit < outgoing_len {
        work.truncate(RELATED_EDGE_VISIT_LIMIT);
    }

    let incoming_len = pathdb.relations.incoming_any_len(entity_id);
    let incoming_limit = MAX_GROUNDING_RELATED_EDGE_VISITS
        .saturating_sub(work.related_edge_visits)
        .min(incoming_len);
    for relation in pathdb
        .relations
        .incoming_any_iter(entity_id)
        .take(incoming_limit)
    {
        work.related_edge_visits += 1;
        if !visit(relation, RelatedDirection::Incoming, work) {
            return;
        }
    }
    if incoming_limit < incoming_len {
        work.truncate(RELATED_EDGE_VISIT_LIMIT);
    }
}

enum GroundingSchemaSource<'a> {
    Runtime,
    Canonical {
        entity_types: &'a [String],
        relation_types: &'a [String],
        constraints: &'a [String],
    },
}

fn append_schema_strings(
    source: &[String],
    limit: usize,
    truncation_reason: &'static str,
    output: &mut Vec<String>,
    work: &mut GroundingWork,
) {
    if source.len() > limit {
        work.truncate(truncation_reason);
    }
    for value in source.iter().take(limit) {
        let Some(value) = work.charge_str(value) else {
            break;
        };
        output.push(value);
    }
}

/// Internal implementation shared by the evidence and accepted-derived
/// grounding interfaces.
struct GroundingEngine<'a> {
    pathdb: &'a PathDB,
    max_facts: usize,
}

impl<'a> GroundingEngine<'a> {
    fn new(pathdb: &'a PathDB, max_facts: usize) -> Self {
        Self { pathdb, max_facts }
    }

    fn build_context(
        &self,
        query: &str,
        schema_source: GroundingSchemaSource<'_>,
    ) -> GroundingContext {
        let mut work = GroundingWork::default();
        let keywords = self.extract_keywords(query, &mut work);
        let facts = self.retrieve_relevant_facts(&keywords, &mut work);
        let schema_context = self.build_schema_context(schema_source, &mut work);
        let active_guardrails = self.get_applicable_guardrails(&keywords, &mut work);
        let suggestions = self.generate_suggestions(&facts, &mut work);
        let truncation_reasons = work.reasons();

        GroundingContext {
            provenance: GroundingProvenanceV1::evidence("pathdb_process_local_evidence"),
            facts,
            schema_context: Some(schema_context),
            active_guardrails,
            suggested_queries: suggestions,
            truncated: !truncation_reasons.is_empty(),
            truncation_reasons,
        }
    }

    /// Extract deterministic, deduplicated keywords within the request budget.
    fn extract_keywords(&self, query: &str, work: &mut GroundingWork) -> Vec<String> {
        let stopwords: HashSet<&str> = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "must",
            "shall", "can", "need", "dare", "what", "when", "where", "which", "who", "whom",
            "whose", "why", "how", "this", "that", "these", "those", "i", "you", "he", "she", "it",
            "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "its", "our",
            "their", "and", "or", "but", "if", "then", "than", "so", "as", "for", "with", "about",
            "to", "from", "in", "on", "at", "by", "of", "up", "out", "into", "onto",
        ]
        .into_iter()
        .collect();

        let mut seen = HashSet::new();
        let mut keywords = Vec::new();
        for word in query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() > 2 && !stopwords.contains(*word))
        {
            if word.chars().count() > MAX_GROUNDING_KEYWORD_CHARS {
                work.truncate(KEYWORD_LENGTH_LIMIT);
            }
            let normalized = word
                .chars()
                .take(MAX_GROUNDING_KEYWORD_CHARS)
                .collect::<String>();
            if !seen.insert(normalized.clone()) {
                continue;
            }
            if keywords.len() == MAX_GROUNDING_KEYWORDS {
                work.truncate(KEYWORD_LIMIT);
                break;
            }
            keywords.push(normalized);
        }
        keywords
    }

    fn retrieve_relevant_entity_ids(
        &self,
        keywords: &[String],
        work: &mut GroundingWork,
    ) -> Vec<u32> {
        let candidate_limit = self.max_facts.saturating_add(1);
        let mut matches = BTreeSet::new();
        let type_ids = self.pathdb.entity_type_ids();
        if type_ids.len() > MAX_GROUNDING_SCHEMA_ENTITY_TYPES {
            work.truncate(TYPE_NAME_LIMIT);
        }

        'types: for type_id in type_ids.iter().take(MAX_GROUNDING_SCHEMA_ENTITY_TYPES) {
            let matches_keyword = self
                .pathdb
                .interner
                .with_lookup(*type_id, |type_name| {
                    keywords
                        .iter()
                        .any(|keyword| contains_ascii_case_insensitive(type_name, keyword.as_str()))
                })
                .unwrap_or(false);
            if !matches_keyword {
                continue;
            }
            if let Some(ids) = self.pathdb.find_by_type_id(*type_id) {
                for id in ids.iter() {
                    if work.match_visits == MAX_GROUNDING_MATCH_VISITS {
                        work.truncate(ENTITY_MATCH_VISIT_LIMIT);
                        break 'types;
                    }
                    work.match_visits += 1;
                    matches.insert(id);
                    if matches.len() == candidate_limit {
                        work.truncate(FACT_LIMIT);
                        break 'types;
                    }
                }
            }
        }

        'attributes: for keyword in keywords {
            for attribute in GROUNDING_SEARCH_ATTRIBUTES {
                if matches.len() == candidate_limit {
                    break 'attributes;
                }
                let remaining_visits = MAX_GROUNDING_MATCH_VISITS.saturating_sub(work.match_visits);
                if remaining_visits == 0 {
                    work.truncate(ENTITY_MATCH_VISIT_LIMIT);
                    break 'attributes;
                }
                let (ids, visits, truncated) = self.pathdb.entities_with_attr_fts_any_bounded(
                    attribute,
                    keyword,
                    remaining_visits,
                );
                work.match_visits += visits;
                for id in ids {
                    matches.insert(id);
                    if matches.len() == candidate_limit {
                        work.truncate(FACT_LIMIT);
                        break 'attributes;
                    }
                }
                if truncated {
                    work.truncate(ENTITY_MATCH_VISIT_LIMIT);
                }
            }
        }

        if matches.len() > self.max_facts {
            work.truncate(FACT_LIMIT);
        }
        matches.into_iter().take(self.max_facts).collect()
    }

    /// Retrieve facts relevant to keywords.
    fn retrieve_relevant_facts(
        &self,
        keywords: &[String],
        work: &mut GroundingWork,
    ) -> Vec<GroundedFact> {
        let mut facts = Vec::new();
        for id in self.retrieve_relevant_entity_ids(keywords, work) {
            if work.exhausted_output() {
                break;
            }
            let Some(natural) = self.entity_to_natural(id, work) else {
                break;
            };
            let Some(type_id) = self.pathdb.entities.get_type(id) else {
                continue;
            };
            let Some(entity_type) = clone_interned_charged(self.pathdb, type_id, work) else {
                break;
            };
            let structured = format!("Entity(id={id}, type={entity_type})");
            if !work.charge(structured.len().saturating_sub(entity_type.len())) {
                break;
            }
            let citation = format!("PathDB:Entity:{id}");
            if !work.charge(citation.len()) {
                break;
            }
            let related = self.get_related_concepts(id, work);
            facts.push(GroundedFact {
                id,
                natural,
                structured,
                confidence: 1.0,
                citation: vec![citation],
                related,
            });
        }
        facts
    }

    fn entity_to_natural(&self, entity_id: u32, work: &mut GroundingWork) -> Option<String> {
        let type_id = self.pathdb.entities.get_type(entity_id)?;
        let entity_type = clone_interned_charged(self.pathdb, type_id, work)?;
        let name = match preferred_label_id(self.pathdb, entity_id) {
            Some(name_id) => clone_interned_charged(self.pathdb, name_id, work)?,
            None => work.charge_str("entity")?,
        };

        let excluded = ["name", "label", "axiograph.value"]
            .into_iter()
            .filter_map(|key| self.pathdb.interner.id_of(key))
            .collect::<HashSet<_>>();
        let (attr_ids, attrs_truncated) = self
            .pathdb
            .entity_attr_ids_bounded(entity_id, MAX_GROUNDING_ATTRIBUTES_PER_ENTITY);
        if attrs_truncated {
            work.truncate(ATTRIBUTE_LIMIT);
        }
        let mut attrs = Vec::new();
        for (key_id, value_id) in attr_ids {
            if excluded.contains(&key_id) {
                continue;
            }
            let key = clone_interned_charged(self.pathdb, key_id, work)?;
            let value = clone_interned_charged(self.pathdb, value_id, work)?;
            if !work.charge(2) {
                return None;
            }
            attrs.push((key, value));
        }
        attrs.sort_unstable();

        let fixed_bytes = if attrs.is_empty() {
            " is a ".len()
        } else {
            " is a ".len() + " with ".len() + attrs.len().saturating_sub(1) * ", ".len()
        };
        if !work.charge(fixed_bytes) {
            return None;
        }
        if attrs.is_empty() {
            Some(format!("{name} is a {entity_type}"))
        } else {
            let attrs = attrs
                .into_iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("{name} is a {entity_type} with {attrs}"))
        }
    }

    fn get_related_concepts(&self, entity_id: u32, work: &mut GroundingWork) -> Vec<String> {
        let mut related = BTreeSet::new();
        visit_bounded_adjacency(self.pathdb, entity_id, work, |relation, direction, work| {
            let Some(relation_type) = clone_interned_charged(self.pathdb, relation.rel_type, work)
            else {
                return false;
            };
            let endpoint = match direction {
                RelatedDirection::Outgoing => relation.target,
                RelatedDirection::Incoming => relation.source,
            };
            let Some(label) = self.entity_reference_label(endpoint, work) else {
                return false;
            };
            let fixed = match direction {
                RelatedDirection::Outgoing => "->".len(),
                RelatedDirection::Incoming => "<-".len() + "-".len(),
            };
            if !work.charge(fixed) {
                return false;
            }
            related.insert(match direction {
                RelatedDirection::Outgoing => format!("{relation_type}->{label}"),
                RelatedDirection::Incoming => format!("<-{relation_type}-{label}"),
            });
            true
        });
        if related.len() > self.max_facts {
            work.truncate(RELATED_OUTPUT_LIMIT);
        }
        related.into_iter().take(self.max_facts).collect()
    }

    fn entity_reference_label(&self, entity_id: u32, work: &mut GroundingWork) -> Option<String> {
        if let Some(label_id) = preferred_label_id(self.pathdb, entity_id) {
            return clone_interned_charged(self.pathdb, label_id, work);
        }
        let fallback = format!("entity:{entity_id}");
        work.charge(fallback.len()).then_some(fallback)
    }

    fn build_schema_context(
        &self,
        source: GroundingSchemaSource<'_>,
        work: &mut GroundingWork,
    ) -> SchemaContext {
        if work.exhausted_output() {
            return SchemaContext {
                entity_types: Vec::new(),
                relation_types: Vec::new(),
                constraints: Vec::new(),
            };
        }
        let mut entity_types = Vec::new();
        let mut relation_types = Vec::new();
        let mut constraints = Vec::new();
        match source {
            GroundingSchemaSource::Runtime => {
                let entity_type_ids = self.pathdb.entity_type_ids();
                if entity_type_ids.len() > MAX_GROUNDING_SCHEMA_ENTITY_TYPES {
                    work.truncate(SCHEMA_ENTITY_TYPE_LIMIT);
                }
                for id in entity_type_ids
                    .into_iter()
                    .take(MAX_GROUNDING_SCHEMA_ENTITY_TYPES)
                {
                    let Some(name) = clone_interned_charged(self.pathdb, id, work) else {
                        break;
                    };
                    entity_types.push(name);
                }
                entity_types.sort_unstable();

                let relation_type_ids = self.pathdb.relation_type_ids();
                if relation_type_ids.len() > MAX_GROUNDING_SCHEMA_RELATION_TYPES {
                    work.truncate(SCHEMA_RELATION_TYPE_LIMIT);
                }
                for id in relation_type_ids
                    .into_iter()
                    .take(MAX_GROUNDING_SCHEMA_RELATION_TYPES)
                {
                    let Some(name) = clone_interned_charged(self.pathdb, id, work) else {
                        break;
                    };
                    relation_types.push(name);
                }
                relation_types.sort_unstable();
            }
            GroundingSchemaSource::Canonical {
                entity_types: source_entity_types,
                relation_types: source_relation_types,
                constraints: source_constraints,
            } => {
                append_schema_strings(
                    source_entity_types,
                    MAX_GROUNDING_SCHEMA_ENTITY_TYPES,
                    SCHEMA_ENTITY_TYPE_LIMIT,
                    &mut entity_types,
                    work,
                );
                append_schema_strings(
                    source_relation_types,
                    MAX_GROUNDING_SCHEMA_RELATION_TYPES,
                    SCHEMA_RELATION_TYPE_LIMIT,
                    &mut relation_types,
                    work,
                );
                append_schema_strings(
                    source_constraints,
                    MAX_GROUNDING_SCHEMA_CONSTRAINTS,
                    SCHEMA_CONSTRAINT_LIMIT,
                    &mut constraints,
                    work,
                );
            }
        }
        SchemaContext {
            entity_types,
            relation_types,
            constraints,
        }
    }

    fn get_applicable_guardrails(
        &self,
        keywords: &[String],
        work: &mut GroundingWork,
    ) -> Vec<GuardrailContext> {
        if work.exhausted_output() {
            return Vec::new();
        }
        let Some(ids) = self.pathdb.find_by_type("Guardrail") else {
            return Vec::new();
        };
        let mut guardrails = Vec::new();
        for id in ids.iter().take(MAX_GROUNDING_GUARDRAIL_VISITS) {
            work.guardrail_visits += 1;
            let (attrs, attrs_truncated) = self
                .pathdb
                .entity_attr_ids_bounded(id, MAX_GROUNDING_ATTRIBUTES_PER_ENTITY);
            if attrs_truncated {
                work.truncate(ATTRIBUTE_LIMIT);
            }
            let matches = attrs.iter().any(|(_, value_id)| {
                self.pathdb
                    .interner
                    .with_lookup(*value_id, |value| {
                        keywords
                            .iter()
                            .any(|keyword| contains_ascii_case_insensitive(value, keyword.as_str()))
                    })
                    .unwrap_or(false)
            });
            if !matches {
                continue;
            }
            if guardrails.len() == self.max_facts {
                work.truncate(GUARDRAIL_OUTPUT_LIMIT);
                break;
            }

            let Some(rule_id) = self.clone_attr_or_fallback(
                id,
                &["rule_id", "name"],
                format!("PathDB:Guardrail:{id}"),
                work,
            ) else {
                break;
            };
            let Some(severity) =
                self.clone_attr_or_fallback(id, &["severity"], "unspecified".to_string(), work)
            else {
                break;
            };
            let Some(description) = self.clone_attr_or_fallback(
                id,
                &["description", "rule"],
                format!("stored Guardrail entity {id}"),
                work,
            ) else {
                break;
            };
            let Some(applies_when) =
                self.clone_attr_or_fallback(id, &["applies_when"], String::new(), work)
            else {
                break;
            };
            guardrails.push(GuardrailContext {
                rule_id,
                severity,
                description,
                applies_when,
            });
        }
        if work.guardrail_visits == MAX_GROUNDING_GUARDRAIL_VISITS
            && ids.len() > work.guardrail_visits as u64
        {
            work.truncate(GUARDRAIL_VISIT_LIMIT);
        }
        guardrails
    }

    fn clone_attr_or_fallback(
        &self,
        entity_id: u32,
        keys: &[&str],
        fallback: String,
        work: &mut GroundingWork,
    ) -> Option<String> {
        if let Some(value_id) = keys
            .iter()
            .find_map(|key| entity_attr_id(self.pathdb, entity_id, key))
        {
            clone_interned_charged(self.pathdb, value_id, work)
        } else {
            work.charge(fallback.len()).then_some(fallback)
        }
    }

    fn generate_suggestions(
        &self,
        facts: &[GroundedFact],
        work: &mut GroundingWork,
    ) -> Vec<String> {
        if work.exhausted_output() {
            return Vec::new();
        }
        let mut candidates = Vec::new();
        if !facts.is_empty() {
            candidates.push(format!(
                "What are the relationships between these {} concepts?",
                facts.len()
            ));
        }
        candidates.extend([
            "What constraints apply to this domain?".to_string(),
            "Are there any safety considerations?".to_string(),
            "What are the best practices?".to_string(),
        ]);

        let mut suggestions = Vec::new();
        for candidate in candidates.into_iter().take(5) {
            if !work.charge(candidate.len()) {
                break;
            }
            suggestions.push(candidate);
        }
        suggestions
    }
}

fn validate_grounding_request(query: &str, max_facts: usize) -> Result<()> {
    if query.len() > MAX_GROUNDING_QUERY_BYTES {
        return Err(anyhow!(
            "grounding query bytes {} exceed {MAX_GROUNDING_QUERY_BYTES}",
            query.len()
        ));
    }
    if !(1..=MAX_GROUNDING_FACTS).contains(&max_facts) {
        return Err(anyhow!(
            "grounding fact limit must be in 1..={MAX_GROUNDING_FACTS}, got {max_facts}"
        ));
    }
    Ok(())
}

/// Build explicitly non-authoritative grounding from process-local PathDB
/// evidence. Limits are checked before retrieval.
pub fn evidence_grounding_context(
    pathdb: &PathDB,
    query: &str,
    max_facts: usize,
) -> Result<GroundingContext> {
    validate_grounding_request(query, max_facts)?;
    Ok(
        GroundingEngine::new(pathdb, max_facts)
            .build_context(query, GroundingSchemaSource::Runtime),
    )
}

pub(crate) fn evidence_grounding_context_with_schema(
    pathdb: &PathDB,
    query: &str,
    max_facts: usize,
    entity_types: &[String],
    relation_types: &[String],
    constraints: &[String],
) -> Result<GroundingContext> {
    validate_grounding_request(query, max_facts)?;
    Ok(GroundingEngine::new(pathdb, max_facts).build_context(
        query,
        GroundingSchemaSource::Canonical {
            entity_types,
            relation_types,
            constraints,
        },
    ))
}

fn accepted_runtime_stable_id(pathdb: &PathDB, entity_id: u32) -> Option<StrId> {
    entity_attr_id(pathdb, entity_id, "axiograph.entity_key")
        .or_else(|| entity_attr_id(pathdb, entity_id, "axi_fact_id"))
}

fn accepted_related_stable_ids(
    pathdb: &PathDB,
    entity_id: u32,
    limit: usize,
    work: &mut GroundingWork,
) -> Vec<String> {
    let mut related = BTreeSet::new();
    visit_bounded_adjacency(pathdb, entity_id, work, |relation, direction, work| {
        let endpoint = match direction {
            RelatedDirection::Outgoing => relation.target,
            RelatedDirection::Incoming => relation.source,
        };
        let Some(stable_id) = accepted_runtime_stable_id(pathdb, endpoint) else {
            return true;
        };
        let Some(stable_id) = clone_interned_charged(pathdb, stable_id, work) else {
            return false;
        };
        related.insert(stable_id);
        true
    });
    if related.len() > limit {
        work.truncate(RELATED_OUTPUT_LIMIT);
    }
    related.into_iter().take(limit).collect()
}

fn render_accepted_fact(
    engine: &GroundingEngine<'_>,
    entity_id: u32,
    materialization_id: &str,
    work: &mut GroundingWork,
) -> Result<Option<AcceptedGroundedFact>> {
    let stable_id_id = accepted_runtime_stable_id(engine.pathdb, entity_id).ok_or_else(|| {
        anyhow!("authenticated runtime entity {entity_id} has no stable entity or fact identity")
    })?;
    let Some(stable_id) = clone_interned_charged(engine.pathdb, stable_id_id, work) else {
        return Ok(None);
    };
    let Some(natural) = engine.entity_to_natural(entity_id, work) else {
        return Ok(None);
    };
    let type_id = engine
        .pathdb
        .entities
        .get_type(entity_id)
        .ok_or_else(|| anyhow!("grounding selected absent runtime entity {entity_id}"))?;
    let Some(entity_type) = clone_interned_charged(engine.pathdb, type_id, work) else {
        return Ok(None);
    };
    if !work.charge(stable_id.len()) {
        return Ok(None);
    }
    let structured = format!("AcceptedEntity(stable_id={stable_id}, type={entity_type})");
    if !work.charge(
        structured
            .len()
            .saturating_sub(stable_id.len())
            .saturating_sub(entity_type.len()),
    ) {
        return Ok(None);
    }
    let citation = format!("AxiStore:Materialization:{materialization_id}:Entity:{stable_id}");
    if !work.charge(citation.len()) {
        return Ok(None);
    }
    let related = accepted_related_stable_ids(engine.pathdb, entity_id, engine.max_facts, work);
    Ok(Some(AcceptedGroundedFact::new(
        stable_id,
        natural,
        structured,
        vec![citation],
        related,
    )))
}

/// Build accepted-derived grounding exclusively from an authenticated AxiStore
/// materialization. There is no constructor from a bare `PathDB` or receipt.
pub fn accepted_grounding_context(
    materialized: &MaterializedPathDb,
    query: &str,
    max_facts: usize,
) -> Result<AcceptedGroundingContext> {
    validate_grounding_request(query, max_facts)?;
    let engine = GroundingEngine::new(materialized.db(), max_facts);
    let mut work = GroundingWork::default();
    let keywords = engine.extract_keywords(query, &mut work);
    let entity_ids = engine.retrieve_relevant_entity_ids(&keywords, &mut work);
    let materialization_id = materialized.receipt().materialization_id.to_string();
    let mut facts = Vec::new();
    for id in entity_ids {
        match render_accepted_fact(&engine, id, &materialization_id, &mut work)? {
            Some(fact) => facts.push(fact),
            None => break,
        }
    }
    let schema_context = engine.build_schema_context(GroundingSchemaSource::Runtime, &mut work);
    let truncation_reasons = work.reasons();
    let truncated = !truncation_reasons.is_empty();
    let limit_bytes = (max_facts as u64).to_be_bytes();
    let query_digest = ObjectBlobIdV2::from_canonical_fields(&[
        b"axiograph_accepted_grounding_query_v1",
        query.as_bytes(),
        &limit_bytes,
    ]);
    let truncated_bytes = [u8::from(truncated)];
    let mut selection_fields = Vec::with_capacity(facts.len() + 4);
    selection_fields.push(b"axiograph_accepted_grounding_selection_v1".as_slice());
    selection_fields.push(query_digest.as_str().as_bytes());
    selection_fields.push(materialization_id.as_bytes());
    selection_fields.push(truncated_bytes.as_slice());
    selection_fields.extend(facts.iter().map(|fact| fact.stable_id().as_bytes()));
    let selection_digest = ObjectBlobIdV2::from_canonical_fields(&selection_fields);
    let provenance = AcceptedGroundingProvenanceV1::from_materialized_pathdb(
        materialized,
        &query_digest,
        &selection_digest,
    );
    Ok(AcceptedGroundingContext::new(
        provenance,
        facts,
        schema_context,
        truncation_reasons,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_extraction_omits_stopwords() {
        let pathdb = PathDB::new();
        let engine = GroundingEngine::new(&pathdb, 20);

        let mut work = GroundingWork::default();
        let keywords = engine.extract_keywords("What is the hardness of titanium?", &mut work);
        assert!(keywords.contains(&"hardness".to_string()));
        assert!(keywords.contains(&"titanium".to_string()));
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"is".to_string()));
    }

    #[test]
    fn evidence_grounding_is_bounded_and_uses_runtime_types() {
        let mut pathdb = PathDB::new();
        let alice = pathdb.add_entity("Person", vec![("name", "Alice")]);
        let team = pathdb.add_entity("Team", vec![("name", "Safety")]);
        pathdb.add_relation("memberOf", alice, team, 1.0, Vec::new());

        let context = evidence_grounding_context(&pathdb, "Alice", 10)
            .expect("bounded evidence grounding should build");
        assert_eq!(context.facts.len(), 1);
        assert_eq!(context.facts[0].related, vec!["memberOf->Safety"]);
        let schema = context
            .schema_context
            .as_ref()
            .expect("runtime schema summary");
        assert_eq!(schema.entity_types, vec!["Person", "Team"]);
        assert_eq!(schema.relation_types, vec!["memberOf"]);
        assert_eq!(context.provenance.plane, crate::GroundingPlaneV1::Evidence);

        let mut wire = serde_json::to_value(&context).expect("serialize grounding context");
        wire.as_object_mut()
            .expect("context object")
            .insert("accepted".to_string(), serde_json::Value::Bool(true));
        assert!(serde_json::from_value::<GroundingContext>(wire).is_err());

        assert!(evidence_grounding_context(&pathdb, "Alice", 0).is_err());
        assert!(evidence_grounding_context(&pathdb, "Alice", MAX_GROUNDING_FACTS + 1).is_err());
        assert!(
            evidence_grounding_context(&pathdb, &"q".repeat(MAX_GROUNDING_QUERY_BYTES + 1), 1,)
                .is_err()
        );
    }

    #[test]
    fn grounding_work_budget() {
        let mut pathdb = PathDB::new();
        let selected = pathdb.add_entity("Node", vec![("name", "Selected")]);
        for ordinal in 0..1_100 {
            let target = pathdb.add_entity("Neighbor", vec![]);
            pathdb.add_relation(
                "related",
                selected,
                target,
                1.0,
                vec![("ordinal", &ordinal.to_string())],
            );
        }
        for _ in 0..4_096 {
            let source = pathdb.add_entity("Unrelated", vec![]);
            let target = pathdb.add_entity("Unrelated", vec![]);
            pathdb.add_relation("unrelated", source, target, 1.0, vec![]);
        }

        let engine = GroundingEngine::new(&pathdb, 8);
        let mut work = GroundingWork::default();
        let keywords = engine.extract_keywords("Selected", &mut work);
        let facts = engine.retrieve_relevant_facts(&keywords, &mut work);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].id, selected);
        assert_eq!(work.related_edge_visits, MAX_GROUNDING_RELATED_EDGE_VISITS);
        assert!(work.truncation_reasons.contains(RELATED_EDGE_VISIT_LIMIT));
        assert!(work.truncation_reasons.contains(RELATED_OUTPUT_LIMIT));

        let query = (0..=MAX_GROUNDING_KEYWORDS)
            .map(|ordinal| format!("keyword{ordinal}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut keyword_work = GroundingWork::default();
        let keywords = engine.extract_keywords(&query, &mut keyword_work);
        assert_eq!(keywords.len(), MAX_GROUNDING_KEYWORDS);
        assert!(keyword_work.truncation_reasons.contains(KEYWORD_LIMIT));
    }

    #[test]
    fn rendered_output_bytes_are_request_bounded() {
        let mut oversized = PathDB::new();
        let huge = "x".repeat(MAX_GROUNDING_OUTPUT_BYTES + 1);
        oversized.add_entity(
            "Node",
            vec![("name", "Selected"), ("description", huge.as_str())],
        );
        let context = evidence_grounding_context(&oversized, "Selected", 8).unwrap();
        assert!(context.facts.is_empty());
        assert!(context
            .truncation_reasons
            .iter()
            .any(|reason| reason == OUTPUT_BYTE_LIMIT));

        let mut related = PathDB::new();
        let selected = related.add_entity("Node", vec![("name", "Selected")]);
        let large_label = "l".repeat(16 * 1024);
        for _ in 0..64 {
            let target = related.add_entity("Node", vec![("name", large_label.as_str())]);
            related.add_relation("related", selected, target, 1.0, vec![]);
        }
        let context = evidence_grounding_context(&related, "Selected", 64).unwrap();
        assert_eq!(context.facts.len(), 1);
        assert!(context.facts[0].related.len() < 64);
        assert!(context
            .truncation_reasons
            .iter()
            .any(|reason| reason == OUTPUT_BYTE_LIMIT));
        let rendered_bytes = context
            .facts
            .iter()
            .map(|fact| {
                fact.natural.len()
                    + fact.structured.len()
                    + fact.citation.iter().map(String::len).sum::<usize>()
                    + fact.related.iter().map(String::len).sum::<usize>()
            })
            .sum::<usize>()
            + context
                .schema_context
                .iter()
                .flat_map(|schema| {
                    schema
                        .entity_types
                        .iter()
                        .chain(schema.relation_types.iter())
                        .chain(schema.constraints.iter())
                })
                .map(String::len)
                .sum::<usize>()
            + context
                .active_guardrails
                .iter()
                .map(|guardrail| {
                    guardrail.rule_id.len()
                        + guardrail.severity.len()
                        + guardrail.description.len()
                        + guardrail.applies_when.len()
                })
                .sum::<usize>()
            + context
                .suggested_queries
                .iter()
                .map(String::len)
                .sum::<usize>();
        assert!(rendered_bytes <= MAX_GROUNDING_OUTPUT_BYTES);
    }

    #[test]
    fn canonical_schema_does_not_report_discarded_runtime_schema_truncation() {
        let mut pathdb = PathDB::new();
        for ordinal in 0..=MAX_GROUNDING_SCHEMA_ENTITY_TYPES {
            pathdb.add_entity(&format!("RuntimeType{ordinal}"), vec![]);
        }
        let context = evidence_grounding_context_with_schema(
            &pathdb,
            "absent",
            8,
            &["CanonicalType".to_string()],
            &["canonicalRelation".to_string()],
            &["canonical constraint".to_string()],
        )
        .unwrap();
        let schema = context.schema_context.unwrap();
        assert_eq!(schema.entity_types, vec!["CanonicalType"]);
        assert_eq!(schema.relation_types, vec!["canonicalRelation"]);
        assert_eq!(schema.constraints, vec!["canonical constraint"]);
        assert!(!context
            .truncation_reasons
            .iter()
            .any(|reason| reason == SCHEMA_ENTITY_TYPE_LIMIT));
    }

    #[test]
    fn accepted_related_ids_share_the_request_edge_budget() {
        let mut pathdb = PathDB::new();
        let selected = pathdb.add_entity("Node", vec![("axiograph.entity_key", "entity:selected")]);
        for ordinal in 0..(MAX_GROUNDING_RELATED_EDGE_VISITS + 1) {
            let stable_id = format!("entity:related:{ordinal}");
            let target =
                pathdb.add_entity("Node", vec![("axiograph.entity_key", stable_id.as_str())]);
            pathdb.add_relation("related", selected, target, 1.0, vec![]);
        }
        let mut work = GroundingWork::default();
        let ids = accepted_related_stable_ids(&pathdb, selected, usize::MAX, &mut work);
        assert_eq!(ids.len(), MAX_GROUNDING_RELATED_EDGE_VISITS);
        assert_eq!(work.related_edge_visits, MAX_GROUNDING_RELATED_EDGE_VISITS);
        assert!(work.truncation_reasons.contains(RELATED_EDGE_VISIT_LIMIT));
        assert!(work.output_bytes <= MAX_GROUNDING_OUTPUT_BYTES);
    }

    #[test]
    fn overlapping_attribute_matches_fill_global_fact_budget() {
        let mut pathdb = PathDB::new();
        let first = pathdb.add_entity("Node", vec![("name", "match"), ("label", "match")]);
        let second = pathdb.add_entity("Node", vec![("name", "match"), ("label", "match")]);
        let third = pathdb.add_entity("Node", vec![("label", "match")]);

        let context = evidence_grounding_context(&pathdb, "match", 3).unwrap();
        assert_eq!(
            context.facts.iter().map(|fact| fact.id).collect::<Vec<_>>(),
            vec![first, second, third]
        );
        assert!(!context
            .truncation_reasons
            .iter()
            .any(|reason| reason == FACT_LIMIT));
    }

    #[test]
    fn accepted_grounding_is_byte_deterministic() {
        fn render(attrs: Vec<(&'static str, &'static str)>) -> String {
            let mut pathdb = PathDB::new();
            let entity_id = pathdb.add_entity("Person", attrs);
            let engine = GroundingEngine::new(&pathdb, 8);
            let mut work = GroundingWork::default();
            let fact =
                render_accepted_fact(&engine, entity_id, "axi:materialization:test", &mut work)
                    .expect("accepted fact rendering")
                    .expect("accepted fact fits output budget");
            serde_json::to_string(&fact).expect("serialize accepted fact")
        }

        let first = render(vec![
            ("role", "Inspector"),
            ("name", "Alice"),
            ("axiograph.entity_key", "entity:alice"),
            ("department", "Safety"),
        ]);
        let second = render(vec![
            ("department", "Safety"),
            ("axiograph.entity_key", "entity:alice"),
            ("name", "Alice"),
            ("role", "Inspector"),
        ]);
        assert_eq!(first, second);
        assert_eq!(
            first,
            r#"{"stable_id":"entity:alice","natural":"Alice is a Person with axiograph.entity_key: entity:alice, department: Safety, role: Inspector","structured":"AcceptedEntity(stable_id=entity:alice, type=Person)","citation":["AxiStore:Materialization:axi:materialization:test:Entity:entity:alice"],"related":[]}"#
        );
    }

    #[test]
    fn guardrails_come_only_from_stored_entities() {
        let mut pathdb = PathDB::new();
        let context = evidence_grounding_context(&pathdb, "titanium cutting", 10)
            .expect("empty evidence context");
        assert!(context.active_guardrails.is_empty());

        pathdb.add_entity(
            "Guardrail",
            vec![
                ("rule_id", "stored_safety_rule"),
                ("severity", "warning"),
                ("description", "Review titanium cutting parameters"),
                ("applies_when", "cutting titanium"),
            ],
        );
        let context = evidence_grounding_context(&pathdb, "titanium cutting", 10)
            .expect("stored guardrail context");
        assert_eq!(context.active_guardrails.len(), 1);
        assert_eq!(context.active_guardrails[0].rule_id, "stored_safety_rule");
    }
}
