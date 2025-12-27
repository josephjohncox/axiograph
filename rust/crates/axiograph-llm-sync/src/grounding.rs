//! Grounding Engine: Build context from KG for LLM generation

use crate::{GroundedFact, GroundingContext, GuardrailContext, SchemaContext};
use axiograph_pathdb::PathDB;
use std::collections::HashSet;

/// Engine for building grounding context from PathDB
pub struct GroundingEngine<'a> {
    pathdb: &'a PathDB,
    max_facts: usize,
    include_schema: bool,
    include_guardrails: bool,
}

impl<'a> GroundingEngine<'a> {
    pub fn new(pathdb: &'a PathDB) -> Self {
        Self {
            pathdb,
            max_facts: 20,
            include_schema: true,
            include_guardrails: true,
        }
    }

    pub fn max_facts(mut self, n: usize) -> Self {
        self.max_facts = n;
        self
    }

    pub fn include_schema(mut self, include: bool) -> Self {
        self.include_schema = include;
        self
    }

    pub fn include_guardrails(mut self, include: bool) -> Self {
        self.include_guardrails = include;
        self
    }

    /// Build grounding context for a query
    pub fn build_context(&self, query: &str) -> GroundingContext {
        let keywords = self.extract_keywords(query);
        let facts = self.retrieve_relevant_facts(&keywords);
        let schema = if self.include_schema {
            Some(self.build_schema_context())
        } else {
            None
        };
        let guardrails = if self.include_guardrails {
            self.get_applicable_guardrails(&keywords)
        } else {
            vec![]
        };
        let suggestions = self.generate_suggestions(query, &facts);

        GroundingContext {
            facts,
            schema_context: schema,
            active_guardrails: guardrails,
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

    /// Retrieve facts relevant to keywords
    fn retrieve_relevant_facts(&self, keywords: &[String]) -> Vec<GroundedFact> {
        let mut facts = Vec::new();
        let mut seen_ids = HashSet::new();

        for keyword in keywords {
            // Try as entity type
            if let Some(entities) = self.pathdb.find_by_type(keyword) {
                for id in entities.iter().take(self.max_facts / keywords.len().max(1)) {
                    if seen_ids.insert(id) {
                        if let Some(entity) = self.pathdb.get_entity(id) {
                            facts.push(GroundedFact {
                                id,
                                natural: self.entity_to_natural(&entity),
                                structured: format!(
                                    "Entity(id={}, type={})",
                                    id, entity.entity_type
                                ),
                                confidence: 1.0,
                                citation: vec![format!("PathDB:Entity:{}", id)],
                                related: self.get_related_concepts(id),
                            });
                        }
                    }
                }
            }

            // Try relations
            // (simplified - would use relation type index)
        }

        // Limit total
        facts.truncate(self.max_facts);
        facts
    }

    fn entity_to_natural(&self, entity: &axiograph_pathdb::EntityView) -> String {
        let name = entity
            .attrs
            .get("name")
            .map(|s| s.as_str())
            .unwrap_or("entity");

        let attrs: Vec<String> = entity
            .attrs
            .iter()
            .filter(|(k, _)| k.as_str() != "name")
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

    fn get_related_concepts(&self, _entity_id: u32) -> Vec<String> {
        // Would traverse relations to find related concepts
        vec![]
    }

    fn build_schema_context(&self) -> SchemaContext {
        // Would build from actual schema
        SchemaContext {
            entity_types: vec![
                "Material".to_string(),
                "Tool".to_string(),
                "Operation".to_string(),
                "Concept".to_string(),
            ],
            relation_types: vec![
                "hasMaterial".to_string(),
                "usesTool".to_string(),
                "requires".to_string(),
                "produces".to_string(),
            ],
            constraints: vec![],
        }
    }

    fn get_applicable_guardrails(&self, keywords: &[String]) -> Vec<GuardrailContext> {
        let mut guardrails = Vec::new();

        // Check for safety-related keywords
        let safety_keywords = ["cutting", "speed", "feed", "titanium", "heat", "coolant"];
        if keywords
            .iter()
            .any(|k| safety_keywords.contains(&k.as_str()))
        {
            guardrails.push(GuardrailContext {
                rule_id: "machining_safety".to_string(),
                severity: "warning".to_string(),
                description:
                    "Machining parameters should be verified against material specifications"
                        .to_string(),
                applies_when: "discussing cutting parameters".to_string(),
            });
        }

        // Check for constraint keywords
        if keywords
            .iter()
            .any(|k| k == "constraint" || k == "rule" || k == "must")
        {
            guardrails.push(GuardrailContext {
                rule_id: "constraint_review".to_string(),
                severity: "info".to_string(),
                description: "Constraints should be validated by domain expert".to_string(),
                applies_when: "defining constraints or rules".to_string(),
            });
        }

        guardrails
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

/// Builder pattern for context construction
pub struct ContextBuilder<'a> {
    pathdb: &'a PathDB,
    query: Option<String>,
    entity_ids: Vec<u32>,
    include_relations: bool,
    depth: usize,
    max_facts: usize,
}

impl<'a> ContextBuilder<'a> {
    pub fn new(pathdb: &'a PathDB) -> Self {
        Self {
            pathdb,
            query: None,
            entity_ids: vec![],
            include_relations: true,
            depth: 2,
            max_facts: 20,
        }
    }

    pub fn query(mut self, q: &str) -> Self {
        self.query = Some(q.to_string());
        self
    }

    pub fn entities(mut self, ids: Vec<u32>) -> Self {
        self.entity_ids = ids;
        self
    }

    pub fn include_relations(mut self, include: bool) -> Self {
        self.include_relations = include;
        self
    }

    pub fn depth(mut self, d: usize) -> Self {
        self.depth = d;
        self
    }

    pub fn max_facts(mut self, n: usize) -> Self {
        self.max_facts = n;
        self
    }

    pub fn build(self) -> GroundingContext {
        let engine = GroundingEngine::new(self.pathdb).max_facts(self.max_facts);

        if let Some(q) = self.query {
            engine.build_context(&q)
        } else if !self.entity_ids.is_empty() {
            // Build context from specific entities
            let mut facts = Vec::new();
            for id in &self.entity_ids {
                if let Some(entity) = self.pathdb.get_entity(*id) {
                    facts.push(GroundedFact {
                        id: *id,
                        natural: format!(
                            "{} is a {}",
                            entity
                                .attrs
                                .get("name")
                                .map(|s| s.as_str())
                                .unwrap_or("entity"),
                            entity.entity_type
                        ),
                        structured: format!("Entity(id={}, type={})", *id, entity.entity_type),
                        confidence: 1.0,
                        citation: vec![format!("PathDB:Entity:{}", *id)],
                        related: vec![],
                    });
                }
            }

            GroundingContext {
                facts,
                schema_context: None,
                active_guardrails: vec![],
                suggested_queries: vec![],
            }
        } else {
            GroundingContext {
                facts: vec![],
                schema_context: None,
                active_guardrails: vec![],
                suggested_queries: vec!["Try asking a specific question".to_string()],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_extraction() {
        let pathdb = PathDB::new();
        let engine = GroundingEngine::new(&pathdb);

        let keywords = engine.extract_keywords("What is the hardness of titanium?");
        assert!(keywords.contains(&"hardness".to_string()));
        assert!(keywords.contains(&"titanium".to_string()));
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"is".to_string()));
    }

    #[test]
    fn test_context_builder() {
        let pathdb = PathDB::new();
        let context = ContextBuilder::new(&pathdb)
            .query("titanium cutting")
            .max_facts(10)
            .build();

        // Empty PathDB, so no facts, but suggestions should exist
        assert!(!context.suggested_queries.is_empty());
    }
}
