//! Axiograph LLM Sync: Bidirectional Knowledge Graph ↔ LLM Integration
//!
//! This crate provides two-way synchronization between Axiograph knowledge graphs
//! and Large Language Models, with unified storage to both `.axi` files and PathDB.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────────┐
//! │                       LLM ↔ KG SYNC PIPELINE                             │
//! ├──────────────────────────────────────────────────────────────────────────┤
//! │                                                                          │
//! │  ┌───────────┐                                        ┌───────────────┐  │
//! │  │    LLM    │◄──────── Grounding Context ───────────│  Unified      │  │
//! │  │ (Claude,  │                                        │  Storage      │  │
//! │  │  GPT-4,   │──────── Extracted Facts ──────────────►│               │  │
//! │  │  Local)   │                                        │  ┌─────────┐  │  │
//! │  └───────────┘                                        │  │  .axi   │  │  │
//! │       ▲                                               │  │  files  │  │  │
//! │       │                                               │  └─────────┘  │  │
//! │   Conversation                                        │       ▲       │  │
//! │       │                                               │       │sync   │  │
//! │  ┌────▼────┐     ┌───────────┐     ┌───────────┐     │  ┌────▼────┐  │  │
//! │  │  User   │────►│ Extractor │────►│ Validator │────►│  │ PathDB  │  │  │
//! │  └─────────┘     └───────────┘     └───────────┘     │  │ (binary)│  │  │
//! │                                           │          │  └─────────┘  │  │
//! │                                      Conflicts       └───────────────┘  │
//! │                                           │                              │
//! │                                      ┌────▼────┐                        │
//! │                                      │ Resolver│                        │
//! │                                      └─────────┘                        │
//! │                                                                          │
//! └──────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Direction 1: KG → LLM (Query/Grounding)
//! - Semantic query interface for LLMs
//! - Structured facts with provenance
//! - Grounded generation with citations
//!
//! ## Direction 2: LLM → KG (Generation/Update)
//! - Fact extraction from conversations
//! - Schema-validated entity creation
//! - Confidence-tracked knowledge addition
//! - **Writes to both .axi files and PathDB**
//!
//! ## Sync Protocol
//! - Incremental updates with conflict resolution
//! - Version tracking for rollback
//! - Human-in-the-loop for critical changes

#![allow(dead_code)]

pub mod extraction;
pub mod format;
pub mod grounding;
pub mod llm;
pub mod path_optimized;
pub mod path_verification;
pub mod probabilistic;
pub mod protocol;
pub mod providers;
pub mod reconciliation;
pub mod reconciliation_format;
pub mod sync;

use axiograph_pathdb::PathDB;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Re-export storage for convenience
pub use axiograph_storage::{
    Change, ChangeSource, ChangeStatus, StorableFact, StorageConfig, UnifiedStorage,
};

// ============================================================================
// Core Types
// ============================================================================

/// Unique identifier for a sync session
pub type SessionId = Uuid;

/// Unique identifier for a fact/claim
pub type FactId = Uuid;

/// LLM provider identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LLMProvider {
    OpenAI { model: String },
    Anthropic { model: String },
    Local { model_path: String },
    Custom { name: String, endpoint: String },
}

/// A conversation turn
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    System,
}

/// A fact extracted from LLM conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    pub id: FactId,
    /// The claim in natural language
    pub claim: String,
    /// Structured representation
    pub structured: StructuredFact,
    /// Confidence from extraction
    pub confidence: f32,
    /// Source conversation
    pub source: FactSource,
    /// Validation status
    pub status: FactStatus,
}

/// Structured representation of a fact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StructuredFact {
    /// Entity definition
    Entity {
        entity_type: String,
        name: String,
        attributes: HashMap<String, String>,
    },
    /// Relation between entities
    Relation {
        rel_type: String,
        source: String,
        target: String,
        attributes: HashMap<String, String>,
    },
    /// Constraint or rule
    Constraint {
        name: String,
        condition: String,
        severity: String,
    },
    /// Tacit knowledge (probabilistic)
    TacitKnowledge {
        rule: String,
        confidence: f32,
        domain: String,
    },
}

impl StructuredFact {
    pub fn type_name(&self) -> String {
        match self {
            StructuredFact::Entity { entity_type, .. } => entity_type.clone(),
            StructuredFact::Relation { rel_type, .. } => rel_type.clone(),
            StructuredFact::Constraint { .. } => "Constraint".to_string(),
            StructuredFact::TacitKnowledge { .. } => "TacitKnowledge".to_string(),
        }
    }
}

/// Source of a fact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactSource {
    pub session_id: SessionId,
    pub provider: LLMProvider,
    pub conversation_turns: Vec<usize>, // Indices into conversation
    pub extraction_timestamp: DateTime<Utc>,
    pub human_verified: bool,
}

/// Status of a fact in the sync pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactStatus {
    /// Just extracted, not yet validated
    Pending,
    /// Passed schema validation
    Validated,
    /// Conflicts with existing knowledge
    Conflicting { conflicts_with: Vec<FactId> },
    /// Requires human review
    NeedsReview { reason: String },
    /// Approved and integrated
    Integrated {
        /// Entity IDs assigned by the runtime store (PathDB).
        #[serde(alias = "kg_ids")]
        entity_ids: Vec<u32>,
    },
    /// Rejected
    Rejected { reason: String },
}

// ============================================================================
// Grounding Context (KG → LLM)
// ============================================================================

/// Context provided to LLM from knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingContext {
    /// Relevant facts from KG
    pub facts: Vec<GroundedFact>,
    /// Schema information
    pub schema_context: Option<SchemaContext>,
    /// Guardrails that apply
    pub active_guardrails: Vec<GuardrailContext>,
    /// Suggested queries for follow-up
    pub suggested_queries: Vec<String>,
}

/// A fact from KG formatted for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundedFact {
    pub id: u32,
    /// Natural language representation
    pub natural: String,
    /// Structured representation (for precise reference)
    pub structured: String,
    /// Confidence
    pub confidence: f32,
    /// Citation path in KG
    pub citation: Vec<String>,
    /// Related concepts
    pub related: Vec<String>,
}

/// Schema context for LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaContext {
    pub entity_types: Vec<String>,
    pub relation_types: Vec<String>,
    pub constraints: Vec<String>,
}

/// Guardrail context for LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailContext {
    pub rule_id: String,
    pub severity: String,
    pub description: String,
    pub applies_when: String,
}

// ============================================================================
// Sync State
// ============================================================================

/// State of the sync between LLM and KG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    /// Current session
    pub session_id: SessionId,
    /// Last sync timestamp
    pub last_sync: DateTime<Utc>,
    /// Pending facts to integrate
    pub pending_facts: Vec<ExtractedFact>,
    /// Recently integrated facts
    pub recent_integrations: Vec<FactId>,
    /// Conflicts requiring resolution
    pub conflicts: Vec<Conflict>,
    /// Version of the graph at last sync.
    #[serde(alias = "kg_version")]
    pub graph_version: u64,
}

/// A conflict between extracted fact and existing knowledge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub new_fact: ExtractedFact,
    pub existing_facts: Vec<u32>,
    pub conflict_type: ConflictType,
    pub suggested_resolution: Resolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConflictType {
    /// Direct contradiction
    Contradiction,
    /// Same entity, different attributes
    AttributeMismatch,
    /// Confidence disagreement
    ConfidenceConflict,
    /// Schema violation
    SchemaViolation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Resolution {
    /// Keep new, discard old
    ReplaceOld,
    /// Keep old, discard new
    KeepOld,
    /// Merge with weighted average
    Merge { weights: (f32, f32) },
    /// Human decision required
    HumanReview,
}

// ============================================================================
// Main Sync Engine
// ============================================================================

/// The main LLM-KG sync engine
pub struct LLMSyncEngine {
    /// The knowledge graph
    pathdb: PathDB,
    /// Current sync state
    state: SyncState,
    /// Extraction pipeline
    extractor: Box<dyn FactExtractor>,
    /// Validation pipeline
    validator: Box<dyn FactValidator>,
    /// Conflict resolver
    resolver: Box<dyn ConflictResolver>,
    /// Configuration
    config: SyncConfig,
}

/// Configuration for sync behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Minimum confidence to auto-integrate
    pub auto_integrate_threshold: f32,
    /// Maximum facts to batch before sync
    pub batch_size: usize,
    /// Require human review for constraint changes
    pub human_review_constraints: bool,
    /// Track provenance in KG
    pub track_provenance: bool,
    /// Enable conflict auto-resolution
    pub auto_resolve_conflicts: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            auto_integrate_threshold: 0.9,
            batch_size: 100,
            human_review_constraints: true,
            track_provenance: true,
            auto_resolve_conflicts: false,
        }
    }
}

// ============================================================================
// Traits for Extensibility
// ============================================================================

/// Extracts facts from LLM conversations
#[async_trait::async_trait]
pub trait FactExtractor: Send + Sync {
    /// Extract facts from a conversation
    async fn extract(
        &self,
        conversation: &[ConversationTurn],
    ) -> anyhow::Result<Vec<ExtractedFact>>;

    /// Extract facts from a single message
    async fn extract_from_message(&self, message: &str) -> anyhow::Result<Vec<ExtractedFact>>;
}

/// Validates facts against schema and existing knowledge
pub trait FactValidator: Send + Sync {
    /// Validate a fact against schema
    fn validate_schema(&self, fact: &ExtractedFact, pathdb: &PathDB) -> ValidationResult;

    /// Check for conflicts with existing knowledge
    fn check_conflicts(&self, fact: &ExtractedFact, pathdb: &PathDB) -> Vec<Conflict>;
}

/// Resolves conflicts between facts
pub trait ConflictResolver: Send + Sync {
    /// Suggest resolution for a conflict
    fn suggest_resolution(&self, conflict: &Conflict, pathdb: &PathDB) -> Resolution;

    /// Apply a resolution
    fn apply_resolution(
        &self,
        conflict: &Conflict,
        resolution: &Resolution,
        pathdb: &mut PathDB,
    ) -> anyhow::Result<()>;
}

/// Result of schema validation
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Valid,
    Invalid { errors: Vec<String> },
    NeedsSchemaExtension { suggestions: Vec<String> },
}

// ============================================================================
// LLM Interface Trait
// ============================================================================

/// Interface for LLM providers
#[async_trait::async_trait]
pub trait LLMInterface: Send + Sync {
    /// Generate response with grounding context
    async fn generate_grounded(
        &self,
        prompt: &str,
        context: &GroundingContext,
    ) -> anyhow::Result<String>;

    /// Extract structured facts from text
    async fn extract_facts(
        &self,
        text: &str,
        schema: &SchemaContext,
    ) -> anyhow::Result<Vec<StructuredFact>>;

    /// Validate a claim against knowledge
    async fn validate_claim(
        &self,
        claim: &str,
        evidence: &[GroundedFact],
    ) -> anyhow::Result<(bool, f32, String)>; // (valid, confidence, reasoning)
}

// ============================================================================
// Re-exports
// ============================================================================

pub use reconciliation::{
    Evidence, EvidenceType, ReconciliationAction, ReconciliationConfig, ReconciliationEngine,
    ReconciliationResult, ResolvedConflict, SourceCredibility, TrackRecord, Weight, WeightedFact,
};
pub use reconciliation_format::ReconciliationState;
pub use sync::{SyncEvent, SyncManager, SyncResult, SyncStats};
