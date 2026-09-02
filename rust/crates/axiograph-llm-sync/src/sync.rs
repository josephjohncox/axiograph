//! Sync Manager: orchestrates LLM evidence extraction and grounding
//!
//! The sync manager coordinates:
//! 1. Fact extraction from LLM conversations
//! 2. Validation against schema
//! 3. Conflict detection and resolution
//! 4. Evidence/cache materialization
//! 5. Provenance and version tracking

use crate::{
    Conflict, ConflictType, ConversationTurn, ExtractedFact, FactId, FactSource, FactStatus,
    GroundingContext, GroundingProvenanceV1, LLMProvider, Resolution, SessionId, StructuredFact,
    SyncConfig, SyncState,
};
use axiograph_storage::{ChangeId, ChangeSource, StorableFact, UnifiedStorage};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use uuid::Uuid;

const MAX_SYNC_BATCH_SIZE: usize = 10_000;
const MAX_SYNC_CONVERSATION_TURNS: usize = 4_096;
const MAX_SYNC_CONVERSATION_BYTES: usize = 8 * 1024 * 1024;

struct FactIntegrationOutcome {
    integrated: Vec<ExtractedFact>,
    pending_review: Vec<(ExtractedFact, ChangeId)>,
}

struct SyncExtractionPatterns {
    is_a: regex::Regex,
    has: regex::Regex,
    rule: regex::Regex,
}

fn sync_extraction_patterns() -> anyhow::Result<&'static SyncExtractionPatterns> {
    static PATTERNS: OnceLock<Result<SyncExtractionPatterns, regex::Error>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            Ok(SyncExtractionPatterns {
                is_a: regex::Regex::new(r"(?i)(\w+)\s+is\s+a\s+(\w+)")?,
                has: regex::Regex::new(r"(?i)(\w+)\s+has\s+(\w+)\s+of\s+(\w+)")?,
                rule: regex::Regex::new(r"(?i)(always|never|should)\s+(.+?)\s+when\s+(.+)")?,
            })
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!("compile sync extraction patterns: {error}"))
}

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
    FactsIntegrated { count: usize, pathdb_ids: Vec<u32> },
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

/// The main sync manager integrating LLM output with runtime evidence storage.
pub struct SyncManager {
    /// Runtime evidence store and PathDB cache materialization.
    storage: Arc<UnifiedStorage>,
    /// Current sync state
    state: Arc<RwLock<SyncState>>,
    /// Storage review changes keyed by their corresponding pending fact.
    pending_storage_changes: Arc<RwLock<HashMap<FactId, ChangeId>>>,
    /// Configuration
    config: SyncConfig,
    /// Event handlers
    event_handlers: Vec<SyncEventHandler>,
    /// Default LLM provider
    default_provider: LLMProvider,
}

impl SyncManager {
    /// Create a new sync manager with runtime evidence storage.
    pub fn new(
        storage: Arc<UnifiedStorage>,
        config: SyncConfig,
        default_provider: LLMProvider,
    ) -> anyhow::Result<Self> {
        if !config.auto_integrate_threshold.is_finite()
            || !(0.0..=1.0).contains(&config.auto_integrate_threshold)
        {
            return Err(anyhow::anyhow!(
                "auto_integrate_threshold must be a finite probability in [0, 1]"
            ));
        }
        if config.batch_size == 0 || config.batch_size > MAX_SYNC_BATCH_SIZE {
            return Err(anyhow::anyhow!(
                "batch_size must be in 1..={MAX_SYNC_BATCH_SIZE}"
            ));
        }
        let state = SyncState {
            session_id: Uuid::new_v4(),
            last_sync: Utc::now(),
            pending_facts: Vec::new(),
            recent_integrations: Vec::new(),
            rejected_facts: Vec::new(),
            conflicts: Vec::new(),
            graph_version: 0,
        };

        Ok(Self {
            storage,
            state: Arc::new(RwLock::new(state)),
            pending_storage_changes: Arc::new(RwLock::new(HashMap::new())),
            config,
            event_handlers: Vec::new(),
            default_provider,
        })
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
        if conversation.len() > MAX_SYNC_CONVERSATION_TURNS {
            return Err(anyhow::anyhow!(
                "conversation turn count {} exceeds {MAX_SYNC_CONVERSATION_TURNS}",
                conversation.len()
            ));
        }
        let mut conversation_bytes = 0_usize;
        for turn in conversation {
            conversation_bytes = conversation_bytes
                .checked_add(turn.content.len())
                .ok_or_else(|| anyhow::anyhow!("conversation byte count overflow"))?;
            for (key, value) in &turn.metadata {
                conversation_bytes = conversation_bytes
                    .checked_add(key.len())
                    .and_then(|total| total.checked_add(value.len()))
                    .ok_or_else(|| anyhow::anyhow!("conversation byte count overflow"))?;
            }
            if conversation_bytes > MAX_SYNC_CONVERSATION_BYTES {
                return Err(anyhow::anyhow!(
                    "conversation bytes {conversation_bytes} exceed {MAX_SYNC_CONVERSATION_BYTES}"
                ));
            }
        }
        let provider = provider.unwrap_or_else(|| self.default_provider.clone());
        let session_id = self.state.read().session_id;

        // Step 1: Extract facts
        let extracted = self.extract_facts(conversation, &provider).await?;
        if extracted.len() > self.config.batch_size {
            return Err(anyhow::anyhow!(
                "extracted fact count {} exceeds configured batch_size {}",
                extracted.len(),
                self.config.batch_size
            ));
        }

        self.emit(SyncEvent::FactsExtracted {
            session_id,
            count: extracted.len(),
            source: format!("{provider:?}"),
        });

        // Step 2: Validate facts
        let (valid, invalid, mut needs_review) = self.validate_facts(&extracted)?;

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
        let conflicting_fact_ids = conflicts
            .iter()
            .map(|conflict| conflict.new_fact.id)
            .collect::<HashSet<_>>();
        let integrable = valid
            .into_iter()
            .filter(|fact| !conflicting_fact_ids.contains(&fact.id))
            .collect();
        let integration = self.integrate_facts(integrable, &provider, session_id)?;
        let integrated = integration.integrated;
        {
            let mut pending_storage_changes = self.pending_storage_changes.write();
            for (fact, change_id) in integration.pending_review {
                pending_storage_changes.insert(fact.id, change_id);
                needs_review.push(fact);
            }
        }

        self.emit(SyncEvent::FactsIntegrated {
            count: integrated.len(),
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
            let remaining = self.config.batch_size.saturating_sub(facts.len());
            let extracted = self.pattern_extract(&turn.content, remaining)?;

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
    fn pattern_extract(&self, text: &str, max_facts: usize) -> anyhow::Result<Vec<StructuredFact>> {
        let mut facts = Vec::new();

        let patterns = sync_extraction_patterns()?;

        // Pattern: "X is a Y"
        for captures in patterns.is_a.captures_iter(text) {
            let (Some(name), Some(entity_type)) = (captures.get(1), captures.get(2)) else {
                continue;
            };
            if facts.len() >= max_facts {
                return Err(anyhow::anyhow!(
                    "extracted fact count exceeds configured batch_size"
                ));
            }
            facts.push(StructuredFact::Entity {
                entity_type: entity_type.as_str().to_string(),
                name: name.as_str().to_string(),
                attributes: std::collections::HashMap::new(),
            });
        }

        // Pattern: "X has Y of Z"
        for captures in patterns.has.captures_iter(text) {
            let (Some(name), Some(attribute), Some(value)) =
                (captures.get(1), captures.get(2), captures.get(3))
            else {
                continue;
            };
            let mut attrs = std::collections::HashMap::new();
            attrs.insert(attribute.as_str().to_string(), value.as_str().to_string());
            if facts.len() >= max_facts {
                return Err(anyhow::anyhow!(
                    "extracted fact count exceeds configured batch_size"
                ));
            }
            facts.push(StructuredFact::Entity {
                entity_type: "Unknown".to_string(),
                name: name.as_str().to_string(),
                attributes: attrs,
            });
        }

        // Pattern: "always/never/should X when Y"
        for captures in patterns.rule.captures_iter(text) {
            let (Some(action), Some(condition)) = (captures.get(2), captures.get(3)) else {
                continue;
            };
            if facts.len() >= max_facts {
                return Err(anyhow::anyhow!(
                    "extracted fact count exceeds configured batch_size"
                ));
            }
            facts.push(StructuredFact::TacitKnowledge {
                rule: format!("{} -> {}", condition.as_str(), action.as_str()),
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
                    .map(|(k, v)| format!("{k} = {v}"))
                    .collect();
                if attrs.is_empty() {
                    format!("{name} is a {entity_type}")
                } else {
                    format!("{name} is a {entity_type} with {}", attrs.join(", "))
                }
            }
            StructuredFact::Relation {
                rel_type,
                source,
                target,
                ..
            } => {
                format!("{source} {rel_type} {target}")
            }
            StructuredFact::Constraint {
                name, condition, ..
            } => {
                format!("Constraint {name}: {condition}")
            }
            StructuredFact::TacitKnowledge {
                rule,
                confidence,
                domain,
            } => {
                format!("[{domain}] {rule} (confidence: {:.0}%)", confidence * 100.0)
            }
        }
    }

    /// Validate extracted facts
    fn validate_facts(
        &self,
        facts: &[ExtractedFact],
    ) -> anyhow::Result<(Vec<ExtractedFact>, Vec<ExtractedFact>, Vec<ExtractedFact>)> {
        let mut valid = Vec::new();
        let invalid = Vec::new();
        let mut needs_review = Vec::new();

        for fact in facts {
            // Check schema validity
            let schema_valid = self.check_schema_validity(&fact.structured);

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
    fn check_schema_validity(&self, fact: &StructuredFact) -> bool {
        let schema = self.storage.schema();
        match fact {
            StructuredFact::Entity { entity_type, .. } => {
                schema.entity_types.iter().any(|known| known == entity_type)
            }
            StructuredFact::Relation { rel_type, .. } => {
                schema.relation_types.iter().any(|known| known == rel_type)
            }
            StructuredFact::Constraint { .. } | StructuredFact::TacitKnowledge { .. } => true,
        }
    }

    fn detect_conflicts(&self, facts: &[ExtractedFact]) -> anyhow::Result<Vec<Conflict>> {
        let db = self.storage.pathdb();
        let mut conflicts = Vec::new();

        for fact in facts {
            // Check for duplicate entities
            if let StructuredFact::Entity { entity_type, .. } = &fact.structured {
                // Look for existing entity with same name
                // Simplified - would use actual name lookup
                if let Some(existing) = db.find_by_type(entity_type.as_str()) {
                    if !existing.is_empty() {
                        // Potential duplicate
                        conflicts.push(Conflict {
                            new_fact: fact.clone(),
                            existing_facts: existing.iter().collect(),
                            conflict_type: ConflictType::AttributeMismatch,
                            suggested_resolution: Resolution::HumanReview,
                        });
                    }
                }
            }
        }

        Ok(conflicts)
    }

    /// Integrate validated facts into runtime evidence storage.
    fn integrate_facts(
        &self,
        mut facts: Vec<ExtractedFact>,
        provider: &LLMProvider,
        session_id: SessionId,
    ) -> anyhow::Result<FactIntegrationOutcome> {
        if facts.is_empty() {
            return Ok(FactIntegrationOutcome {
                integrated: Vec::new(),
                pending_review: Vec::new(),
            });
        }
        let storable = facts
            .iter()
            .map(|fact| {
                self.to_storable(&fact.structured).ok_or_else(|| {
                    anyhow::anyhow!(
                        "validated fact {} cannot be represented in storage",
                        fact.id
                    )
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let source = ChangeSource::LLMExtraction {
            session_id,
            model: format!("{provider:?}"),
            confidence: facts
                .iter()
                .map(|fact| fact.confidence)
                .fold(1.0_f32, f32::min),
        };
        let change_id = self.storage.add_facts(storable, source)?;
        let results = self.storage.flush()?;

        if let Some(result) = results.iter().find(|result| result.change_id == change_id) {
            for fact in &mut facts {
                fact.status = FactStatus::Integrated {
                    entity_ids: result.pathdb_ids.clone(),
                };
            }
            Ok(FactIntegrationOutcome {
                integrated: facts,
                pending_review: Vec::new(),
            })
        } else if self
            .storage
            .pending()
            .iter()
            .any(|change| change.id == change_id)
        {
            let pending = facts
                .into_iter()
                .map(|mut fact| {
                    fact.status = FactStatus::NeedsReview {
                        reason: "runtime evidence storage policy requires review".to_string(),
                    };
                    (fact, change_id)
                })
                .collect();
            Ok(FactIntegrationOutcome {
                integrated: Vec::new(),
                pending_review: pending,
            })
        } else {
            Err(anyhow::anyhow!(
                "storage change {change_id} was neither applied nor retained for review"
            ))
        }
    }

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
                    Uuid::new_v4()
                        .simple()
                        .to_string()
                        .chars()
                        .take(8)
                        .collect::<String>()
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

    /// Build bounded, explicitly evidence-plane grounding from process-local
    /// staged state. Schema names and constraints still come from the canonical
    /// `.axi` schema loaded by `UnifiedStorage`.
    pub fn build_grounding_context(
        &self,
        query: &str,
        max_facts: usize,
    ) -> anyhow::Result<GroundingContext> {
        let db = self.storage.pathdb();
        let schema_module = self.storage.schema();
        let mut context = crate::grounding::evidence_grounding_context_with_schema(
            &db,
            query,
            max_facts,
            &schema_module.entity_types,
            &schema_module.relation_types,
            &schema_module.constraints,
        )?;
        context.provenance =
            GroundingProvenanceV1::evidence("unified_storage_process_local_evidence");
        Ok(context)
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
        let fact = self
            .state
            .read()
            .pending_facts
            .iter()
            .find(|fact| fact.id == fact_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("pending fact {fact_id} was not found"))?;

        let mapped_change = self.pending_storage_changes.read().get(&fact_id).copied();
        let approved_fact_ids = if let Some(change_id) = mapped_change {
            let ids = self
                .pending_storage_changes
                .read()
                .iter()
                .filter_map(|(pending_fact_id, pending_change_id)| {
                    (*pending_change_id == change_id).then_some(*pending_fact_id)
                })
                .collect::<Vec<_>>();
            self.storage.approve_change(change_id)?;
            ids
        } else {
            let storable = self.to_storable(&fact.structured).ok_or_else(|| {
                anyhow::anyhow!("pending fact {fact_id} cannot be represented in storage")
            })?;
            let change_id = self
                .storage
                .add_facts(vec![storable], ChangeSource::UserEdit { user_id: None })?;
            self.storage.approve_change(change_id)?;
            vec![fact_id]
        };

        self.pending_storage_changes
            .write()
            .retain(|pending_fact_id, _| !approved_fact_ids.contains(pending_fact_id));
        let mut state = self.state.write();
        state
            .pending_facts
            .retain(|pending| !approved_fact_ids.contains(&pending.id));
        state.recent_integrations.extend(approved_fact_ids);
        state.graph_version += 1;
        Ok(())
    }

    pub fn reject_fact(&self, fact_id: FactId, reason: &str) -> anyhow::Result<()> {
        if reason.trim().is_empty() {
            return Err(anyhow::anyhow!("rejection reason must not be empty"));
        }
        if !self
            .state
            .read()
            .pending_facts
            .iter()
            .any(|fact| fact.id == fact_id)
        {
            return Err(anyhow::anyhow!("pending fact {fact_id} was not found"));
        }

        let mapped_change = self.pending_storage_changes.read().get(&fact_id).copied();
        let rejected_fact_ids = if let Some(change_id) = mapped_change {
            let ids = self
                .pending_storage_changes
                .read()
                .iter()
                .filter_map(|(pending_fact_id, pending_change_id)| {
                    (*pending_change_id == change_id).then_some(*pending_fact_id)
                })
                .collect::<Vec<_>>();
            self.storage.reject_change(change_id, reason)?;
            ids
        } else {
            vec![fact_id]
        };

        self.pending_storage_changes
            .write()
            .retain(|pending_fact_id, _| !rejected_fact_ids.contains(pending_fact_id));
        let mut state = self.state.write();
        let rejected = state
            .pending_facts
            .iter()
            .filter(|fact| rejected_fact_ids.contains(&fact.id))
            .cloned()
            .map(|mut fact| {
                fact.status = FactStatus::Rejected {
                    reason: reason.to_string(),
                };
                fact
            })
            .collect::<Vec<_>>();
        state
            .pending_facts
            .retain(|pending| !rejected_fact_ids.contains(&pending.id));
        state.rejected_facts.extend(rejected);
        state.graph_version += 1;
        Ok(())
    }

    pub fn resolve_conflict(
        &self,
        conflict_id: usize,
        resolution: Resolution,
    ) -> anyhow::Result<()> {
        let mut state = self.state.write();
        if conflict_id >= state.conflicts.len() {
            return Err(anyhow::anyhow!("conflict {conflict_id} was not found"));
        }

        match resolution {
            Resolution::KeepOld => {
                let conflict = state.conflicts.remove(conflict_id);
                let mut fact = conflict.new_fact;
                fact.status = FactStatus::Rejected {
                    reason: "kept existing runtime evidence during conflict resolution".to_string(),
                };
                state.rejected_facts.push(fact);
                Ok(())
            }
            Resolution::HumanReview => {
                let conflict = state.conflicts.remove(conflict_id);
                let mut fact = conflict.new_fact;
                fact.status = FactStatus::NeedsReview {
                    reason: "conflict requires explicit human review".to_string(),
                };
                state.pending_facts.push(fact);
                Ok(())
            }
            Resolution::ReplaceOld => Err(anyhow::anyhow!(
                "ReplaceOld is unsupported: runtime evidence storage cannot atomically replace the conflicting record"
            )),
            Resolution::Merge { .. } => Err(anyhow::anyhow!(
                "Merge is unsupported: no typed attribute-merge operation is implemented"
            )),
        }
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
    pub graph_version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_storage::{Change, ReviewPolicy, StorageConfig};
    use tempfile::tempdir;

    fn write_test_schema(dir: &tempfile::TempDir, objects: &[&str]) {
        let mut source = "module TestSchema\n\nschema S:\n".to_string();
        for object in objects {
            source.push_str(&format!("  object {object}\n"));
        }
        std::fs::write(dir.path().join("TestSchema.axi"), source).unwrap();
    }

    #[test]
    fn sync_config_rejects_legacy_or_unknown_fields() {
        let mut value = serde_json::to_value(SyncConfig::default()).unwrap();
        value["legacy_batch_limit"] = serde_json::json!(100);
        assert!(serde_json::from_value::<SyncConfig>(value).is_err());
    }

    #[test]
    fn sync_manager_rejects_non_finite_auto_integrate_threshold() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let error = SyncManager::new(
            storage,
            SyncConfig {
                auto_integrate_threshold: f32::NAN,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .err()
        .expect("non-finite sync threshold must fail closed");
        assert!(error.to_string().contains("auto_integrate_threshold"));
    }

    #[test]
    fn sync_manager_rejects_zero_batch_size() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let error = SyncManager::new(
            storage,
            SyncConfig {
                batch_size: 0,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .err()
        .expect("zero sync batch size must fail closed");
        assert!(error.to_string().contains("batch_size"));
    }

    #[test]
    fn sync_manager_rejects_unbounded_batch_size() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let error = SyncManager::new(
            storage,
            SyncConfig {
                batch_size: usize::MAX,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .err()
        .expect("unbounded sync batch size must fail closed");
        assert!(error.to_string().contains("batch_size"));
    }

    #[tokio::test]
    async fn sync_rejects_extracted_fact_batches_over_configured_limit() {
        let dir = tempdir().unwrap();
        write_test_schema(&dir, &["Material", "Tool"]);
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let manager = SyncManager::new(
            Arc::clone(&storage),
            SyncConfig {
                auto_integrate_threshold: 0.0,
                batch_size: 1,
                human_review_constraints: false,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .unwrap();
        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Titanium is a Material. Carbide is a Tool.".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];

        let error = manager
            .sync_from_conversation(&conversation, None)
            .await
            .expect_err("oversized extracted-fact batch must fail closed");

        assert!(error.to_string().contains("extracted fact count"));
        assert!(storage.pending().is_empty());
        assert!(storage.changelog().is_empty());
        assert!(manager.pending_review().is_empty());
    }

    #[tokio::test]
    async fn sync_rejects_excessive_conversation_turns_before_extraction() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let manager = SyncManager::new(
            storage,
            SyncConfig::default(),
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .unwrap();
        let conversation = (0..4097)
            .map(|_| ConversationTurn {
                role: crate::Role::User,
                content: String::new(),
                timestamp: Utc::now(),
                metadata: std::collections::HashMap::new(),
            })
            .collect::<Vec<_>>();

        let error = manager
            .sync_from_conversation(&conversation, None)
            .await
            .expect_err("excessive conversation turns must fail closed");
        assert!(error.to_string().contains("conversation turn count"));
    }

    #[tokio::test]
    async fn sync_rejects_excessive_conversation_bytes_before_extraction() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let manager = SyncManager::new(
            storage,
            SyncConfig::default(),
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .unwrap();
        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "x".repeat(8 * 1024 * 1024 + 1),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];

        let error = manager
            .sync_from_conversation(&conversation, None)
            .await
            .expect_err("excessive conversation bytes must fail closed");
        assert!(error.to_string().contains("conversation bytes"));
    }

    #[tokio::test]
    async fn test_sync_from_conversation() {
        let dir = tempdir().unwrap();
        write_test_schema(&dir, &["Material", "Tool"]);
        let config = StorageConfig {
            axi_dir: dir.path().to_path_buf(),
            require_review: ReviewPolicy {
                constraints: true,
                low_confidence_threshold: Some(0.95),
                schema_changes: true,
            },
            ..Default::default()
        };

        let storage = Arc::new(UnifiedStorage::new(config).unwrap());
        let sync_config = SyncConfig {
            auto_integrate_threshold: 0.0,
            human_review_constraints: false,
            ..Default::default()
        };
        let manager = SyncManager::new(
            Arc::clone(&storage),
            sync_config,
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .expect("valid sync configuration");

        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Titanium is a Material. Carbide is a Tool.".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];

        let result = manager
            .sync_from_conversation(&conversation, None)
            .await
            .unwrap();

        assert_eq!(result.integrated_count, 0);
        assert_eq!(result.pending_review, 2);
        assert!(storage.pathdb().find_by_type("Material").is_none());
        assert!(storage.pathdb().find_by_type("Tool").is_none());

        let fact_id = manager.pending_review()[0].id;
        manager.approve_fact(fact_id).unwrap();
        assert!(manager.pending_review().is_empty());
        assert!(storage.pathdb().find_by_type("Material").is_some());
        assert!(storage.pathdb().find_by_type("Tool").is_some());
    }

    #[tokio::test]
    async fn storage_review_change_is_rejected_with_pending_fact() {
        let dir = tempdir().unwrap();
        write_test_schema(&dir, &["Material"]);
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                require_review: ReviewPolicy {
                    constraints: true,
                    low_confidence_threshold: Some(0.95),
                    schema_changes: true,
                },
                ..Default::default()
            })
            .unwrap(),
        );
        let manager = SyncManager::new(
            Arc::clone(&storage),
            SyncConfig {
                auto_integrate_threshold: 0.0,
                human_review_constraints: false,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .expect("valid sync configuration");
        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Titanium is a Material with hardness of 36".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];
        manager
            .sync_from_conversation(&conversation, None)
            .await
            .unwrap();

        let fact_id = manager.pending_review()[0].id;
        manager
            .reject_fact(fact_id, "unsupported evidence")
            .unwrap();

        assert!(manager.pending_review().is_empty());
        assert!(storage.pending().is_empty());
        assert!(storage.pathdb().find_by_type("Material").is_none());
        assert!(matches!(
            storage.changelog().as_slice(),
            [Change {
                status: axiograph_storage::ChangeStatus::Rejected { reason },
                ..
            }] if reason == "unsupported evidence"
        ));
    }

    #[tokio::test]
    async fn sync_validation_uses_loaded_schema_types() {
        let dir = tempdir().unwrap();
        write_test_schema(&dir, &["Widget"]);
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                ..Default::default()
            })
            .unwrap(),
        );
        let manager = SyncManager::new(
            Arc::clone(&storage),
            SyncConfig {
                auto_integrate_threshold: 0.0,
                human_review_constraints: false,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .expect("valid sync configuration");
        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Bolt is a Widget.".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];

        let result = manager
            .sync_from_conversation(&conversation, None)
            .await
            .unwrap();

        assert_eq!(result.integrated_count, 1);
        assert_eq!(result.pending_review, 0);
        assert!(storage.pathdb().find_by_type("Widget").is_some());
    }

    #[tokio::test]
    async fn conflicting_fact_is_not_materialized() {
        let dir = tempdir().unwrap();
        write_test_schema(&dir, &["Material"]);
        let storage = Arc::new(
            UnifiedStorage::new(StorageConfig {
                axi_dir: dir.path().to_path_buf(),
                require_review: ReviewPolicy {
                    constraints: false,
                    low_confidence_threshold: None,
                    schema_changes: false,
                },
                ..Default::default()
            })
            .unwrap(),
        );
        storage
            .add_facts(
                vec![StorableFact::Entity {
                    name: "Titanium".to_string(),
                    entity_type: "Material".to_string(),
                    attributes: Vec::new(),
                }],
                ChangeSource::UserEdit { user_id: None },
            )
            .unwrap();
        storage.flush().unwrap();
        let manager = SyncManager::new(
            Arc::clone(&storage),
            SyncConfig {
                auto_integrate_threshold: 0.0,
                human_review_constraints: false,
                ..Default::default()
            },
            LLMProvider::Custom {
                name: "test".to_string(),
                endpoint: "http://localhost".to_string(),
            },
        )
        .expect("valid sync configuration");
        let conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Aluminum is a Material.".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];

        let result = manager
            .sync_from_conversation(&conversation, None)
            .await
            .unwrap();

        assert_eq!(result.integrated_count, 0);
        assert_eq!(result.conflicts, 1);
        assert!(matches!(
            manager.unresolved_conflicts()[0].suggested_resolution,
            Resolution::HumanReview
        ));
        assert_eq!(
            storage
                .pathdb()
                .find_by_type("Material")
                .expect("preloaded Material")
                .len(),
            1
        );

        let error = manager
            .resolve_conflict(0, Resolution::ReplaceOld)
            .expect_err("unsupported replacement must preserve the conflict");
        assert!(error.to_string().contains("ReplaceOld"));
        assert_eq!(manager.unresolved_conflicts().len(), 1);
        assert_eq!(
            storage
                .pathdb()
                .find_by_type("Material")
                .expect("preloaded Material")
                .len(),
            1
        );

        manager
            .resolve_conflict(0, Resolution::HumanReview)
            .unwrap();
        assert!(manager.unresolved_conflicts().is_empty());
        assert!(matches!(
            manager.pending_review().as_slice(),
            [ExtractedFact {
                status: FactStatus::NeedsReview { reason },
                ..
            }] if reason.contains("conflict")
        ));

        let second_conversation = vec![ConversationTurn {
            role: crate::Role::User,
            content: "Copper is a Material.".to_string(),
            timestamp: Utc::now(),
            metadata: std::collections::HashMap::new(),
        }];
        manager
            .sync_from_conversation(&second_conversation, None)
            .await
            .unwrap();
        manager.resolve_conflict(0, Resolution::KeepOld).unwrap();
        assert!(manager.unresolved_conflicts().is_empty());
        assert!(matches!(
            manager.state().rejected_facts.as_slice(),
            [ExtractedFact {
                status: FactStatus::Rejected { reason },
                ..
            }] if reason.contains("kept existing")
        ));
    }
}
