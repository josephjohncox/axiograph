//! Grounding Engine: Build context from KG for LLM generation

use crate::{
    AcceptedGroundedFact, AcceptedGroundingContext, AcceptedGroundingProvenanceV1, GroundedFact,
    GroundingContext, GroundingProvenanceV1, GuardrailContext, SchemaContext,
};
use anyhow::{anyhow, Result};
use axiograph_kernel::ObjectBlobIdV2;
use axiograph_pathdb::{materialization::MaterializedPathDb, PathDB};
use roaring::RoaringBitmap;
use std::collections::{BTreeSet, HashSet};

pub const MAX_GROUNDING_FACTS: usize = 256;
pub const MAX_GROUNDING_QUERY_BYTES: usize = 16 * 1024;
const GROUNDING_SEARCH_ATTRIBUTES: &[&str] = &[
    "name",
    "label",
    "text",
    "description",
    "axiograph.value",
    "axiograph.entity_key",
    "axi_fact_id",
];

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

    fn build_context(&self, query: &str) -> GroundingContext {
        let keywords = self.extract_keywords(query);
        let facts = self.retrieve_relevant_facts(&keywords);
        let suggestions = self.generate_suggestions(query, &facts);

        GroundingContext {
            provenance: GroundingProvenanceV1::evidence("pathdb_process_local_evidence"),
            facts,
            schema_context: Some(self.build_schema_context()),
            active_guardrails: self.get_applicable_guardrails(&keywords),
            suggested_queries: suggestions,
        }
    }

    /// Extract keywords from query
    fn extract_keywords(&self, query: &str) -> Vec<String> {
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

        query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2 && !stopwords.contains(w))
            .map(String::from)
            .collect()
    }

    fn retrieve_relevant_entity_ids(&self, keywords: &[String]) -> (Vec<u32>, bool) {
        let mut matches = RoaringBitmap::new();
        let type_names = self.pathdb.entity_type_names();
        for keyword in keywords {
            for type_name in &type_names {
                if type_name.to_ascii_lowercase().contains(keyword) {
                    if let Some(ids) = self.pathdb.find_by_type(type_name) {
                        matches |= ids;
                    }
                }
            }
            for attribute in GROUNDING_SEARCH_ATTRIBUTES {
                matches |= self.pathdb.entities_with_attr_fts_any(attribute, keyword);
            }
        }
        let truncated = matches.len() > self.max_facts as u64;
        (matches.iter().take(self.max_facts).collect(), truncated)
    }

    /// Retrieve facts relevant to keywords.
    fn retrieve_relevant_facts(&self, keywords: &[String]) -> Vec<GroundedFact> {
        self.retrieve_relevant_entity_ids(keywords)
            .0
            .into_iter()
            .filter_map(|id| {
                let entity = self.pathdb.get_entity(id)?;
                Some(GroundedFact {
                    id,
                    natural: self.entity_to_natural(&entity),
                    structured: format!("Entity(id={id}, type={})", entity.entity_type),
                    confidence: 1.0,
                    citation: vec![format!("PathDB:Entity:{id}")],
                    related: self.get_related_concepts(id),
                })
            })
            .collect()
    }

    fn entity_to_natural(&self, entity: &axiograph_pathdb::EntityView) -> String {
        let name = entity
            .attrs
            .get("name")
            .or_else(|| entity.attrs.get("label"))
            .or_else(|| entity.attrs.get("axiograph.value"))
            .map(String::as_str)
            .unwrap_or("entity");

        let attrs: Vec<String> = entity
            .attrs
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "name" | "label" | "axiograph.value"))
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();

        if attrs.is_empty() {
            format!("{name} is a {}", entity.entity_type)
        } else {
            format!(
                "{name} is a {} with {}",
                entity.entity_type,
                attrs.join(", ")
            )
        }
    }

    fn get_related_concepts(&self, entity_id: u32) -> Vec<String> {
        let mut related = BTreeSet::new();
        for relation in self.pathdb.relations.outgoing_any(entity_id) {
            let relation_type = self
                .pathdb
                .interner
                .lookup(relation.rel_type)
                .unwrap_or_else(|| "unknown_relation".to_string());
            related.insert(format!(
                "{relation_type}->{}",
                self.entity_reference_label(relation.target)
            ));
        }
        for relation in self.pathdb.relations.incoming_any(entity_id) {
            let relation_type = self
                .pathdb
                .interner
                .lookup(relation.rel_type)
                .unwrap_or_else(|| "unknown_relation".to_string());
            related.insert(format!(
                "<-{relation_type}-{}",
                self.entity_reference_label(relation.source)
            ));
        }
        related.into_iter().take(self.max_facts).collect()
    }

    fn entity_reference_label(&self, entity_id: u32) -> String {
        let Some(entity) = self.pathdb.get_entity(entity_id) else {
            return format!("entity:{entity_id}");
        };
        entity
            .attrs
            .get("name")
            .or_else(|| entity.attrs.get("label"))
            .or_else(|| entity.attrs.get("axiograph.value"))
            .cloned()
            .unwrap_or_else(|| format!("entity:{entity_id}"))
    }

    fn build_schema_context(&self) -> SchemaContext {
        SchemaContext {
            entity_types: self.pathdb.entity_type_names(),
            relation_types: self.pathdb.relation_type_names(),
            constraints: Vec::new(),
        }
    }

    fn get_applicable_guardrails(&self, keywords: &[String]) -> Vec<GuardrailContext> {
        let Some(ids) = self.pathdb.find_by_type("Guardrail") else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|id| self.pathdb.get_entity(id))
            .filter(|entity| {
                let searchable = entity
                    .attrs
                    .values()
                    .map(|value| value.to_ascii_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ");
                keywords.iter().any(|keyword| searchable.contains(keyword))
            })
            .take(self.max_facts)
            .map(|entity| GuardrailContext {
                rule_id: entity
                    .attrs
                    .get("rule_id")
                    .or_else(|| entity.attrs.get("name"))
                    .cloned()
                    .unwrap_or_else(|| format!("PathDB:Guardrail:{}", entity.id)),
                severity: entity
                    .attrs
                    .get("severity")
                    .cloned()
                    .unwrap_or_else(|| "unspecified".to_string()),
                description: entity
                    .attrs
                    .get("description")
                    .or_else(|| entity.attrs.get("rule"))
                    .cloned()
                    .unwrap_or_else(|| format!("stored Guardrail entity {}", entity.id)),
                applies_when: entity
                    .attrs
                    .get("applies_when")
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect()
    }

    fn generate_suggestions(&self, _query: &str, facts: &[GroundedFact]) -> Vec<String> {
        let mut suggestions = vec![];

        // Suggest exploring related concepts
        if !facts.is_empty() {
            suggestions.push(format!(
                "What are the relationships between these {} concepts?",
                facts.len()
            ));
        }

        // Generic suggestions
        suggestions.extend(vec![
            "What constraints apply to this domain?".to_string(),
            "Are there any safety considerations?".to_string(),
            "What are the best practices?".to_string(),
        ]);

        suggestions.truncate(5);
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
    Ok(GroundingEngine::new(pathdb, max_facts).build_context(query))
}

fn accepted_runtime_stable_id(entity: &axiograph_pathdb::EntityView) -> Option<&str> {
    entity
        .attrs
        .get("axiograph.entity_key")
        .or_else(|| entity.attrs.get("axi_fact_id"))
        .map(String::as_str)
}

fn accepted_related_stable_ids(pathdb: &PathDB, entity_id: u32, limit: usize) -> Vec<String> {
    let mut related = BTreeSet::new();
    for relation in pathdb.relations.outgoing_any(entity_id) {
        if let Some(entity) = pathdb.get_entity(relation.target) {
            if let Some(stable_id) = accepted_runtime_stable_id(&entity) {
                related.insert(stable_id.to_string());
            }
        }
    }
    for relation in pathdb.relations.incoming_any(entity_id) {
        if let Some(entity) = pathdb.get_entity(relation.source) {
            if let Some(stable_id) = accepted_runtime_stable_id(&entity) {
                related.insert(stable_id.to_string());
            }
        }
    }
    related.into_iter().take(limit).collect()
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
    let keywords = engine.extract_keywords(query);
    let (entity_ids, truncated) = engine.retrieve_relevant_entity_ids(&keywords);
    let materialization_id = materialized.receipt().materialization_id.to_string();
    let facts = entity_ids
        .into_iter()
        .map(|id| {
            let entity = materialized
                .db()
                .get_entity(id)
                .ok_or_else(|| anyhow!("grounding selected absent runtime entity {id}"))?;
            let stable_id = accepted_runtime_stable_id(&entity)
                .map(str::to_string)
                .ok_or_else(|| {
                    anyhow!(
                        "authenticated runtime entity {id} has no stable entity or fact identity"
                    )
                })?;
            Ok(AcceptedGroundedFact::new(
                stable_id.clone(),
                engine.entity_to_natural(&entity),
                format!(
                    "AcceptedEntity(stable_id={stable_id}, type={})",
                    entity.entity_type
                ),
                vec![format!(
                    "AxiStore:Materialization:{materialization_id}:Entity:{stable_id}"
                )],
                accepted_related_stable_ids(materialized.db(), id, max_facts),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
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
        engine.build_schema_context(),
        truncated,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_extraction_omits_stopwords() {
        let pathdb = PathDB::new();
        let engine = GroundingEngine::new(&pathdb, 20);

        let keywords = engine.extract_keywords("What is the hardness of titanium?");
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
