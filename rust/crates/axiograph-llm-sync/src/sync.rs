//! Sync Manager: Orchestrates bidirectional LLM ↔ KG synchronization
//!
//! The sync manager coordinates:
//! 1. Fact extraction from LLM conversations
//! 2. Validation against schema
//! 3. Conflict detection and resolution
//! 4. Storage to both .axi files and PathDB
//! 5. Provenance and version tracking

#![allow(unused_imports, unused_mut, unused_variables)]

use crate::{
    Conflict, ConflictResolver, ConflictType, ConversationTurn, ExtractedFact, FactExtractor,
    FactId, FactSource, FactStatus, FactValidator, GroundedFact, GroundingContext,
    GuardrailContext, LLMProvider, Resolution, SchemaContext, SessionId, StructuredFact,
    SyncConfig, SyncState, ValidationResult,
};
use axiograph_pathdb::PathDB;
use axiograph_storage::{Change, ChangeSource, StorableFact, UnifiedStorage};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Sync Events for Observability
// ============================================================================

/// Events emitted during sync operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEvent {
    /// Facts extracted from conversation
    FactsExtracted {
        session_id: SessionId,
        count: usize,
        source: String,
    },
    /// Facts validated
    FactsValidated {
        valid: usize,
        invalid: usize,
        needs_review: usize,
    },
    /// Conflicts detected
    ConflictsDetected {
        count: usize,
        types: Vec<ConflictType>,
    },
    /// Facts integrated into storage
    FactsIntegrated {
        count: usize,
        axi_files: Vec<String>,
        pathdb_ids: Vec<u32>,
    },
    /// Rollback performed
    RolledBack {
        to_version: u64,
        facts_removed: usize,
    },
    /// Error during sync
    SyncError { message: String },
}

/// Callback for sync events
pub type SyncEventHandler = Box<dyn Fn(SyncEvent) + Send + Sync>;

// ============================================================================
// Sync Manager
// ============================================================================

/// The main sync manager integrating LLM with unified storage
pub struct SyncManager {
    /// Unified storage (handles both .axi and PathDB)
    storage: Arc<UnifiedStorage>,
    /// Current sync state
    state: Arc<RwLock<SyncState>>,
    /// Configuration
    config: SyncConfig,
    /// Event handlers
    event_handlers: Vec<SyncEventHandler>,
    /// Default LLM provider
    default_provider: LLMProvider,
}

impl SyncManager {
    /// Create a new sync manager with unified storage
    pub fn new(
        storage: Arc<UnifiedStorage>,
        config: SyncConfig,
        default_provider: LLMProvider,
    ) -> Self {
        let state = SyncState {
            session_id: Uuid::new_v4(),
            last_sync: Utc::now(),
            pending_facts: Vec::new(),
            recent_integrations: Vec::new(),
            conflicts: Vec::new(),
            graph_version: 0,
        };

        Self {
            storage,
            state: Arc::new(RwLock::new(state)),
            config,
            event_handlers: Vec::new(),
            default_provider,
        }
    }

    /// Add an event handler
    pub fn on_event(&mut self, handler: SyncEventHandler) {
        self.event_handlers.push(handler);
    }

    /// Emit an event to all handlers
    fn emit(&self, event: SyncEvent) {
        for handler in &self.event_handlers {
            handler(event.clone());
        }
    }

    // ========================================================================
    // LLM → KG: Extract and Integrate
    // ========================================================================

    /// Extract facts from conversation and integrate into storage
    pub async fn sync_from_conversation(
        &self,
        conversation: &[ConversationTurn],
        provider: Option<LLMProvider>,
    ) -> anyhow::Result<SyncResult> {
        let provider = provider.unwrap_or_else(|| self.default_provider.clone());
        let session_id = self.state.read().session_id;

        // Step 1: Extract facts
        let extracted = self.extract_facts(conversation, &provider).await?;

        self.emit(SyncEvent::FactsExtracted {
            session_id,
            count: extracted.len(),
            source: format!("{:?}", provider),
        });

        // Step 2: Validate facts
        let (valid, invalid, needs_review) = self.validate_facts(&extracted)?;

        self.emit(SyncEvent::FactsValidated {
            valid: valid.len(),
            invalid: invalid.len(),
            needs_review: needs_review.len(),
        });

        // Step 3: Detect conflicts
        let conflicts = self.detect_conflicts(&valid)?;

        if !conflicts.is_empty() {
            self.emit(SyncEvent::ConflictsDetected {
                count: conflicts.len(),
                types: conflicts.iter().map(|c| c.conflict_type.clone()).collect(),
            });
        }

        // Step 4: Integrate valid, non-conflicting facts
        let integrated = self.integrate_facts(valid, &provider, session_id)?;

        self.emit(SyncEvent::FactsIntegrated {
            count: integrated.len(),
            axi_files: vec!["llm_extracted.axi".to_string()],
            pathdb_ids: integrated
                .iter()
                .flat_map(|f| {
                    if let FactStatus::Integrated { entity_ids } = &f.status {
                        entity_ids.clone()
                    } else {
                        Vec::new()
                    }
                })
                .collect(),
        });

        // Step 5: Store pending review items
        {
            let mut state = self.state.write();
            state.pending_facts.extend(needs_review);
            state.conflicts.extend(conflicts);
            state
                .recent_integrations
                .extend(integrated.iter().map(|f| f.id));
            state.last_sync = Utc::now();
            state.graph_version += 1;
        }

        Ok(SyncResult {
            integrated_count: integrated.len(),
            pending_review: self.state.read().pending_facts.len(),
            conflicts: self.state.read().conflicts.len(),
            invalid_count: invalid.len(),
        })
    }

    /// Extract facts from conversation using pattern matching + LLM
    async fn extract_facts(
        &self,
        conversation: &[ConversationTurn],
        _provider: &LLMProvider,
    ) -> anyhow::Result<Vec<ExtractedFact>> {
        let mut facts = Vec::new();

        for (idx, turn) in conversation.iter().enumerate() {
            // Simple pattern-based extraction (would use LLM in production)
            let extracted = self.pattern_extract(&turn.content)?;

            for structured in extracted {
                facts.push(ExtractedFact {
                    id: Uuid::new_v4(),
                    claim: self.structured_to_natural(&structured),
                    structured,
                    confidence: 0.85, // Would come from LLM
                    source: FactSource {
                        session_id: self.state.read().session_id,
                        provider: _provider.clone(),
                        conversation_turns: vec![idx],
                        extraction_timestamp: Utc::now(),
                        human_verified: false,
                    },
                    status: FactStatus::Pending,
                });
            }
        }

        Ok(facts)
    }

    /// Pattern-based fact extraction (simplified)
    fn pattern_extract(&self, text: &str) -> anyhow::Result<Vec<StructuredFact>> {
        let mut facts = Vec::new();

        // Pattern: "X is a Y"
        let is_a_re = regex::Regex::new(r"(?i)(\w+)\s+is\s+a\s+(\w+)")?;
        for cap in is_a_re.captures_iter(text) {
            facts.push(StructuredFact::Entity {
                entity_type: cap[2].to_string(),
                name: cap[1].to_string(),
                attributes: std::collections::HashMap::new(),
            });
        }

        // Pattern: "X has Y of Z"
        let has_re = regex::Regex::new(r"(?i)(\w+)\s+has\s+(\w+)\s+of\s+(\w+)")?;
        for cap in has_re.captures_iter(text) {
            let mut attrs = std::collections::HashMap::new();
            attrs.insert(cap[2].to_string(), cap[3].to_string());
            facts.push(StructuredFact::Entity {
                entity_type: "Unknown".to_string(),
                name: cap[1].to_string(),
                attributes: attrs,
            });
        }

        // Pattern: "always/never/should X when Y"
        let rule_re = regex::Regex::new(r"(?i)(always|never|should)\s+(.+?)\s+when\s+(.+)")?;
        for cap in rule_re.captures_iter(text) {
            facts.push(StructuredFact::TacitKnowledge {
                rule: format!("{} -> {}", &cap[3], &cap[2]),
                confidence: 0.8,
                domain: "general".to_string(),
            });
        }

        Ok(facts)
    }

    /// Convert structured fact to natural language
    fn structured_to_natural(&self, fact: &StructuredFact) -> String {
        match fact {
            StructuredFact::Entity {
                entity_type,
                name,
                attributes,
            } => {
                let attrs: Vec<String> = attributes
                    .iter()
                    .map(|(k, v)| format!("{} = {}", k, v))
                    .collect();
                if attrs.is_empty() {
                    format!("{} is a {}", name, entity_type)
                } else {
                    format!("{} is a {} with {}", name, entity_type, attrs.join(", "))
                }
            }
            StructuredFact::Relation {
                rel_type,
                source,
                target,
                ..
            } => {
                format!("{} {} {}", source, rel_type, target)
            }
            StructuredFact::Constraint {
                name, condition, ..
            } => {
                format!("Constraint {}: {}", name, condition)
            }
            StructuredFact::TacitKnowledge {
                rule,
                confidence,
                domain,
            } => {
                format!(
                    "[{}] {} (confidence: {:.0}%)",
                    domain,
                    rule,
                    confidence * 100.0
                )
            }
        }
    }

    /// Validate extracted facts
    fn validate_facts(
        &self,
        facts: &[ExtractedFact],
    ) -> anyhow::Result<(Vec<ExtractedFact>, Vec<ExtractedFact>, Vec<ExtractedFact>)> {
        let mut valid = Vec::new();
        let mut invalid = Vec::new();
        let mut needs_review = Vec::new();

        let pathdb = self.storage.pathdb();
        let db = pathdb.read();

        for fact in facts {
            // Check schema validity
            let schema_valid = self.check_schema_validity(&fact.structured, &db);

            if !schema_valid {
                let mut rejected = fact.clone();
                rejected.status = FactStatus::Validated; // Mark as needing schema extension
                needs_review.push(rejected);
                continue;
            }

            // Check confidence threshold
            if fact.confidence < self.config.auto_integrate_threshold {
                let mut low_conf = fact.clone();
                low_conf.status = FactStatus::NeedsReview {
                    reason: format!("Confidence {:.0}% below threshold", fact.confidence * 100.0),
                };
                needs_review.push(low_conf);
                continue;
            }

            // Check if constraint (requires human review)
            if matches!(fact.structured, StructuredFact::Constraint { .. })
                && self.config.human_review_constraints
            {
                let mut constraint = fact.clone();
                constraint.status = FactStatus::NeedsReview {
                    reason: "Constraints require human review".to_string(),
                };
                needs_review.push(constraint);
                continue;
            }

            // Valid
            let mut validated = fact.clone();
            validated.status = FactStatus::Validated;
            valid.push(validated);
        }

        Ok((valid, invalid, needs_review))
    }

    /// Check if fact matches schema
    fn check_schema_validity(&self, fact: &StructuredFact, _db: &PathDB) -> bool {
        // Simplified - would check against actual schema
        match fact {
            StructuredFact::Entity { entity_type, .. } => {
                // Accept common entity types
                matches!(
                    entity_type.as_str(),
                    "Material"
                        | "Tool"
                        | "Operation"
                        | "Machine"
                        | "Constraint"
                        | "Guideline"
                        | "Concept"
                        | "Unknown"
                        | "Person"
                        | "Organization"
                        | "Location"
                        | "Event"
                )
            }
            StructuredFact::Relation { rel_type, .. } => {
                // Accept common relation types
                matches!(
                    rel_type.as_str(),
                    "hasMaterial"
                        | "usesTool"
                        | "produces"
                        | "requires"
                        | "isPartOf"
                        | "relatedTo"
                        | "precedes"
                        | "follows"
                )
            }
            _ => true, // Constraints and tacit knowledge always pass
        }
    }

    /// Detect conflicts with existing knowledge
    fn detect_conflicts(&self, facts: &[ExtractedFact]) -> anyhow::Result<Vec<Conflict>> {
        let pathdb = self.storage.pathdb();
        let db = pathdb.read();
        let mut conflicts = Vec::new();

        for fact in facts {
            // Check for duplicate entities
            if let StructuredFact::Entity {
                name, entity_type, ..
            } = &fact.structured
            {
                // Look for existing entity with same name
                // Simplified - would use actual name lookup
                if let Some(existing) = db.find_by_type(entity_type.as_str()) {
                    if !existing.is_empty() {
                        // Potential duplicate
                        conflicts.push(Conflict {
                            new_fact: fact.clone(),
                            existing_facts: existing.iter().collect(),
                            conflict_type: ConflictType::AttributeMismatch,
                            suggested_resolution: Resolution::Merge {
                                weights: (0.5, 0.5),
                            },
                        });
                    }
                }
            }
        }

        Ok(conflicts)
    }

    /// Integrate validated facts into unified storage
    fn integrate_facts(
        &self,
        facts: Vec<ExtractedFact>,
        provider: &LLMProvider,
        session_id: SessionId,
    ) -> anyhow::Result<Vec<ExtractedFact>> {
        let mut integrated = Vec::new();

        // Convert to storable facts
        let storable: Vec<StorableFact> = facts
            .iter()
            .filter_map(|f| self.to_storable(&f.structured))
            .collect();

        if storable.is_empty() {
            return Ok(integrated);
        }

        // Add to unified storage
        let source = ChangeSource::LLMExtraction {
            session_id,
            model: format!("{:?}", provider),
            confidence: facts.iter().map(|f| f.confidence).sum::<f32>() / facts.len() as f32,
        };

        self.storage.add_facts(storable, source)?;

        // Flush to persist
        let results = self.storage.flush()?;

        // Update fact statuses
        for (i, fact) in facts.into_iter().enumerate() {
            let mut updated = fact;
            if let Some(result) = results.get(0) {
                // Simplified
                updated.status = FactStatus::Integrated {
                    entity_ids: result.pathdb_ids.clone(),
                };
            }
            integrated.push(updated);
        }

        Ok(integrated)
    }

    /// Convert extracted fact to storable fact
    fn to_storable(&self, fact: &StructuredFact) -> Option<StorableFact> {
        match fact {
            StructuredFact::Entity {
                entity_type,
                name,
                attributes,
            } => Some(StorableFact::Entity {
                name: name.clone(),
                entity_type: entity_type.clone(),
                attributes: attributes
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            }),
            StructuredFact::Relation {
                rel_type,
                source,
                target,
                attributes,
            } => Some(StorableFact::Relation {
                name: None,
                rel_type: rel_type.clone(),
                source: source.clone(),
                target: target.clone(),
                confidence: 1.0,
                attributes: attributes
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            }),
            StructuredFact::Constraint {
                name,
                condition,
                severity,
            } => Some(StorableFact::Constraint {
                name: name.clone(),
                condition: condition.clone(),
                severity: severity.clone(),
                message: None,
            }),
            StructuredFact::TacitKnowledge {
                rule,
                confidence,
                domain,
            } => Some(StorableFact::TacitKnowledge {
                name: format!(
                    "tacit_{}",
                    Uuid::new_v4().to_string().split('-').next().unwrap()
                ),
                rule: rule.clone(),
                confidence: *confidence,
                domain: domain.clone(),
                source: "LLM extraction".to_string(),
            }),
        }
    }

    // ========================================================================
    // KG → LLM: Build Grounding Context
    // ========================================================================

    /// Build grounding context for LLM from knowledge graph
    pub fn build_grounding_context(
        &self,
        query: &str,
        max_facts: usize,
    ) -> anyhow::Result<GroundingContext> {
        let pathdb = self.storage.pathdb();
        let db = pathdb.read();

        // Extract keywords from query
        let keywords = self.extract_keywords(query);

        // Find relevant facts
        let mut facts = Vec::new();
        for keyword in &keywords {
            if let Some(entities) = db.find_by_type(keyword) {
                for id in entities.iter().take(max_facts / keywords.len().max(1)) {
                    if let Some(entity) = db.get_entity(id) {
                        facts.push(GroundedFact {
                            id,
                            natural: format!(
                                "{} is a {}",
                                entity
                                    .attrs
                                    .get("name")
                                    .map(|s| s.as_str())
                                    .unwrap_or("entity"),
                                entity.entity_type
                            ),
                            structured: format!("Entity({}, type={})", id, entity.entity_type),
                            confidence: 1.0,
                            citation: vec![format!("PathDB:Entity:{}", id)],
                            related: vec![],
                        });
                    }
                }
            }
        }

        // Build schema context
        let schema = self.storage.schema();
        let schema_module = schema.read();
        let schema_context = SchemaContext {
            entity_types: schema_module.entity_types.clone(),
            relation_types: schema_module.relation_types.clone(),
            constraints: schema_module.constraints.clone(),
        };

        // Get applicable guardrails
        let guardrails = self.get_applicable_guardrails(&keywords);

        Ok(GroundingContext {
            facts,
            schema_context: Some(schema_context),
            active_guardrails: guardrails,
            suggested_queries: self.suggest_followup_queries(query),
        })
    }

    /// Extract keywords from query
    fn extract_keywords(&self, query: &str) -> Vec<String> {
        // Simple keyword extraction (would use NLP in production)
        query
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.to_lowercase())
            .filter(|w| {
                !matches!(
                    w.as_str(),
                    "what" | "how" | "when" | "where" | "which" | "that" | "this"
                )
            })
            .collect()
    }

    /// Get applicable guardrails for topic
    fn get_applicable_guardrails(&self, _keywords: &[String]) -> Vec<GuardrailContext> {
        // Simplified - would query guardrail index
        vec![GuardrailContext {
            rule_id: "safety_001".to_string(),
            severity: "warning".to_string(),
            description: "Safety verification required for cutting operations".to_string(),
            applies_when: "discussing cutting parameters".to_string(),
        }]
    }

    /// Suggest follow-up queries
    fn suggest_followup_queries(&self, _query: &str) -> Vec<String> {
        // Simplified - would use query patterns
        vec![
            "What are the recommended parameters?".to_string(),
            "Are there any safety constraints?".to_string(),
            "What related concepts should I understand?".to_string(),
        ]
    }

    // ========================================================================
    // Review and Conflict Resolution
    // ========================================================================

    /// Get pending facts awaiting review
    pub fn pending_review(&self) -> Vec<ExtractedFact> {
        self.state.read().pending_facts.clone()
    }

    /// Get unresolved conflicts
    pub fn unresolved_conflicts(&self) -> Vec<Conflict> {
        self.state.read().conflicts.clone()
    }

    /// Approve a pending fact
    pub fn approve_fact(&self, fact_id: FactId) -> anyhow::Result<()> {
        let mut state = self.state.write();

        if let Some(idx) = state.pending_facts.iter().position(|f| f.id == fact_id) {
            let fact = state.pending_facts.remove(idx);
            drop(state);

            // Integrate the approved fact
            if let Some(storable) = self.to_storable(&fact.structured) {
                self.storage
                    .add_facts(vec![storable], ChangeSource::UserEdit { user_id: None })?;
                self.storage.flush()?;
            }
        }

        Ok(())
    }

    /// Reject a pending fact
    pub fn reject_fact(&self, fact_id: FactId, reason: &str) -> anyhow::Result<()> {
        let mut state = self.state.write();

        if let Some(fact) = state.pending_facts.iter_mut().find(|f| f.id == fact_id) {
            fact.status = FactStatus::Rejected {
                reason: reason.to_string(),
            };
        }

        Ok(())
    }

    /// Resolve a conflict
    pub fn resolve_conflict(
        &self,
        conflict_id: usize,
        resolution: Resolution,
    ) -> anyhow::Result<()> {
        let mut state = self.state.write();

        if conflict_id < state.conflicts.len() {
            let conflict = &state.conflicts[conflict_id];

            match resolution {
                Resolution::ReplaceOld => {
                    // Integrate new fact
                    if let Some(storable) = self.to_storable(&conflict.new_fact.structured) {
                        drop(state);
                        self.storage
                            .add_facts(vec![storable], ChangeSource::UserEdit { user_id: None })?;
                        self.storage.flush()?;
                    }
                }
                Resolution::KeepOld => {
                    // Just remove from conflicts
                }
                Resolution::Merge { weights: _ } => {
                    // Would merge attributes
                    // Simplified for now
                }
                Resolution::HumanReview => {
                    // Move to pending review
                    let conflict = state.conflicts.remove(conflict_id);
                    state.pending_facts.push(conflict.new_fact);
                    return Ok(());
                }
            }

            let mut state = self.state.write();
            state.conflicts.remove(conflict_id);
        }

        Ok(())
    }

    // ========================================================================
    // State Management
    // ========================================================================

    /// Get current sync state
    pub fn state(&self) -> SyncState {
        self.state.read().clone()
    }

    /// Start a new session
    pub fn new_session(&self) -> SessionId {
        let mut state = self.state.write();
        state.session_id = Uuid::new_v4();
        state.session_id
    }

    /// Get statistics
    pub fn stats(&self) -> SyncStats {
        let state = self.state.read();
        let changelog = self.storage.changelog();

        SyncStats {
            total_integrated: changelog
                .iter()
                .filter(|c| matches!(c.status, axiograph_storage::ChangeStatus::Applied))
                .flat_map(|c| &c.facts)
                .count(),
            pending_review: state.pending_facts.len(),
            unresolved_conflicts: state.conflicts.len(),
            graph_version: state.graph_version,
        }
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of a sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub integrated_count: usize,
    pub pending_review: usize,
    pub conflicts: usize,
    pub invalid_count: usize,
}

/// Sync statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_integrated: usize,
    pub pending_review: usize,
    pub unresolved_conflicts: usize,
    #[serde(alias = "kg_version")]
    pub graph_version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_storage::StorageConfig;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_sync_from_conversation() {
        let dir = tempdir().unwrap();
        let config = StorageConfig {
            axi_dir: dir.path().to_path_buf(),
            pathdb_path: dir.path().join("test.axpd"),
            changelog_path: dir.path().join("changelog.json"),
            ..Default::default()
        };

        let storage = Arc::new(UnifiedStorage::new(config).unwrap());
        let sync_config = SyncConfig::default();
        let manager = SyncManager::new(
            storage,
            sync_config,
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        );

        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Titanium is a Material with hardness of 36".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];

        let result = manager
            .sync_from_conversation(&conversation, None)
            .await
            .unwrap();

        // Should extract the entity
        assert!(result.integrated_count > 0 || result.pending_review > 0);
    }
}
