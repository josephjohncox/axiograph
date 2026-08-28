//! Axiograph Runtime Evidence Storage
//!
//! Provides a runtime store for evidence facts, materialized caches, and
//! grounding data. Accepted ontology meaning is not stored here; it remains
//! canonical `.axi` plus compiled IR, semantic VCS review, and optional Lean
//! certificates.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                 RUNTIME EVIDENCE STORAGE                            │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │                                                                     │
//! │  ┌─────────┐     ┌───────────────┐     ┌─────────────┐             │
//! │  │   LLM   │────►│               │────►│ Evidence    │             │
//! │  │  Sync   │     │   Runtime     │     │ records     │             │
//! │  └─────────┘     │   Storage     │     └─────────────┘             │
//! │                  │   Manager     │                                  │
//! │  ┌─────────┐     │               │     ┌─────────────┐             │
//! │  │ Tools   │────►│               │────►│   PathDB    │             │
//! │  │/Review  │     │               │     │  cache      │             │
//! │  └─────────┘     └───────────────┘     └─────────────┘             │
//! │                         │                                           │
//! │                         ▼                                           │
//! │                  ┌─────────────┐                                    │
//! │                  │  Change Log │                                    │
//! │                  │  (versioned)│                                    │
//! │                  └─────────────┘                                    │
//! │                                                                     │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Key Features
//!
//! - **Evidence-first**: Records candidate facts and review material.
//! - **Materialized**: Maintains a PathDB cache for local query/grounding.
//! - **Versioned**: Keeps an audit log for evidence changes.
//! - **Fail-closed**: Rejects unresolved or untyped relation endpoints.
#![allow(unused_variables)]

#[cfg(test)]
mod tests;

use axiograph_dsl as dsl;
use axiograph_pathdb::PathDB;
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Core Types
// ============================================================================

/// Unique identifier for a storage change
pub type ChangeId = Uuid;

const STORAGE_ENTITY_NAME_ATTR: &str = "name";

/// Endpoint side for relation diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationEndpointRole {
    Source,
    Target,
}

impl fmt::Display for RelationEndpointRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source => write!(f, "source"),
            Self::Target => write!(f, "target"),
        }
    }
}

/// Fail-closed semantic errors emitted before writing unresolved relations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageSemanticError {
    UnresolvedRelationEndpoint {
        relation_name: Option<String>,
        rel_type: String,
        endpoint: RelationEndpointRole,
        entity_name: String,
    },
    AmbiguousRelationEndpoint {
        relation_name: Option<String>,
        rel_type: String,
        endpoint: RelationEndpointRole,
        entity_name: String,
        candidate_ids: Vec<u32>,
    },
    UntypedRelationEndpoint {
        relation_name: Option<String>,
        rel_type: String,
        endpoint: RelationEndpointRole,
        entity_name: String,
        entity_id: u32,
    },
}

impl fmt::Display for StorageSemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnresolvedRelationEndpoint {
                relation_name,
                rel_type,
                endpoint,
                entity_name,
            } => write!(
                f,
                "unresolved {endpoint} endpoint `{entity_name}` for relation `{}` of type `{rel_type}`; relation endpoints must resolve to exactly one typed PathDB entity by `{STORAGE_ENTITY_NAME_ATTR}`",
                relation_name.as_deref().unwrap_or(rel_type)
            ),
            Self::AmbiguousRelationEndpoint {
                relation_name,
                rel_type,
                endpoint,
                entity_name,
                candidate_ids,
            } => write!(
                f,
                "ambiguous {endpoint} endpoint `{entity_name}` for relation `{}` of type `{rel_type}`; candidates: {candidate_ids:?}",
                relation_name.as_deref().unwrap_or(rel_type)
            ),
            Self::UntypedRelationEndpoint {
                relation_name,
                rel_type,
                endpoint,
                entity_name,
                entity_id,
            } => write!(
                f,
                "untyped {endpoint} endpoint `{entity_name}` for relation `{}` of type `{rel_type}` resolved to entity id {entity_id}",
                relation_name.as_deref().unwrap_or(rel_type)
            ),
        }
    }
}

impl Error for StorageSemanticError {}

/// A storable fact (can come from LLM, user, or file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorableFact {
    /// Entity definition
    Entity {
        name: String,
        entity_type: String,
        attributes: Vec<(String, String)>,
    },
    /// Relation between entities
    Relation {
        name: Option<String>,
        rel_type: String,
        source: String,
        target: String,
        confidence: f32,
        attributes: Vec<(String, String)>,
    },
    /// Constraint/rule
    Constraint {
        name: String,
        condition: String,
        severity: String,
        message: Option<String>,
    },
    /// Tacit knowledge (probabilistic rule)
    TacitKnowledge {
        name: String,
        rule: String,
        confidence: f32,
        domain: String,
        source: String,
    },
    /// Concept (for learning)
    Concept {
        name: String,
        description: String,
        difficulty: String,
        prerequisites: Vec<String>,
    },
    /// Safety guideline
    SafetyGuideline {
        name: String,
        title: String,
        severity: String,
        explanation: String,
    },
}

/// Source of a change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeSource {
    /// From LLM extraction
    LLMExtraction {
        session_id: Uuid,
        model: String,
        confidence: f32,
    },
    /// From user edit
    UserEdit { user_id: Option<String> },
    /// From file import
    FileImport { path: PathBuf },
    /// From API call
    API { client_id: String },
    /// System-generated (e.g., inference)
    System { reason: String },
}

/// A change to the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub id: ChangeId,
    pub timestamp: DateTime<Utc>,
    pub source: ChangeSource,
    pub facts: Vec<StorableFact>,
    pub status: ChangeStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeStatus {
    Pending,
    Applied,
    Rejected { reason: String },
    Rolled { reason: String },
}

/// Result of applying a change
#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub change_id: ChangeId,
    /// Entity IDs created in PathDB
    pub pathdb_ids: Vec<u32>,
    /// Proposed canonical `.axi` fragments retained for review.
    pub axi_lines: Vec<String>,
    /// Any warnings
    pub warnings: Vec<String>,
}

/// A lightweight "schema context" extracted from `.axi` files.
///
/// This is intentionally lossy: it is meant for quick validation/LLM grounding
/// (e.g. "these entity types exist") rather than as a full AST.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AxiSchemaIndex {
    pub entity_types: Vec<String>,
    pub relation_types: Vec<String>,
    pub constraints: Vec<String>,
}

// ============================================================================
// Storage Configuration
// ============================================================================

/// Configuration for runtime evidence storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Directory containing read-only schema inputs.
    pub axi_dir: PathBuf,
    /// Auto-sync on file changes
    pub watch_files: bool,
    /// Require human review for certain changes
    pub require_review: ReviewPolicy,
    /// Hard maximum queued changes before additions fail closed.
    pub max_pending: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPolicy {
    /// Review constraints/rules
    pub constraints: bool,
    /// Review low-confidence facts
    pub low_confidence_threshold: Option<f32>,
    /// Review schema extensions
    pub schema_changes: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            axi_dir: PathBuf::from("./knowledge"),
            watch_files: true,
            require_review: ReviewPolicy {
                constraints: true,
                low_confidence_threshold: Some(0.7),
                schema_changes: true,
            },
            max_pending: 100,
        }
    }
}

// ============================================================================
// Runtime Evidence Storage Manager
// ============================================================================

/// The main storage manager
pub struct UnifiedStorage {
    /// Configuration
    config: StorageConfig,
    /// PathDB instance
    pathdb: Arc<RwLock<PathDB>>,
    /// Pending changes
    pending: Arc<RwLock<Vec<Change>>>,
    /// Change log
    changelog: Arc<RwLock<Vec<Change>>>,
    /// Current schema index (loaded from `.axi` files)
    schema: Arc<RwLock<AxiSchemaIndex>>,
    /// Serializes state transitions spanning pending, PathDB, and changelog.
    mutation_lock: Arc<Mutex<()>>,
}

impl UnifiedStorage {
    /// Create an in-memory evidence staging manager.
    ///
    /// This type deliberately performs no durable writes. Accepted history and
    /// authenticated `.axpd` materializations belong to `axiograph_store::AxiStore`.
    pub fn new(config: StorageConfig) -> anyhow::Result<Self> {
        if config.max_pending == 0 {
            return Err(anyhow::anyhow!("max_pending must be greater than zero"));
        }
        if let Some(threshold) = config.require_review.low_confidence_threshold {
            if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
                return Err(anyhow::anyhow!(
                    "low_confidence_threshold must be a finite probability in [0, 1]"
                ));
            }
        }
        let schema = Self::load_axi_files(&config.axi_dir)?;
        Ok(Self {
            config,
            pathdb: Arc::new(RwLock::new(PathDB::new())),
            pending: Arc::new(RwLock::new(Vec::new())),
            changelog: Arc::new(RwLock::new(Vec::new())),
            schema: Arc::new(RwLock::new(schema)),
            mutation_lock: Arc::new(Mutex::new(())),
        })
    }

    fn schema_constraint_display(constraint: &dsl::schema_v1::ConstraintV1) -> String {
        use dsl::schema_v1::ConstraintV1;
        match constraint {
            ConstraintV1::Functional {
                relation,
                src_field,
                dst_field,
            } => format!("functional {relation} ({src_field} -> {dst_field})"),
            ConstraintV1::AtMost {
                relation,
                src_field,
                dst_field,
                max,
                params,
            } => {
                let mut s = format!("at_most {max} {relation} ({src_field} -> {dst_field})");
                if let Some(ps) = params.as_ref() {
                    if !ps.is_empty() {
                        s.push_str(&format!(" param ({})", ps.join(", ")));
                    }
                }
                s
            }
            ConstraintV1::Typing { relation, rule } => format!("typing {relation}: {rule}"),
            ConstraintV1::SymmetricWhereIn {
                relation,
                field,
                values,
                carriers,
                params,
            } => {
                let mut s = format!(
                    "symmetric {relation} where {relation}.{field} in {{{}}}",
                    values.join(", ")
                );
                if let Some(c) = carriers.as_ref() {
                    s.push_str(&format!(" on ({}, {})", c.left_field, c.right_field));
                }
                if let Some(ps) = params.as_ref() {
                    if !ps.is_empty() {
                        s.push_str(&format!(" param ({})", ps.join(", ")));
                    }
                }
                s
            }
            ConstraintV1::Symmetric {
                relation,
                carriers,
                params,
            } => {
                let mut s = if let Some(c) = carriers.as_ref() {
                    format!(
                        "symmetric {relation} on ({}, {})",
                        c.left_field, c.right_field
                    )
                } else {
                    format!("symmetric {relation}")
                };
                if let Some(ps) = params.as_ref() {
                    if !ps.is_empty() {
                        s.push_str(&format!(" param ({})", ps.join(", ")));
                    }
                }
                s
            }
            ConstraintV1::Transitive {
                relation,
                carriers,
                params,
            } => {
                let mut s = if let Some(c) = carriers.as_ref() {
                    format!(
                        "transitive {relation} on ({}, {})",
                        c.left_field, c.right_field
                    )
                } else {
                    format!("transitive {relation}")
                };
                if let Some(ps) = params.as_ref() {
                    if !ps.is_empty() {
                        s.push_str(&format!(" param ({})", ps.join(", ")));
                    }
                }
                s
            }
            ConstraintV1::Key { relation, fields } => {
                format!("key {relation} ({})", fields.join(", "))
            }
            ConstraintV1::NamedBlock { name, body } => {
                if body.is_empty() {
                    format!("{name}:")
                } else {
                    format!("{name}: {}", body.join(" "))
                }
            }
            ConstraintV1::Unknown { text } => text.clone(),
        }
    }

    /// Load a finite set of bounded regular `.axi` files from one directory.
    fn load_axi_files(dir: &PathBuf) -> anyhow::Result<AxiSchemaIndex> {
        const MAX_AXI_FILES: usize = 10_000;
        const MAX_AXI_SCAN_ENTRIES: usize = 100_000;
        const MAX_AXI_FILE_BYTES: usize = 4 * 1024 * 1024;
        const MAX_AXI_TOTAL_BYTES: usize = 64 * 1024 * 1024;
        let mut entity_types: BTreeSet<String> = BTreeSet::new();
        let mut relation_types: BTreeSet<String> = BTreeSet::new();
        let mut constraints: BTreeSet<String> = BTreeSet::new();
        let mut files = 0_usize;
        let mut entries_scanned = 0_usize;
        let mut total_bytes = 0_usize;

        match std::fs::symlink_metadata(dir) {
            Ok(directory_metadata) => {
                if directory_metadata.file_type().is_symlink()
                    || !directory_metadata.file_type().is_dir()
                {
                    anyhow::bail!(".axi input root must be a real directory, not a symlink");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AxiSchemaIndex::default());
            }
            Err(error) => return Err(error.into()),
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            entries_scanned = entries_scanned.saturating_add(1);
            if entries_scanned > MAX_AXI_SCAN_ENTRIES {
                anyhow::bail!(
                    ".axi input directory exceeds {MAX_AXI_SCAN_ENTRIES} filesystem entries"
                );
            }
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "axi") {
                continue;
            }
            files = files.saturating_add(1);
            if files > MAX_AXI_FILES {
                anyhow::bail!(".axi input directory exceeds {MAX_AXI_FILES} files");
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
                anyhow::bail!(".axi input must be a regular file, not a symlink");
            }
            let length = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
            if length > MAX_AXI_FILE_BYTES {
                anyhow::bail!(".axi input exceeds {MAX_AXI_FILE_BYTES} bytes");
            }
            let bytes = axiograph_security::read_file_bounded(
                &path,
                MAX_AXI_FILE_BYTES,
                ".axi storage input",
            )?;
            total_bytes = total_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| anyhow::anyhow!(".axi input byte count overflow"))?;
            if total_bytes > MAX_AXI_TOTAL_BYTES {
                anyhow::bail!(".axi input closure exceeds {MAX_AXI_TOTAL_BYTES} bytes");
            }
            let contents = String::from_utf8(bytes)?;
            match dsl::axi_v1::parse_axi_v1(&contents) {
                Ok(module) => {
                    for schema in &module.schemas {
                        for obj in &schema.objects {
                            entity_types.insert(obj.clone());
                        }
                        for rel in &schema.relations {
                            relation_types.insert(rel.name.clone());
                        }
                        for subtype in &schema.subtypes {
                            constraints
                                .insert(format!("subtype {} <: {}", subtype.sub, subtype.sup));
                        }
                    }

                    for theory in &module.theories {
                        for constraint in &theory.constraints {
                            constraints.insert(Self::schema_constraint_display(constraint));
                        }
                        for eq in &theory.equations {
                            constraints.insert(format!("equation {}", eq.name));
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %err,
                        "failed to parse .axi while building schema index"
                    );
                }
            }
        }

        Ok(AxiSchemaIndex {
            entity_types: entity_types.into_iter().collect(),
            relation_types: relation_types.into_iter().collect(),
            constraints: constraints.into_iter().collect(),
        })
    }

    fn entity_attrs_with_storage_name<'a>(
        name: &'a str,
        attributes: &'a [(String, String)],
    ) -> Vec<(&'a str, &'a str)> {
        let mut attrs = Vec::with_capacity(attributes.len() + 1);
        attrs.extend(
            attributes
                .iter()
                .filter(|(k, _)| k.as_str() != STORAGE_ENTITY_NAME_ATTR)
                .map(|(k, v)| (k.as_str(), v.as_str())),
        );
        attrs.push((STORAGE_ENTITY_NAME_ATTR, name));
        attrs
    }

    fn fact_pathdb_entity_name(fact: &StorableFact) -> Option<&str> {
        match fact {
            StorableFact::Entity { name, .. }
            | StorableFact::TacitKnowledge { name, .. }
            | StorableFact::Concept { name, .. }
            | StorableFact::SafetyGuideline { name, .. } => Some(name.as_str()),
            StorableFact::Relation { .. } | StorableFact::Constraint { .. } => None,
        }
    }

    fn validate_relation_endpoints_for_change(
        pathdb: &PathDB,
        facts: &[StorableFact],
    ) -> Result<(), StorageSemanticError> {
        let mut available_new_entity_names: BTreeMap<&str, usize> = BTreeMap::new();
        for fact in facts {
            match fact {
                StorableFact::Relation {
                    name,
                    rel_type,
                    source,
                    target,
                    ..
                } => {
                    Self::validate_relation_endpoint_candidate(
                        pathdb,
                        &available_new_entity_names,
                        name.as_deref(),
                        rel_type,
                        RelationEndpointRole::Source,
                        source,
                    )?;
                    Self::validate_relation_endpoint_candidate(
                        pathdb,
                        &available_new_entity_names,
                        name.as_deref(),
                        rel_type,
                        RelationEndpointRole::Target,
                        target,
                    )?;
                }
                _ => {
                    if let Some(name) = Self::fact_pathdb_entity_name(fact) {
                        *available_new_entity_names.entry(name).or_default() += 1;
                    }
                }
            }
        }

        Ok(())
    }

    fn validate_relation_endpoint_candidate(
        pathdb: &PathDB,
        new_entity_names: &BTreeMap<&str, usize>,
        relation_name: Option<&str>,
        rel_type: &str,
        endpoint: RelationEndpointRole,
        entity_name: &str,
    ) -> Result<(), StorageSemanticError> {
        let new_count = new_entity_names
            .get(entity_name)
            .copied()
            .unwrap_or_default();
        let existing = Self::entity_ids_by_storage_name(pathdb, entity_name);

        if new_count > 1 || (new_count == 1 && !existing.is_empty()) {
            return Err(StorageSemanticError::AmbiguousRelationEndpoint {
                relation_name: relation_name.map(str::to_string),
                rel_type: rel_type.to_string(),
                endpoint,
                entity_name: entity_name.to_string(),
                candidate_ids: existing,
            });
        }

        if new_count == 1 {
            return Ok(());
        }

        Self::resolve_relation_endpoint(pathdb, relation_name, rel_type, endpoint, entity_name)
            .map(|_| ())
    }

    fn resolve_relation_endpoint(
        pathdb: &PathDB,
        relation_name: Option<&str>,
        rel_type: &str,
        endpoint: RelationEndpointRole,
        entity_name: &str,
    ) -> Result<u32, StorageSemanticError> {
        let candidate_ids = Self::entity_ids_by_storage_name(pathdb, entity_name);
        match candidate_ids.as_slice() {
            [] => Err(StorageSemanticError::UnresolvedRelationEndpoint {
                relation_name: relation_name.map(str::to_string),
                rel_type: rel_type.to_string(),
                endpoint,
                entity_name: entity_name.to_string(),
            }),
            [entity_id] => {
                if pathdb.entities.get_type(*entity_id).is_none() {
                    return Err(StorageSemanticError::UntypedRelationEndpoint {
                        relation_name: relation_name.map(str::to_string),
                        rel_type: rel_type.to_string(),
                        endpoint,
                        entity_name: entity_name.to_string(),
                        entity_id: *entity_id,
                    });
                }
                Ok(*entity_id)
            }
            _ => Err(StorageSemanticError::AmbiguousRelationEndpoint {
                relation_name: relation_name.map(str::to_string),
                rel_type: rel_type.to_string(),
                endpoint,
                entity_name: entity_name.to_string(),
                candidate_ids,
            }),
        }
    }

    fn entity_ids_by_storage_name(pathdb: &PathDB, entity_name: &str) -> Vec<u32> {
        let Some(name_key_id) = pathdb.interner.id_of(STORAGE_ENTITY_NAME_ATTR) else {
            return Vec::new();
        };
        let Some(name_value_id) = pathdb.interner.id_of(entity_name) else {
            return Vec::new();
        };
        pathdb
            .entities
            .entities_with_attr_value(name_key_id, name_value_id)
            .iter()
            .collect()
    }

    // ========================================================================
    // Write Operations
    // ========================================================================

    /// Add facts to storage (from any source)
    fn requires_review(&self, change: &Change) -> bool {
        if self.config.require_review.constraints
            && change
                .facts
                .iter()
                .any(|fact| matches!(fact, StorableFact::Constraint { .. }))
        {
            return true;
        }

        if self.config.require_review.schema_changes {
            let schema = self.schema.read();
            let extends_schema = change.facts.iter().any(|fact| match fact {
                StorableFact::Entity { entity_type, .. } => {
                    !schema.entity_types.iter().any(|known| known == entity_type)
                }
                StorableFact::Relation { rel_type, .. } => {
                    !schema.relation_types.iter().any(|known| known == rel_type)
                }
                _ => false,
            });
            if extends_schema {
                return true;
            }
        }

        let Some(threshold) = self.config.require_review.low_confidence_threshold else {
            return false;
        };
        let below_threshold = |confidence: f32| !confidence.is_finite() || confidence < threshold;
        change.facts.iter().any(|fact| match fact {
            StorableFact::Relation { confidence, .. }
            | StorableFact::TacitKnowledge { confidence, .. } => below_threshold(*confidence),
            _ => false,
        }) || match &change.source {
            ChangeSource::LLMExtraction { confidence, .. } => below_threshold(*confidence),
            _ => false,
        }
    }

    pub fn add_facts(
        &self,
        facts: Vec<StorableFact>,
        source: ChangeSource,
    ) -> anyhow::Result<ChangeId> {
        let valid_confidence =
            |confidence: f32| confidence.is_finite() && (0.0..=1.0).contains(&confidence);
        for (index, fact) in facts.iter().enumerate() {
            let confidence = match fact {
                StorableFact::Relation { confidence, .. }
                | StorableFact::TacitKnowledge { confidence, .. } => Some(*confidence),
                _ => None,
            };
            if confidence.is_some_and(|value| !valid_confidence(value)) {
                return Err(anyhow::anyhow!(
                    "fact {index} confidence must be a finite probability in [0, 1]"
                ));
            }
        }
        if let ChangeSource::LLMExtraction { confidence, .. } = &source {
            if !valid_confidence(*confidence) {
                return Err(anyhow::anyhow!(
                    "LLM extraction confidence must be a finite probability in [0, 1]"
                ));
            }
        }

        let change = Change {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source,
            facts,
            status: ChangeStatus::Pending,
        };

        if self.pending.read().len() >= self.config.max_pending {
            self.flush()?;
        }

        let change_id = change.id;
        let should_flush = {
            let mut pending = self.pending.write();
            if pending.len() >= self.config.max_pending {
                return Err(anyhow::anyhow!(
                    "pending change limit {} reached; approve or reject reviewed changes",
                    self.config.max_pending
                ));
            }
            pending.push(change);
            pending.len() >= self.config.max_pending
        };
        if should_flush {
            self.flush()?;
        }

        Ok(change_id)
    }

    /// Apply pending changes that do not require review to the evidence view.
    ///
    /// Review-required changes remain pending until `approve_change` or
    /// `reject_change`. The operation is atomic in memory. Callers must promote reviewed facts
    /// through `AxiStore`; this manager never persists an unauthenticated cache.
    pub fn flush(&self) -> anyhow::Result<Vec<ApplyResult>> {
        let _mutation_guard = self.mutation_lock.lock();
        let pending = self
            .pending
            .read()
            .iter()
            .filter(|change| !self.requires_review(change))
            .cloned()
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(Vec::new());
        }
        let pathdb_before = self.pathdb.read().detached_clone()?;
        let changelog_before = self.changelog.read().clone();

        let attempt = (|| -> anyhow::Result<Vec<ApplyResult>> {
            let mut results = Vec::with_capacity(pending.len());
            for change in &pending {
                results.push(self.apply_change(change)?);
            }
            Ok(results)
        })();

        match attempt {
            Ok(results) => {
                let applied_ids = pending
                    .iter()
                    .map(|change| change.id)
                    .collect::<BTreeSet<_>>();
                self.pending
                    .write()
                    .retain(|change| !applied_ids.contains(&change.id));
                Ok(results)
            }
            Err(error) => {
                *self.pathdb.write() = pathdb_before;
                *self.changelog.write() = changelog_before;
                Err(error)
            }
        }
    }

    /// Explicitly approve and apply one pending review-required change.
    pub fn approve_change(&self, change_id: ChangeId) -> anyhow::Result<ApplyResult> {
        let _mutation_guard = self.mutation_lock.lock();
        let change = self
            .pending
            .read()
            .iter()
            .find(|change| change.id == change_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("pending change {change_id} was not found"))?;
        let pathdb_before = self.pathdb.read().detached_clone()?;
        let changelog_before = self.changelog.read().clone();

        match self.apply_change(&change) {
            Ok(result) => {
                self.pending
                    .write()
                    .retain(|pending| pending.id != change_id);
                Ok(result)
            }
            Err(error) => {
                *self.pathdb.write() = pathdb_before;
                *self.changelog.write() = changelog_before;
                Err(error)
            }
        }
    }

    /// Explicitly reject one pending change without materializing its facts.
    pub fn reject_change(&self, change_id: ChangeId, reason: &str) -> anyhow::Result<()> {
        let _mutation_guard = self.mutation_lock.lock();
        if reason.trim().is_empty() {
            return Err(anyhow::anyhow!("rejection reason must not be empty"));
        }
        let mut pending = self.pending.write();
        let position = pending
            .iter()
            .position(|change| change.id == change_id)
            .ok_or_else(|| anyhow::anyhow!("pending change {change_id} was not found"))?;
        let mut change = pending.remove(position);
        drop(pending);
        change.status = ChangeStatus::Rejected {
            reason: reason.to_string(),
        };
        self.changelog.write().push(change);
        Ok(())
    }

    fn apply_change(&self, change: &Change) -> anyhow::Result<ApplyResult> {
        let mut pathdb = self.pathdb.write();
        Self::validate_relation_endpoints_for_change(&pathdb, &change.facts)?;

        let mut pathdb_ids = Vec::new();
        let mut axi_lines = Vec::new();
        let mut warnings = Vec::new();

        for fact in &change.facts {
            match fact {
                StorableFact::Entity {
                    name,
                    entity_type,
                    attributes,
                } => {
                    // Add to PathDB
                    let attrs = Self::entity_attrs_with_storage_name(name, attributes);
                    let id = pathdb.add_entity(entity_type, attrs);
                    pathdb_ids.push(id);

                    // Generate .axi line
                    let axi = self.entity_to_axi(name, entity_type, attributes);
                    axi_lines.push(axi);
                }

                StorableFact::Relation {
                    name,
                    rel_type,
                    source,
                    target,
                    confidence,
                    attributes,
                } => {
                    let source_id = Self::resolve_relation_endpoint(
                        &pathdb,
                        name.as_deref(),
                        rel_type,
                        RelationEndpointRole::Source,
                        source,
                    )?;
                    let target_id = Self::resolve_relation_endpoint(
                        &pathdb,
                        name.as_deref(),
                        rel_type,
                        RelationEndpointRole::Target,
                        target,
                    )?;

                    let attrs: Vec<(&str, &str)> = attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
                    let id =
                        pathdb.add_relation(rel_type, source_id, target_id, *confidence, attrs);
                    pathdb_ids.push(id);

                    // Generate .axi line
                    let axi = self.relation_to_axi(
                        name.as_deref(),
                        rel_type,
                        source,
                        target,
                        *confidence,
                    );
                    axi_lines.push(axi);
                }

                StorableFact::Constraint {
                    name,
                    condition,
                    severity,
                    message,
                } => {
                    // Constraints go to .axi only (interpreted at query time)
                    let axi = self.constraint_to_axi(name, condition, severity, message.as_deref());
                    axi_lines.push(axi);

                    if self.config.require_review.constraints {
                        warnings.push(format!("Constraint '{name}' added - requires review"));
                    }
                }

                StorableFact::TacitKnowledge {
                    name,
                    rule,
                    confidence,
                    domain,
                    source,
                } => {
                    // Add as special entity in PathDB
                    let id = pathdb.add_entity(
                        "TacitKnowledge",
                        vec![
                            ("name", name.as_str()),
                            ("rule", rule.as_str()),
                            ("domain", domain.as_str()),
                            ("source", source.as_str()),
                        ],
                    );
                    pathdb_ids.push(id);

                    // Generate .axi
                    let axi = self.tacit_to_axi(name, rule, *confidence, domain, source);
                    axi_lines.push(axi);
                }

                StorableFact::Concept {
                    name,
                    description,
                    difficulty,
                    prerequisites,
                } => {
                    let id = pathdb.add_entity(
                        "Concept",
                        vec![
                            ("name", name.as_str()),
                            ("description", description.as_str()),
                            ("difficulty", difficulty.as_str()),
                        ],
                    );
                    pathdb_ids.push(id);

                    let axi = self.concept_to_axi(name, description, difficulty, prerequisites);
                    axi_lines.push(axi);
                }

                StorableFact::SafetyGuideline {
                    name,
                    title,
                    severity,
                    explanation,
                } => {
                    let id = pathdb.add_entity(
                        "SafetyGuideline",
                        vec![
                            ("name", name.as_str()),
                            ("title", title.as_str()),
                            ("severity", severity.as_str()),
                        ],
                    );
                    pathdb_ids.push(id);

                    let axi = self.guideline_to_axi(name, title, severity, explanation);
                    axi_lines.push(axi);
                }
            }
        }

        // Record in the in-memory review log. The generated `.axi` fragments
        // remain proposals in `ApplyResult`; they are never appended to accepted files.

        let mut applied_change = change.clone();
        applied_change.status = ChangeStatus::Applied;
        self.changelog.write().push(applied_change);

        Ok(ApplyResult {
            change_id: change.id,
            pathdb_ids,
            axi_lines,
            warnings,
        })
    }

    // ========================================================================
    // .axi Generation
    // ========================================================================

    fn entity_to_axi(&self, name: &str, entity_type: &str, attrs: &[(String, String)]) -> String {
        let mut s = format!("{name} : {entity_type} {{\n");
        for (k, v) in attrs {
            s.push_str(&format!("  {k} = \"{v}\"\n"));
        }
        s.push_str("}\n");
        s
    }

    fn relation_to_axi(
        &self,
        name: Option<&str>,
        rel_type: &str,
        source: &str,
        target: &str,
        confidence: f32,
    ) -> String {
        if let Some(n) = name {
            format!("{n} : {rel_type}({source}, {target}) @confidence({confidence})\n")
        } else {
            format!("{rel_type}({source}, {target}) @confidence({confidence})\n")
        }
    }

    fn constraint_to_axi(
        &self,
        name: &str,
        condition: &str,
        severity: &str,
        message: Option<&str>,
    ) -> String {
        let mut s = format!("constraint {name} {{\n");
        s.push_str(&format!("  severity = {severity}\n"));
        s.push_str(&format!("  condition = \"{condition}\"\n"));
        if let Some(msg) = message {
            s.push_str(&format!("  message = \"{msg}\"\n"));
        }
        s.push_str("}\n");
        s
    }

    fn tacit_to_axi(
        &self,
        name: &str,
        rule: &str,
        confidence: f32,
        domain: &str,
        source: &str,
    ) -> String {
        format!(
            "tacit \"{name}\" {{\n  rule: {rule}\n  confidence: {confidence}\n  domain: \"{domain}\"\n  source: \"{source}\"\n}}\n"
        )
    }

    fn concept_to_axi(
        &self,
        name: &str,
        description: &str,
        difficulty: &str,
        prerequisites: &[String],
    ) -> String {
        let prereqs = if prerequisites.is_empty() {
            "[]".to_string()
        } else {
            format!("[{}]", prerequisites.join(", "))
        };
        format!(
            "concept {name} : Concept {{\n  description = \"\"\"{description}\"\"\"\n  difficulty = {difficulty}\n  prerequisites = {prereqs}\n}}\n"
        )
    }

    fn guideline_to_axi(
        &self,
        name: &str,
        title: &str,
        severity: &str,
        explanation: &str,
    ) -> String {
        format!(
            "guideline {name} : SafetyGuideline {{\n  title = \"{title}\"\n  severity = {severity}\n  explanation = \"\"\"{explanation}\"\"\"\n}}\n"
        )
    }

    // ========================================================================
    // Read Operations
    // ========================================================================

    /// Borrow the process-local PathDB evidence view for read-only queries.
    pub fn pathdb(&self) -> RwLockReadGuard<'_, PathDB> {
        self.pathdb.read()
    }

    /// Borrow the current read-only schema index.
    pub fn schema(&self) -> RwLockReadGuard<'_, AxiSchemaIndex> {
        self.schema.read()
    }

    /// Get change history
    pub fn changelog(&self) -> Vec<Change> {
        self.changelog.read().clone()
    }

    /// Get pending changes
    pub fn pending(&self) -> Vec<Change> {
        self.pending.read().clone()
    }

    // ========================================================================
    // Rollback
    // ========================================================================

    /// Rollback to a specific change
    pub fn rollback_to(&self, change_id: ChangeId) -> anyhow::Result<()> {
        let _mutation_guard = self.mutation_lock.lock();
        // Find the change index
        let changelog = self.changelog.read();
        let idx = changelog
            .iter()
            .position(|c| c.id == change_id)
            .ok_or_else(|| anyhow::anyhow!("Change not found: {change_id}"))?;

        // Mark subsequent changes as rolled back
        drop(changelog);
        let mut changelog = self.changelog.write();
        for change in changelog.iter_mut().skip(idx + 1) {
            change.status = ChangeStatus::Rolled {
                reason: format!("Rolled back to {change_id}"),
            };
        }

        // Rebuild PathDB from changelog
        drop(changelog);
        self.rebuild_from_changelog()?;
        Ok(())
    }

    /// Rebuild PathDB from changelog (up to Applied changes)
    fn rebuild_from_changelog(&self) -> anyhow::Result<()> {
        let mut pathdb = self.pathdb.write();
        *pathdb = PathDB::new();

        let changelog = self.changelog.read();
        for change in changelog.iter() {
            if matches!(change.status, ChangeStatus::Applied) {
                Self::validate_relation_endpoints_for_change(&pathdb, &change.facts)?;
                for fact in &change.facts {
                    match fact {
                        StorableFact::Entity {
                            name,
                            entity_type,
                            attributes,
                        } => {
                            let attrs = Self::entity_attrs_with_storage_name(name, attributes);
                            pathdb.add_entity(entity_type, attrs);
                        }
                        StorableFact::Relation {
                            name,
                            rel_type,
                            source,
                            target,
                            confidence,
                            attributes,
                            ..
                        } => {
                            let source_id = Self::resolve_relation_endpoint(
                                &pathdb,
                                name.as_deref(),
                                rel_type,
                                RelationEndpointRole::Source,
                                source,
                            )?;
                            let target_id = Self::resolve_relation_endpoint(
                                &pathdb,
                                name.as_deref(),
                                rel_type,
                                RelationEndpointRole::Target,
                                target,
                            )?;
                            let attrs: Vec<(&str, &str)> = attributes
                                .iter()
                                .map(|(k, v)| (k.as_str(), v.as_str()))
                                .collect();
                            pathdb.add_relation(rel_type, source_id, target_id, *confidence, attrs);
                        }
                        StorableFact::TacitKnowledge {
                            name,
                            rule,
                            domain,
                            source,
                            ..
                        } => {
                            pathdb.add_entity(
                                "TacitKnowledge",
                                vec![
                                    ("name", name.as_str()),
                                    ("rule", rule.as_str()),
                                    ("domain", domain.as_str()),
                                    ("source", source.as_str()),
                                ],
                            );
                        }
                        StorableFact::Concept {
                            name,
                            description,
                            difficulty,
                            ..
                        } => {
                            pathdb.add_entity(
                                "Concept",
                                vec![
                                    ("name", name.as_str()),
                                    ("description", description.as_str()),
                                    ("difficulty", difficulty.as_str()),
                                ],
                            );
                        }
                        StorableFact::SafetyGuideline {
                            name,
                            title,
                            severity,
                            ..
                        } => {
                            pathdb.add_entity(
                                "SafetyGuideline",
                                vec![
                                    ("name", name.as_str()),
                                    ("title", title.as_str()),
                                    ("severity", severity.as_str()),
                                ],
                            );
                        }
                        StorableFact::Constraint { .. } => {}
                    }
                }
            }
        }

        // Rebuild indexes
        pathdb.build_indexes();

        Ok(())
    }

    // ========================================================================
    // Sync from .axi files
    // ========================================================================

    /// Reload .axi files and sync to PathDB
    pub fn sync_from_axi(&self) -> anyhow::Result<usize> {
        // Today we only refresh a lightweight schema index (names of entity/relation
        // types and constraints) to support grounding/validation. Importing full
        // `.axi` instances into PathDB is intentionally deferred until the
        // dialects and certificate semantics are fully stabilized.
        let schema = Self::load_axi_files(&self.config.axi_dir)?;
        *self.schema.write() = schema;
        Ok(0)
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Create storage from common paths
pub fn open_storage(knowledge_dir: &str) -> anyhow::Result<UnifiedStorage> {
    let dir = PathBuf::from(knowledge_dir);
    let config = StorageConfig {
        axi_dir: dir,
        ..Default::default()
    };
    UnifiedStorage::new(config)
}
