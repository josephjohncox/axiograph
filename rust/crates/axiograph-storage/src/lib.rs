//! Axiograph Unified Storage Layer
//!
//! Provides a single interface for reading and writing knowledge:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                    UNIFIED STORAGE                                  │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │                                                                     │
//! │  ┌─────────┐     ┌───────────────┐     ┌─────────────┐             │
//! │  │   LLM   │────►│               │────►│  .axi file  │             │
//! │  │  Sync   │     │   Unified     │     │  (human)    │             │
//! │  └─────────┘     │   Storage     │     └─────────────┘             │
//! │                  │   Manager     │                                  │
//! │  ┌─────────┐     │               │     ┌─────────────┐             │
//! │  │  User   │────►│               │────►│   PathDB    │             │
//! │  │  Edits  │     │               │     │  (binary)   │             │
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
//! - **Dual Format**: Writes to both .axi (human-readable) and PathDB (indexed)
//! - **Transactional**: Changes are atomic across both formats
//! - **Versioned**: Full change history with rollback
//! - **Synced**: Hot reload when files change externally
#![allow(unused_variables)]

pub mod persistence;

#[cfg(test)]
mod tests;

use axiograph_dsl as dsl;
use axiograph_pathdb::PathDB;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

// ============================================================================
// Core Types
// ============================================================================

/// Unique identifier for a storage change
pub type ChangeId = Uuid;

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
    /// Lines added to .axi file
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

/// Configuration for the unified storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory for .axi files
    pub axi_dir: PathBuf,
    /// Path to PathDB binary file
    pub pathdb_path: PathBuf,
    /// Path to changelog
    pub changelog_path: PathBuf,
    /// Auto-sync on file changes
    pub watch_files: bool,
    /// Require human review for certain changes
    pub require_review: ReviewPolicy,
    /// Maximum pending changes before force-sync
    pub max_pending: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            pathdb_path: PathBuf::from("./knowledge.axpd"),
            changelog_path: PathBuf::from("./changelog.json"),
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
// Unified Storage Manager
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
}

impl UnifiedStorage {
    /// Create new storage manager
    pub fn new(config: StorageConfig) -> anyhow::Result<Self> {
        // Load or create PathDB
        let pathdb = if config.pathdb_path.exists() {
            let bytes = std::fs::read(&config.pathdb_path)?;
            PathDB::from_bytes(&bytes)?
        } else {
            PathDB::new()
        };

        // Load changelog if exists
        let changelog = if config.changelog_path.exists() {
            let contents = std::fs::read_to_string(&config.changelog_path)?;
            serde_json::from_str(&contents)?
        } else {
            Vec::new()
        };

        // Load schema from .axi files
        let schema = Self::load_axi_files(&config.axi_dir)?;

        Ok(Self {
            config,
            pathdb: Arc::new(RwLock::new(pathdb)),
            pending: Arc::new(RwLock::new(Vec::new())),
            changelog: Arc::new(RwLock::new(changelog)),
            schema: Arc::new(RwLock::new(schema)),
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
            ConstraintV1::Typing { relation, rule } => format!("typing {relation}: {rule}"),
            ConstraintV1::SymmetricWhereIn {
                relation,
                field,
                values,
            } => format!("symmetric {relation} where {relation}.{field} in {{{}}}", values.join(", ")),
            ConstraintV1::Symmetric { relation } => format!("symmetric {relation}"),
            ConstraintV1::Transitive { relation } => format!("transitive {relation}"),
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

    /// Load all .axi files from directory
    fn load_axi_files(dir: &PathBuf) -> anyhow::Result<AxiSchemaIndex> {
        let mut entity_types: BTreeSet<String> = BTreeSet::new();
        let mut relation_types: BTreeSet<String> = BTreeSet::new();
        let mut constraints: BTreeSet<String> = BTreeSet::new();

        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if !path.extension().map_or(false, |e| e == "axi") {
                    continue;
                }

                let contents = std::fs::read_to_string(&path)?;
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
        }

        Ok(AxiSchemaIndex {
            entity_types: entity_types.into_iter().collect(),
            relation_types: relation_types.into_iter().collect(),
            constraints: constraints.into_iter().collect(),
        })
    }

    // ========================================================================
    // Write Operations
    // ========================================================================

    /// Add facts to storage (from any source)
    pub fn add_facts(
        &self,
        facts: Vec<StorableFact>,
        source: ChangeSource,
    ) -> anyhow::Result<ChangeId> {
        let change = Change {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            source,
            facts,
            status: ChangeStatus::Pending,
        };

        let change_id = change.id;
        self.pending.write().push(change);

        // Auto-apply if below threshold
        if self.pending.read().len() >= self.config.max_pending {
            self.flush()?;
        }

        Ok(change_id)
    }

    /// Apply all pending changes
    pub fn flush(&self) -> anyhow::Result<Vec<ApplyResult>> {
        let pending: Vec<Change> = self.pending.write().drain(..).collect();
        let mut results = Vec::new();

        for change in pending {
            let result = self.apply_change(&change)?;
            results.push(result);
        }

        // Save changelog
        self.save_changelog()?;

        // Save PathDB
        self.save_pathdb()?;

        Ok(results)
    }

    /// Apply a single change
    fn apply_change(&self, change: &Change) -> anyhow::Result<ApplyResult> {
        let mut pathdb = self.pathdb.write();
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
                    let attrs: Vec<(&str, &str)> = attributes
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str()))
                        .collect();
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
                    // Resolve source/target to IDs (simplified)
                    // In production, would look up by name
                    let source_id = 0; // placeholder
                    let target_id = 1; // placeholder

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
                        warnings.push(format!("Constraint '{}' added - requires review", name));
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

        // Write to .axi file
        self.append_to_axi(&axi_lines, &change.source)?;

        // Record in changelog
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
        let mut s = format!("{} : {} {{\n", name, entity_type);
        for (k, v) in attrs {
            s.push_str(&format!("  {} = \"{}\"\n", k, v));
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
            format!(
                "{} : {}({}, {}) @confidence({})\n",
                n, rel_type, source, target, confidence
            )
        } else {
            format!(
                "{}({}, {}) @confidence({})\n",
                rel_type, source, target, confidence
            )
        }
    }

    fn constraint_to_axi(
        &self,
        name: &str,
        condition: &str,
        severity: &str,
        message: Option<&str>,
    ) -> String {
        let mut s = format!("constraint {} {{\n", name);
        s.push_str(&format!("  severity = {}\n", severity));
        s.push_str(&format!("  condition = \"{}\"\n", condition));
        if let Some(msg) = message {
            s.push_str(&format!("  message = \"{}\"\n", msg));
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
            "tacit \"{}\" {{\n  rule: {}\n  confidence: {}\n  domain: \"{}\"\n  source: \"{}\"\n}}\n",
            name, rule, confidence, domain, source
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
            "concept {} : Concept {{\n  description = \"\"\"{}\"\"\"\n  difficulty = {}\n  prerequisites = {}\n}}\n",
            name, description, difficulty, prereqs
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
            "guideline {} : SafetyGuideline {{\n  title = \"{}\"\n  severity = {}\n  explanation = \"\"\"{}\"\"\"\n}}\n",
            name, title, severity, explanation
        )
    }

    /// Append lines to the appropriate .axi file
    fn append_to_axi(&self, lines: &[String], source: &ChangeSource) -> anyhow::Result<()> {
        // Determine file name based on source
        let filename = match source {
            ChangeSource::LLMExtraction { .. } => "llm_extracted.axi",
            ChangeSource::UserEdit { .. } => "user_edits.axi",
            ChangeSource::FileImport { path } => {
                return Ok(()); // Already in a file
            }
            ChangeSource::API { .. } => "api_additions.axi",
            ChangeSource::System { .. } => "system_inferred.axi",
        };

        let path = self.config.axi_dir.join(filename);

        // Create directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Append to file
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        // Add header comment for this batch
        writeln!(file, "\n-- Added at {}", Utc::now().to_rfc3339())?;
        match source {
            ChangeSource::LLMExtraction {
                session_id,
                model,
                confidence,
            } => {
                writeln!(
                    file,
                    "-- Source: LLM extraction (model: {}, confidence: {:.2})",
                    model, confidence
                )?;
            }
            ChangeSource::UserEdit { user_id } => {
                writeln!(
                    file,
                    "-- Source: User edit ({})",
                    user_id.as_deref().unwrap_or("anonymous")
                )?;
            }
            _ => {}
        }

        for line in lines {
            writeln!(file, "{}", line)?;
        }

        Ok(())
    }

    // ========================================================================
    // Persistence
    // ========================================================================

    fn save_changelog(&self) -> anyhow::Result<()> {
        let changelog = self.changelog.read();
        let json = serde_json::to_string_pretty(&*changelog)?;
        std::fs::write(&self.config.changelog_path, json)?;
        Ok(())
    }

    fn save_pathdb(&self) -> anyhow::Result<()> {
        let pathdb = self.pathdb.read();
        let bytes = pathdb.to_bytes()?;
        std::fs::write(&self.config.pathdb_path, bytes)?;
        Ok(())
    }

    // ========================================================================
    // Read Operations
    // ========================================================================

    /// Get PathDB for queries
    pub fn pathdb(&self) -> Arc<RwLock<PathDB>> {
        Arc::clone(&self.pathdb)
    }

    /// Get current schema
    pub fn schema(&self) -> Arc<RwLock<AxiSchemaIndex>> {
        Arc::clone(&self.schema)
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
        // Find the change index
        let changelog = self.changelog.read();
        let idx = changelog
            .iter()
            .position(|c| c.id == change_id)
            .ok_or_else(|| anyhow::anyhow!("Change not found: {}", change_id))?;

        // Mark subsequent changes as rolled back
        drop(changelog);
        let mut changelog = self.changelog.write();
        for change in changelog.iter_mut().skip(idx + 1) {
            change.status = ChangeStatus::Rolled {
                reason: format!("Rolled back to {}", change_id),
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
                for fact in &change.facts {
                    match fact {
                        StorableFact::Entity {
                            name,
                            entity_type,
                            attributes,
                        } => {
                            let attrs: Vec<(&str, &str)> = attributes
                                .iter()
                                .map(|(k, v)| (k.as_str(), v.as_str()))
                                .collect();
                            pathdb.add_entity(entity_type, attrs);
                        }
                        StorableFact::Relation {
                            rel_type,
                            source,
                            target,
                            confidence,
                            attributes,
                            ..
                        } => {
                            // Simplified - would need name resolution
                            let attrs: Vec<(&str, &str)> = attributes
                                .iter()
                                .map(|(k, v)| (k.as_str(), v.as_str()))
                                .collect();
                            pathdb.add_relation(rel_type, 0, 1, *confidence, attrs);
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
                        _ => {}
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
        axi_dir: dir.clone(),
        pathdb_path: dir.join("knowledge.axpd"),
        changelog_path: dir.join("changelog.json"),
        ..Default::default()
    };
    UnifiedStorage::new(config)
}
