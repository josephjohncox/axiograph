//! Axiograph LLM Sync: Typed Grounding And Evidence-Plane LLM Integration
//!
//! This crate turns conversations and model output into evidence-plane facts,
//! grounding contexts, reconciliation inputs, and review material. It also
//! exposes an accepted-derived retrieval type that can be constructed only from
//! receipt-checked `MaterializedPathDb`. Accepted ontology meaning still comes
//! from canonical `.axi`, compiled IR, typed review, semantic VCS, and optional
//! Lean certificates.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────────┐
//! │                    LLM ↔ EVIDENCE/REVIEW PIPELINE                       │
//! ├──────────────────────────────────────────────────────────────────────────┤
//! │                                                                          │
//! │  ┌───────────┐                                        ┌───────────────┐  │
//! │  │    LLM    │◄──────── Grounding Context ───────────│  Runtime      │  │
//! │  │ (Claude,  │                                        │  Storage      │  │
//! │  │  GPT-4,   │──────── Evidence Facts ───────────────►│               │  │
//! │  │  Local)   │                                        │  ┌─────────┐  │  │
//! │  └───────────┘                                        │  │Evidence │  │  │
//! │       ▲                                               │  │records  │  │  │
//! │       │                                               │  └─────────┘  │  │
//! │   Conversation                                        │       ▲       │  │
//! │       │                                               │       │cache  │  │
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
//! ## Direction 1: Axiograph → LLM (Query/Grounding)
//! - Semantic query interface for LLMs
//! - Structured facts with provenance
//! - Grounded generation with citations
//!
//! ## Direction 2: LLM → Evidence/Review (Generation)
//! - Fact extraction from conversations
//! - Schema-aware candidate creation
//! - Confidence-tracked knowledge addition
//! - Review and reconciliation before accepted ontology mutation
//!
//! ## Tooling Surface
//! - Evidence extraction and grounding records
//! - Reconciliation inputs for review
//! - Human-in-the-loop promotion through Axiograph semantic workflows

#![allow(dead_code)]

pub mod extraction;
pub mod format;
pub mod grounding;
pub mod llm;
pub mod path_optimized;
pub mod path_verification;
pub mod probabilistic;
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
        entity_ids: Vec<u32>,
    },
    /// Rejected
    Rejected { reason: String },
}

// ============================================================================
// Grounding Context (KG → LLM)
// ============================================================================

pub const GROUNDING_PROVENANCE_VERSION_V1: u32 = 1;
pub const ACCEPTED_GROUNDING_PROVENANCE_VERSION_V1: u32 = 1;

/// Authority plane of context supplied to an LLM.
///
/// There is intentionally no accepted-plane variant. Process-local PathDB and
/// evidence state cannot grant accepted or certificate-backed authority.
/// Accepted-derived retrieval uses the separate output-only
/// `AcceptedGroundingContext` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroundingPlaneV1 {
    Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundingProvenanceV1 {
    pub version: u32,
    pub plane: GroundingPlaneV1,
    pub source: String,
}

impl GroundingProvenanceV1 {
    pub fn evidence(source: impl Into<String>) -> Self {
        Self {
            version: GROUNDING_PROVENANCE_VERSION_V1,
            plane: GroundingPlaneV1::Evidence,
            source: source.into(),
        }
    }
}

/// Evidence-plane context provided to an LLM from derived graph state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroundingContext {
    /// Explicit non-authoritative provenance for every evidence grounding path.
    pub provenance: GroundingProvenanceV1,
    /// Relevant facts from KG
    pub facts: Vec<GroundedFact>,
    /// Schema information
    pub schema_context: Option<SchemaContext>,
    /// Guardrails that apply
    pub active_guardrails: Vec<GuardrailContext>,
    /// Suggested queries for follow-up
    pub suggested_queries: Vec<String>,
    /// True when any request work or output budget can have omitted context.
    pub truncated: bool,
    /// Deterministic names of every budget that can have omitted context.
    pub truncation_reasons: Vec<String>,
}

/// Authority plane for grounding derived exclusively from an authenticated
/// accepted AxiStore materialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptedGroundingPlaneV1 {
    AcceptedDerived,
}

/// Non-forgeable provenance for accepted-derived grounding.
///
/// This type is output-only and its fields are private. It can be constructed
/// only inside this crate from `MaterializedPathDb`, whose loader has already
/// authenticated the exact AxiStore receipt and accepted manifest closure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptedGroundingProvenanceV1 {
    version: u32,
    plane: AcceptedGroundingPlaneV1,
    source: String,
    repository_id: String,
    accepted_snapshot_id: String,
    accepted_tree_id: String,
    ordered_module_closure: Vec<String>,
    kernel_ir_digest: String,
    canonical_fact_log_digest: String,
    materialization_id: String,
    logical_digest: String,
    exact_image_digest: String,
    query_digest: String,
    selection_digest: String,
}

impl AcceptedGroundingProvenanceV1 {
    pub(crate) fn from_materialized_pathdb(
        materialized: &axiograph_pathdb::materialization::MaterializedPathDb,
        query_digest: &axiograph_kernel::ObjectBlobIdV2,
        selection_digest: &axiograph_kernel::ObjectBlobIdV2,
    ) -> Self {
        let receipt = materialized.receipt();
        Self {
            version: ACCEPTED_GROUNDING_PROVENANCE_VERSION_V1,
            plane: AcceptedGroundingPlaneV1::AcceptedDerived,
            source: "axi_store_verified_axpd".to_string(),
            repository_id: receipt.anchors.repository_id.to_string(),
            accepted_snapshot_id: receipt.anchors.accepted_snapshot_id.to_string(),
            accepted_tree_id: receipt.anchors.accepted_tree_id.to_string(),
            ordered_module_closure: receipt
                .anchors
                .ordered_module_closure
                .iter()
                .map(ToString::to_string)
                .collect(),
            kernel_ir_digest: receipt.anchors.kernel_ir_digest.to_string(),
            canonical_fact_log_digest: receipt.anchors.canonical_fact_log_digest.to_string(),
            materialization_id: receipt.materialization_id.to_string(),
            logical_digest: receipt.logical_digest.to_string(),
            exact_image_digest: receipt.exact_image_digest.to_string(),
            query_digest: query_digest.to_string(),
            selection_digest: selection_digest.to_string(),
        }
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn plane(&self) -> AcceptedGroundingPlaneV1 {
        self.plane
    }

    pub fn repository_id(&self) -> &str {
        &self.repository_id
    }

    pub fn accepted_snapshot_id(&self) -> &str {
        &self.accepted_snapshot_id
    }

    pub fn accepted_tree_id(&self) -> &str {
        &self.accepted_tree_id
    }

    pub fn ordered_module_closure(&self) -> &[String] {
        &self.ordered_module_closure
    }

    pub fn materialization_id(&self) -> &str {
        &self.materialization_id
    }

    pub fn query_digest(&self) -> &str {
        &self.query_digest
    }

    pub fn selection_digest(&self) -> &str {
        &self.selection_digest
    }
}

/// One accepted entity selected from an authenticated materialization.
/// Runtime row ids and confidence scores are intentionally absent: neither is
/// an accepted semantic identity or a proof of equality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptedGroundedFact {
    stable_id: String,
    natural: String,
    structured: String,
    citation: Vec<String>,
    related: Vec<String>,
}

impl AcceptedGroundedFact {
    pub(crate) fn new(
        stable_id: String,
        natural: String,
        structured: String,
        citation: Vec<String>,
        related: Vec<String>,
    ) -> Self {
        Self {
            stable_id,
            natural,
            structured,
            citation,
            related,
        }
    }

    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }

    pub fn natural(&self) -> &str {
        &self.natural
    }

    pub fn structured(&self) -> &str {
        &self.structured
    }

    pub fn citation(&self) -> &[String] {
        &self.citation
    }

    pub fn related(&self) -> &[String] {
        &self.related
    }
}

/// Grounding payload whose source rows and accepted-state anchors have been
/// authenticated by AxiStore and `MaterializedPathDb`.
///
/// The payload is output-only. Callers cannot deserialize or construct one and
/// thereby relabel evidence-plane data as accepted-derived.
///
/// ```compile_fail
/// use axiograph_llm_sync::AcceptedGroundingContext;
/// let _: AcceptedGroundingContext = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct AcceptedGroundingContext {
    provenance: AcceptedGroundingProvenanceV1,
    facts: Vec<AcceptedGroundedFact>,
    schema_context: SchemaContext,
    truncated: bool,
    truncation_reasons: Vec<String>,
    non_claims: Vec<String>,
}

impl AcceptedGroundingContext {
    pub(crate) fn new(
        provenance: AcceptedGroundingProvenanceV1,
        facts: Vec<AcceptedGroundedFact>,
        schema_context: SchemaContext,
        truncation_reasons: Vec<String>,
    ) -> Self {
        Self {
            provenance,
            facts,
            schema_context,
            truncated: !truncation_reasons.is_empty(),
            truncation_reasons,
            non_claims: vec![
                "lexical grounding selection is not an entailment or completeness proof"
                    .to_string(),
                "accepted-derived source rows do not certify downstream LLM output".to_string(),
                "schema context lists materialized runtime types and relations; it is not a complete canonical constraint report".to_string(),
                "confidence arithmetic does not establish path or categorical equality".to_string(),
            ],
        }
    }

    pub fn provenance(&self) -> &AcceptedGroundingProvenanceV1 {
        &self.provenance
    }

    pub fn facts(&self) -> &[AcceptedGroundedFact] {
        &self.facts
    }

    pub fn schema_context(&self) -> &SchemaContext {
        &self.schema_context
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn truncation_reasons(&self) -> &[String] {
        &self.truncation_reasons
    }

    pub fn non_claims(&self) -> &[String] {
        &self.non_claims
    }
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
    /// Facts explicitly rejected during review.
    pub rejected_facts: Vec<ExtractedFact>,
    /// Conflicts requiring resolution
    pub conflicts: Vec<Conflict>,
    /// Version of the graph at last sync.
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
#[serde(deny_unknown_fields)]
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
