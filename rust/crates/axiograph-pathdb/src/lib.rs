//! PathDB: Efficient Binary Path-Indexed Knowledge Graph Storage
//!
//! Based on research from:
//! - Graph database path query optimization (Gubichev et al.)
//! - Roaring Bitmaps for set operations (Lemire et al.)
//! - Succinct data structures for compact representation
//! - Zero-copy deserialization (rkyv)
//!
//! Key innovations:
//! 1. **String Interning**: All strings stored once, referenced by u32 ID
//! 2. **Path Indexing**: Pre-computed path signatures for fast traversal
//! 3. **Bitmap Joins**: Set operations on entity IDs using Roaring bitmaps
//! 4. **Memory Mapping**: Large KGs accessed via mmap without full load
//! 5. **Columnar Storage**: Relations stored column-wise for cache efficiency
//!
//! ## Verification
//!
//! This crate is designed for provable correctness:
//! - **Lean**: trusted checker/spec for certificates (`lean/Axiograph/*`)
//! - **Verus**: additive runtime invariant hardening (`rust/verus/` + `verified.rs`)
//! - **Shared binary format**: v2 `.axpd` with modal/probabilistic extensions
//!
//! ## Module Organization
//!
//! - `verified`: Verus-verified types and operations
//! - `modal`: Modal logic support (Kripke, epistemic, deontic)
//! - `guardrails`: Safety checks and learning support

#![allow(unused_variables)]

pub mod axi_export;
pub mod axi_meta;
pub mod axi_module_constraints;
pub mod axi_module_export;
pub mod axi_module_import;
pub mod axi_module_typecheck;
pub mod axi_semantics;
pub mod axi_typed;
pub mod branding;
pub mod certificate;
mod fact_index;
pub mod guardrails;
pub mod learning;
pub mod migration;
pub mod modal;
pub mod optimizer;
pub mod proof_mode;
mod text_index;
pub mod typestate;
pub mod verified;
pub mod witness;

use anyhow::Result;
use dashmap::DashMap;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

// Re-export key types
pub use branding::{DbBranded, DbToken, DbTokenMismatch};
pub use certificate::{
    AxiAnchorV1, AxiConstraintsOkProofV1, AxiWellTypedProofV1, Certificate, CertificateV2,
    FixedPointProbability, FixedProb, NormalizePathProofV2, PathEquivProofV2, PathExprV2,
    PathRewriteStepV3, ReachabilityProofV2, ResolutionDecisionV2, ResolutionProofV2,
    RewriteDerivationProofV2, RewriteDerivationProofV3, VProb, CERTIFICATE_VERSION,
    CERTIFICATE_VERSION_V2, FIXED_POINT_DENOMINATOR, FIXED_PROB_PRECISION,
};
pub use guardrails::{GuardrailEngine, GuardrailRule, GuardrailViolation, Severity};
pub use migration::{
    ArrowDeclV1, ArrowMapV1, ArrowMappingV1, DeltaFMigrationProofV1, InstanceV1, Name,
    ObjectElementsV1, ObjectMappingV1, SchemaMorphismV1, SchemaV1, SigmaFMigrationProofV1,
    SubtypeDeclV1,
};
pub use modal::{ModalFrame, ModalPathDB, ModalWorld, Modality};
pub use optimizer::{MigrationOperatorV1, OptimizerRuleV1, ProofProducingOptimizer};
pub use proof_mode::{NoProof, ProofJournal, ProofMode, Proved, WithProof};
pub use typestate::{NormalizedPathExprV2, UnnormalizedPathExprV2};
pub use verified::{BinaryHeader, ReachabilityProof, VerifiedPathSig, VerifiedProb};

use fact_index::FactIndexCache;
use text_index::TextIndexCache;

/// Tokenize a string using the same rules as PathDB's `fts` query operators.
///
/// This is an **extension-layer** helper intended for REPL/LLM grounding, so
/// other crates can stay aligned with PathDB's tokenizer.
pub fn tokenize_fts_query(query: &str) -> Vec<String> {
    text_index::tokenize_query(query)
}

// ============================================================================
// String Interning (Compact String Storage)
// ============================================================================

/// Interned string ID (4 bytes instead of 24+ for String)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct StrId(u32);

impl StrId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// String interner: maps strings to compact IDs
pub struct StringInterner {
    /// String to ID mapping
    str_to_id: DashMap<String, StrId>,
    /// ID to string mapping (for reverse lookup)
    id_to_str: DashMap<StrId, String>,
    /// Next available ID
    next_id: AtomicU32,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            str_to_id: DashMap::new(),
            id_to_str: DashMap::new(),
            next_id: AtomicU32::new(0),
        }
    }

    /// Intern a string, returning its ID
    pub fn intern(&self, s: &str) -> StrId {
        if let Some(id) = self.str_to_id.get(s) {
            return *id;
        }

        let id = StrId(self.next_id.fetch_add(1, Ordering::SeqCst));
        self.str_to_id.insert(s.to_string(), id);
        self.id_to_str.insert(id, s.to_string());
        id
    }

    /// Look up an existing ID for a string without inserting.
    pub fn id_of(&self, s: &str) -> Option<StrId> {
        self.str_to_id.get(s).map(|id| *id)
    }

    /// Look up string by ID
    pub fn lookup(&self, id: StrId) -> Option<String> {
        self.id_to_str.get(&id).map(|s| s.clone())
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let strings: Vec<String> = (0..self.next_id.load(Ordering::SeqCst))
            .filter_map(|i| self.id_to_str.get(&StrId(i)).map(|s| s.clone()))
            .collect();
        bincode::serialize(&strings).unwrap_or_default()
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let strings: Vec<String> = bincode::deserialize(bytes)?;
        let interner = Self::new();
        for s in strings {
            interner.intern(&s);
        }
        Ok(interner)
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Entity Storage (Columnar)
// ============================================================================

/// An entity in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: u32,
    pub type_id: StrId,
    pub attrs: Vec<(StrId, StrId)>, // (attr_name, attr_value)
}

/// Debug/FFI-friendly entity view with resolved strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityView {
    pub id: u32,
    pub entity_type: String,
    pub attrs: HashMap<String, String>,
}

/// Columnar entity storage
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct EntityStore {
    /// Type column: entity_id -> type_id
    types: Vec<StrId>,
    /// Attribute columns: attr_name -> (entity_id -> value)
    attrs: HashMap<StrId, HashMap<u32, StrId>>,
    /// Type index: type_id -> bitmap of entity IDs
    type_index: HashMap<StrId, RoaringBitmap>,
    /// Next entity ID
    next_id: u32,
}

impl EntityStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of entities stored.
    pub fn len(&self) -> usize {
        self.next_id as usize
    }

    pub fn is_empty(&self) -> bool {
        self.next_id == 0
    }

    /// Add an entity
    pub fn add(&mut self, type_id: StrId, attrs: Vec<(StrId, StrId)>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        // Store type
        if id as usize >= self.types.len() {
            self.types.resize(id as usize + 1, StrId(0));
        }
        self.types[id as usize] = type_id;

        // Update type index
        self.type_index
            .entry(type_id)
            .or_insert_with(RoaringBitmap::new)
            .insert(id);

        // Store attributes
        for (attr_name, attr_value) in attrs {
            self.attrs
                .entry(attr_name)
                .or_insert_with(HashMap::new)
                .insert(id, attr_value);
        }

        id
    }

    /// Get entities by type (returns bitmap)
    pub fn by_type(&self, type_id: StrId) -> Option<&RoaringBitmap> {
        self.type_index.get(&type_id)
    }

    /// Get entity type
    pub fn get_type(&self, entity_id: u32) -> Option<StrId> {
        self.types.get(entity_id as usize).copied()
    }

    /// Get attribute value
    pub fn get_attr(&self, entity_id: u32, attr_name: StrId) -> Option<StrId> {
        self.attrs.get(&attr_name)?.get(&entity_id).copied()
    }

    /// Find all entities where `attr_name == value`.
    pub fn entities_with_attr_value(&self, attr_name: StrId, value: StrId) -> RoaringBitmap {
        let mut out = RoaringBitmap::new();
        let Some(col) = self.attrs.get(&attr_name) else {
            return out;
        };
        for (&entity_id, &v) in col {
            if v == value {
                out.insert(entity_id);
            }
        }
        out
    }
}

// ============================================================================
// Relation Storage (Edge-List with Indexes)
// ============================================================================

/// A relation (edge) in the knowledge graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub rel_type: StrId,
    pub source: u32,
    pub target: u32,
    pub confidence: f32, // 4 bytes instead of 8 for f64
    pub attrs: Vec<(StrId, StrId)>,
}

/// Indexed relation storage
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RelationStore {
    /// All relations
    relations: Vec<Relation>,
    /// Forward index: (source, rel_type) -> relation IDs
    forward_index: HashMap<(u32, StrId), Vec<u32>>,
    /// Backward index: (target, rel_type) -> relation IDs
    backward_index: HashMap<(u32, StrId), Vec<u32>>,
    /// Type index: rel_type -> relation IDs
    type_index: HashMap<StrId, RoaringBitmap>,
}

impl RelationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of relations stored.
    pub fn len(&self) -> usize {
        self.relations.len()
    }

    /// Number of relations for a given relation type.
    pub fn rel_type_count(&self, rel_type: StrId) -> usize {
        self.type_index
            .get(&rel_type)
            .map(|ids| ids.len() as usize)
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.relations.is_empty()
    }

    /// Add a relation
    pub fn add(&mut self, rel: Relation) -> u32 {
        let id = self.relations.len() as u32;

        // Update indexes
        self.forward_index
            .entry((rel.source, rel.rel_type))
            .or_insert_with(Vec::new)
            .push(id);

        self.backward_index
            .entry((rel.target, rel.rel_type))
            .or_insert_with(Vec::new)
            .push(id);

        self.type_index
            .entry(rel.rel_type)
            .or_insert_with(RoaringBitmap::new)
            .insert(id);

        self.relations.push(rel);
        id
    }

    /// Get outgoing relations from source with given type
    pub fn outgoing(&self, source: u32, rel_type: StrId) -> Vec<&Relation> {
        self.forward_index
            .get(&(source, rel_type))
            .map(|ids| {
                ids.iter()
                    .filter_map(|&id| self.relations.get(id as usize))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get outgoing relations from source (any type).
    ///
    /// This is primarily intended for lightweight tooling (FFI, debugging).
    /// Performance-sensitive callers should use `outgoing(source, rel_type)` or
    /// a query plan that fixes `rel_type`.
    pub fn outgoing_any(&self, source: u32) -> Vec<&Relation> {
        let mut out = Vec::new();
        for ((src, _), ids) in &self.forward_index {
            if *src != source {
                continue;
            }
            out.extend(ids.iter().filter_map(|&id| self.relations.get(id as usize)));
        }
        out
    }

    /// Get incoming relations to target (any type).
    ///
    /// This is primarily intended for lightweight tooling (REPL, debugging).
    /// Performance-sensitive callers should fix `rel_type` and use `incoming(...)`.
    pub fn incoming_any(&self, target: u32) -> Vec<&Relation> {
        let mut out = Vec::new();
        for ((dst, _), ids) in &self.backward_index {
            if *dst != target {
                continue;
            }
            out.extend(ids.iter().filter_map(|&id| self.relations.get(id as usize)));
        }
        out
    }

    /// Get incoming relations to target with given type
    pub fn incoming(&self, target: u32, rel_type: StrId) -> Vec<&Relation> {
        self.backward_index
            .get(&(target, rel_type))
            .map(|ids| {
                ids.iter()
                    .filter_map(|&id| self.relations.get(id as usize))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all targets reachable from source via rel_type
    pub fn targets(&self, source: u32, rel_type: StrId) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        for rel in self.outgoing(source, rel_type) {
            result.insert(rel.target);
        }
        result
    }

    /// Get all targets reachable from `source` via `rel_type`, but only counting
    /// edges whose `confidence >= min_confidence`.
    pub fn targets_with_min_confidence(
        &self,
        source: u32,
        rel_type: StrId,
        min_confidence: f32,
    ) -> RoaringBitmap {
        let min_confidence = min_confidence.clamp(0.0, 1.0);
        let mut out = RoaringBitmap::new();
        let Some(ids) = self.forward_index.get(&(source, rel_type)) else {
            return out;
        };
        for &id in ids {
            let Some(rel) = self.relations.get(id as usize) else {
                continue;
            };
            if rel.confidence >= min_confidence {
                out.insert(rel.target);
            }
        }
        out
    }

    /// Get all sources that reach `target` via `rel_type`.
    pub fn sources(&self, target: u32, rel_type: StrId) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        if let Some(ids) = self.backward_index.get(&(target, rel_type)) {
            for &id in ids {
                if let Some(rel) = self.relations.get(id as usize) {
                    result.insert(rel.source);
                }
            }
        }
        result
    }

    /// Get all sources that reach `target` via `rel_type`, but only counting
    /// edges whose `confidence >= min_confidence`.
    pub fn sources_with_min_confidence(
        &self,
        target: u32,
        rel_type: StrId,
        min_confidence: f32,
    ) -> RoaringBitmap {
        let min_confidence = min_confidence.clamp(0.0, 1.0);
        let mut out = RoaringBitmap::new();
        let Some(ids) = self.backward_index.get(&(target, rel_type)) else {
            return out;
        };
        for &id in ids {
            let Some(rel) = self.relations.get(id as usize) else {
                continue;
            };
            if rel.confidence >= min_confidence {
                out.insert(rel.source);
            }
        }
        out
    }

    /// Check whether an edge exists: `source -[rel_type]-> target`.
    pub fn has_edge(&self, source: u32, rel_type: StrId, target: u32) -> bool {
        let Some(ids) = self.forward_index.get(&(source, rel_type)) else {
            return false;
        };
        for &id in ids {
            if let Some(rel) = self.relations.get(id as usize) {
                if rel.target == target {
                    return true;
                }
            }
        }
        false
    }

    /// Check whether an edge exists: `source -[rel_type]-> target` with
    /// `confidence >= min_confidence`.
    pub fn has_edge_with_min_confidence(
        &self,
        source: u32,
        rel_type: StrId,
        target: u32,
        min_confidence: f32,
    ) -> bool {
        self.edge_relation_id_with_min_confidence(source, rel_type, target, min_confidence)
            .is_some()
    }

    /// Get a relation by its stable relation id.
    pub fn get_relation(&self, relation_id: u32) -> Option<&Relation> {
        self.relations.get(relation_id as usize)
    }

    /// Outgoing relation ids for a fixed `(source, rel_type)` pair.
    ///
    /// Returns an empty slice if no such relations exist.
    pub fn outgoing_relation_ids(&self, source: u32, rel_type: StrId) -> &[u32] {
        self.forward_index
            .get(&(source, rel_type))
            .map(|ids| ids.as_slice())
            .unwrap_or(&[])
    }

    /// Pick one relation id witnessing `source -[rel_type]-> target`, if present.
    ///
    /// Note: PathDB may contain multiple edges with the same endpoints and label
    /// (e.g. differing attributes/confidence). For certificates we only need one.
    pub fn edge_relation_id(&self, source: u32, rel_type: StrId, target: u32) -> Option<u32> {
        let ids = self.forward_index.get(&(source, rel_type))?;
        for &id in ids {
            let rel = self.relations.get(id as usize)?;
            if rel.target == target {
                return Some(id);
            }
        }
        None
    }

    /// Pick a relation id witnessing `source -[rel_type]-> target` with
    /// `confidence >= min_confidence`, if present.
    ///
    /// If multiple such relations exist, the highest-confidence edge wins;
    /// ties break by smaller `relation_id` for determinism.
    pub fn edge_relation_id_with_min_confidence(
        &self,
        source: u32,
        rel_type: StrId,
        target: u32,
        min_confidence: f32,
    ) -> Option<u32> {
        let min_confidence = min_confidence.clamp(0.0, 1.0);
        let ids = self.forward_index.get(&(source, rel_type))?;

        let mut best_id: Option<u32> = None;
        let mut best_conf: f32 = -1.0;

        for &id in ids {
            let rel = self.relations.get(id as usize)?;
            if rel.target != target {
                continue;
            }
            if rel.confidence < min_confidence {
                continue;
            }

            if rel.confidence > best_conf {
                best_conf = rel.confidence;
                best_id = Some(id);
            } else if (rel.confidence == best_conf) && best_id.map(|b| id < b).unwrap_or(true) {
                best_id = Some(id);
            }
        }

        best_id
    }
}

// ============================================================================
// Path Index (Pre-computed Path Signatures)
// ============================================================================

/// A path signature: sequence of relation types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathSig(Vec<StrId>);

impl PathSig {
    pub fn new(rel_types: Vec<StrId>) -> Self {
        Self(rel_types)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Pre-computed path index for fast multi-hop queries
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PathIndex {
    /// path_sig -> (start_entity -> reachable_entities)
    index: HashMap<PathSig, HashMap<u32, RoaringBitmap>>,
    /// Maximum indexed path length
    max_depth: usize,
}

impl PathIndex {
    pub fn new(max_depth: usize) -> Self {
        Self {
            index: HashMap::new(),
            max_depth,
        }
    }

    /// Build path index from relation store
    pub fn build(
        &mut self,
        entities: &EntityStore,
        relations: &RelationStore,
        interner: &StringInterner,
    ) {
        // Index single-hop paths
        for rel in &relations.relations {
            let sig = PathSig::new(vec![rel.rel_type]);
            self.index
                .entry(sig)
                .or_insert_with(HashMap::new)
                .entry(rel.source)
                .or_insert_with(RoaringBitmap::new)
                .insert(rel.target);
        }

        // Build multi-hop paths iteratively
        for depth in 2..=self.max_depth {
            let prev_sigs: Vec<PathSig> = self
                .index
                .keys()
                .filter(|s| s.len() == depth - 1)
                .cloned()
                .collect();

            for prev_sig in prev_sigs {
                // Get all relation types
                let rel_types: Vec<StrId> = relations.type_index.keys().copied().collect();

                for rel_type in rel_types {
                    let mut new_sig = prev_sig.0.clone();
                    new_sig.push(rel_type);
                    let new_path_sig = PathSig::new(new_sig);

                    // Compute reachability
                    if let Some(prev_reach) = self.index.get(&prev_sig) {
                        let mut new_reach: HashMap<u32, RoaringBitmap> = HashMap::new();

                        for (&start, intermediates) in prev_reach {
                            let mut targets = RoaringBitmap::new();
                            for intermediate in intermediates.iter() {
                                targets |= relations.targets(intermediate, rel_type);
                            }
                            if !targets.is_empty() {
                                new_reach.insert(start, targets);
                            }
                        }

                        if !new_reach.is_empty() {
                            self.index.insert(new_path_sig, new_reach);
                        }
                    }
                }
            }
        }
    }

    /// Query entities reachable via path
    pub fn query(&self, start: u32, path: &PathSig) -> Option<&RoaringBitmap> {
        self.index.get(path)?.get(&start)
    }

    /// Query all starts that can reach target via path (reverse query)
    pub fn reverse_query(&self, target: u32, path: &PathSig) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        if let Some(reach_map) = self.index.get(path) {
            for (&start, targets) in reach_map {
                if targets.contains(target) {
                    result.insert(start);
                }
            }
        }
        result
    }
}

// ============================================================================
// PathDB: The Complete Database
// ============================================================================

/// PathDB: Efficient binary path-indexed knowledge graph
#[derive(Serialize, Deserialize)]
pub struct PathDB {
    #[serde(skip)]
    db_token: DbToken,
    /// String interner for compact storage
    #[serde(skip)]
    pub interner: StringInterner,
    /// Entity storage
    pub entities: EntityStore,
    /// Relation storage
    pub relations: RelationStore,
    /// Path index
    pub path_index: PathIndex,
    /// Equivalence index: entity -> [(equiv_entity, equiv_type)]
    pub equivalences: HashMap<u32, Vec<(u32, StrId)>>,
    /// Confidence index: relation_id -> confidence
    /// (allows fast filtering by confidence)
    confidence_index: Vec<f32>,
    /// Cached fact-node lookup indexes (rebuilt on demand).
    #[serde(skip)]
    fact_index: FactIndexCache,
    /// Cached inverted indexes for attribute full-text search (rebuilt on demand).
    #[serde(skip)]
    text_index: TextIndexCache,
}

impl PathDB {
    pub fn new() -> Self {
        Self {
            db_token: DbToken::new(),
            interner: StringInterner::new(),
            entities: EntityStore::new(),
            relations: RelationStore::new(),
            path_index: PathIndex::new(3), // Index up to 3-hop paths
            equivalences: HashMap::new(),
            confidence_index: Vec::new(),
            fact_index: FactIndexCache::default(),
            text_index: TextIndexCache::default(),
        }
    }

    pub fn db_token(&self) -> DbToken {
        self.db_token
    }

    /// Add an entity
    pub fn add_entity(&mut self, type_name: &str, attrs: Vec<(&str, &str)>) -> u32 {
        self.fact_index.invalidate();
        self.text_index.invalidate();
        let type_id = self.interner.intern(type_name);
        let interned_attrs: Vec<(StrId, StrId)> = attrs
            .into_iter()
            .map(|(k, v)| (self.interner.intern(k), self.interner.intern(v)))
            .collect();
        self.entities.add(type_id, interned_attrs)
    }

    /// Upsert a single entity attribute (extension-layer convenience).
    ///
    /// This supports continuous ingest / reconciliation workflows where multiple
    /// overlays want to enrich the same entity over time (e.g. preserve `iri`,
    /// `label`, `comment`, extracted metadata, etc).
    ///
    /// Note: this mutates the snapshot and invalidates dependent caches.
    pub fn upsert_entity_attr(&mut self, entity_id: u32, key: &str, value: &str) -> Result<()> {
        if entity_id as usize >= self.entities.types.len() {
            return Err(anyhow::anyhow!("unknown entity id {entity_id}"));
        }

        self.fact_index.invalidate();
        self.text_index.invalidate();

        let key_id = self.interner.intern(key);
        let value_id = self.interner.intern(value);
        self.entities
            .attrs
            .entry(key_id)
            .or_insert_with(HashMap::new)
            .insert(entity_id, value_id);
        Ok(())
    }

    /// Mark an entity as belonging to an additional type set (a "virtual type").
    ///
    /// PathDB stores a single canonical type per entity, but many workflows want
    /// derived supertypes and/or evidence-plane types without rewriting entity
    /// records. This inserts the entity into the type index for `type_name`,
    /// allowing queries like `?x is T` to match it.
    pub fn mark_virtual_type(&mut self, entity_id: u32, type_name: &str) -> Result<()> {
        if entity_id as usize >= self.entities.types.len() {
            return Err(anyhow::anyhow!("unknown entity id {entity_id}"));
        }

        self.fact_index.invalidate();
        let type_id = self.interner.intern(type_name);
        self.entities
            .type_index
            .entry(type_id)
            .or_insert_with(RoaringBitmap::new)
            .insert(entity_id);
        Ok(())
    }

    /// Add a relation
    pub fn add_relation(
        &mut self,
        rel_type: &str,
        source: u32,
        target: u32,
        confidence: f32,
        attrs: Vec<(&str, &str)>,
    ) -> u32 {
        self.fact_index.invalidate();
        let rel_type_id = self.interner.intern(rel_type);
        let interned_attrs: Vec<(StrId, StrId)> = attrs
            .into_iter()
            .map(|(k, v)| (self.interner.intern(k), self.interner.intern(v)))
            .collect();

        let rel = Relation {
            rel_type: rel_type_id,
            source,
            target,
            confidence,
            attrs: interned_attrs,
        };

        self.confidence_index.push(confidence);
        self.relations.add(rel)
    }

    /// Add an equivalence
    pub fn add_equivalence(&mut self, e1: u32, e2: u32, equiv_type: &str) {
        // Equivalences don't affect fact-node lookup, but we treat this as a DB mutation
        // and invalidate for simplicity (keeps future dependent caches correct).
        self.fact_index.invalidate();
        let equiv_type_id = self.interner.intern(equiv_type);
        self.equivalences
            .entry(e1)
            .or_insert_with(Vec::new)
            .push((e2, equiv_type_id));
        self.equivalences
            .entry(e2)
            .or_insert_with(Vec::new)
            .push((e1, equiv_type_id));
    }

    /// Build indexes (call after loading data)
    pub fn build_indexes(&mut self) {
        self.path_index
            .build(&self.entities, &self.relations, &self.interner);
    }

    // ========================================================================
    // Query Operations
    // ========================================================================

    /// Find entities by type (bitmap result for efficient joins)
    pub fn find_by_type(&self, type_name: &str) -> Option<&RoaringBitmap> {
        let type_id = self.interner.id_of(type_name)?;
        self.entities.by_type(type_id)
    }

    /// Find entities where `attr(key)` contains `needle` (case-insensitive).
    ///
    /// This is an **approximate** / convenience operation intended for REPL and
    /// discovery workflows (GraphRAG → proposals → canonical `.axi`).
    ///
    /// For certified querying, prefer exact constraints (`attr_eq`) and treat
    /// fuzzy matching as evidence-plane tooling.
    pub fn entities_with_attr_contains(&self, key: &str, needle: &str) -> RoaringBitmap {
        let Some(key_id) = self.interner.id_of(key) else {
            return RoaringBitmap::new();
        };
        let Some(col) = self.entities.attrs.get(&key_id) else {
            return RoaringBitmap::new();
        };

        let needle = needle.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return RoaringBitmap::new();
        }

        let mut out = RoaringBitmap::new();
        for (&entity_id, &value_id) in col {
            let Some(value) = self.interner.lookup(value_id) else {
                continue;
            };
            if value.to_ascii_lowercase().contains(&needle) {
                out.insert(entity_id);
            }
        }
        out
    }

    /// Find entities where `attr(key)` matches a full-text query (token-based).
    ///
    /// Semantics:
    /// - tokenization: split on non-alphanumeric (including `_` and `.`),
    ///   split camelCase/PascalCase boundaries, lowercased
    /// - query: split into tokens; result entities must contain **all** tokens
    ///
    /// This is intended for discovery workflows (LLM grounding, doc search) and
    /// is **not** part of the certified query core.
    pub fn entities_with_attr_fts(&self, key: &str, query: &str) -> RoaringBitmap {
        let Some(key_id) = self.interner.id_of(key) else {
            return RoaringBitmap::new();
        };
        let tokens = text_index::tokenize_query(query);
        self.text_index.query_all_tokens(self, key_id, &tokens)
    }

    /// Like `entities_with_attr_fts`, but uses OR semantics (any token match).
    pub fn entities_with_attr_fts_any(&self, key: &str, query: &str) -> RoaringBitmap {
        let Some(key_id) = self.interner.id_of(key) else {
            return RoaringBitmap::new();
        };
        let tokens = text_index::tokenize_query(query);
        self.text_index.query_any_tokens(self, key_id, &tokens)
    }

    /// Find entities where `attr(key)` is within a Levenshtein distance of
    /// `max_dist` from `needle` (case-insensitive).
    ///
    /// This is intended for approximate discovery flows, not certified querying.
    pub fn entities_with_attr_fuzzy(
        &self,
        key: &str,
        needle: &str,
        max_dist: usize,
    ) -> RoaringBitmap {
        let Some(key_id) = self.interner.id_of(key) else {
            return RoaringBitmap::new();
        };
        let Some(col) = self.entities.attrs.get(&key_id) else {
            return RoaringBitmap::new();
        };

        let needle = needle.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return RoaringBitmap::new();
        }

        let max_dist = max_dist.min(16);

        let needle_chars: Vec<char> = needle.chars().collect();

        let mut out = RoaringBitmap::new();
        for (&entity_id, &value_id) in col {
            let Some(value) = self.interner.lookup(value_id) else {
                continue;
            };
            let value = value.to_ascii_lowercase();
            if levenshtein_with_max(&value, &needle_chars, max_dist) <= max_dist {
                out.insert(entity_id);
            }
        }
        out
    }

    /// Resolve an entity into human-readable strings (type + attributes).
    pub fn get_entity(&self, entity_id: u32) -> Option<EntityView> {
        let type_id = self.entities.get_type(entity_id)?;
        let entity_type = self.interner.lookup(type_id)?;

        let mut attrs: HashMap<String, String> = HashMap::new();
        for (attr_name_id, col) in &self.entities.attrs {
            if let Some(value_id) = col.get(&entity_id) {
                let Some(name) = self.interner.lookup(*attr_name_id) else {
                    continue;
                };
                let Some(value) = self.interner.lookup(*value_id) else {
                    continue;
                };
                attrs.insert(name, value);
            }
        }

        Some(EntityView {
            id: entity_id,
            entity_type,
            attrs,
        })
    }

    /// Follow a single relation from source
    pub fn follow_one(&self, source: u32, rel_type: &str) -> RoaringBitmap {
        let Some(rel_type_id) = self.interner.id_of(rel_type) else {
            return RoaringBitmap::new();
        };
        self.relations.targets(source, rel_type_id)
    }

    /// Follow a single relation from `source`, counting only edges whose
    /// `confidence >= min_confidence`.
    pub fn follow_one_with_min_confidence(
        &self,
        source: u32,
        rel_type: &str,
        min_confidence: f32,
    ) -> RoaringBitmap {
        let Some(rel_type_id) = self.interner.id_of(rel_type) else {
            return RoaringBitmap::new();
        };
        self.relations
            .targets_with_min_confidence(source, rel_type_id, min_confidence)
    }

    /// Follow a path of relations
    pub fn follow_path(&self, start: u32, path: &[&str]) -> RoaringBitmap {
        let mut rel_ids = Vec::with_capacity(path.len());
        for rel in path {
            let Some(id) = self.interner.id_of(rel) else {
                return RoaringBitmap::new();
            };
            rel_ids.push(id);
        }
        let path_sig = PathSig::new(rel_ids);

        // Try indexed path first
        if let Some(result) = self.path_index.query(start, &path_sig) {
            return result.clone();
        }

        // Fall back to iterative traversal
        let mut current = RoaringBitmap::new();
        current.insert(start);

        for rel_type in path {
            let Some(rel_type_id) = self.interner.id_of(rel_type) else {
                return RoaringBitmap::new();
            };
            let mut next = RoaringBitmap::new();
            for entity in current.iter() {
                next |= self.relations.targets(entity, rel_type_id);
            }
            current = next;
            if current.is_empty() {
                break;
            }
        }

        current
    }

    /// Follow a path of relations, counting only edges whose
    /// `confidence >= min_confidence`.
    ///
    /// Note: This intentionally does **not** use the `PathIndex` (which is
    /// currently confidence-agnostic).
    pub fn follow_path_with_min_confidence(
        &self,
        start: u32,
        path: &[&str],
        min_confidence: f32,
    ) -> RoaringBitmap {
        let mut current = RoaringBitmap::new();
        current.insert(start);

        for rel_type in path {
            let Some(rel_type_id) = self.interner.id_of(rel_type) else {
                return RoaringBitmap::new();
            };
            let mut next = RoaringBitmap::new();
            for entity in current.iter() {
                next |=
                    self.relations
                        .targets_with_min_confidence(entity, rel_type_id, min_confidence);
            }
            current = next;
            if current.is_empty() {
                break;
            }
        }

        current
    }

    /// Find paths between two entities
    pub fn find_paths(&self, from: u32, to: u32, max_depth: usize) -> Vec<Vec<StrId>> {
        let mut results = Vec::new();
        let mut queue: Vec<(u32, Vec<StrId>)> = vec![(from, vec![])];
        let mut visited = RoaringBitmap::new();
        visited.insert(from);

        while let Some((current, path)) = queue.pop() {
            if path.len() >= max_depth {
                continue;
            }

            // Check all outgoing relations
            for rel in &self.relations.relations {
                if rel.source == current && !visited.contains(rel.target) {
                    let mut new_path = path.clone();
                    new_path.push(rel.rel_type);

                    if rel.target == to {
                        results.push(new_path);
                    } else {
                        visited.insert(rel.target);
                        queue.push((rel.target, new_path));
                    }
                }
            }
        }

        results
    }

    /// Find paths between two entities, using only edges whose
    /// `confidence >= min_confidence`.
    pub fn find_paths_with_min_confidence(
        &self,
        from: u32,
        to: u32,
        max_depth: usize,
        min_confidence: f32,
    ) -> Vec<Vec<StrId>> {
        let min_confidence = min_confidence.clamp(0.0, 1.0);

        let mut results = Vec::new();
        let mut queue: Vec<(u32, Vec<StrId>)> = vec![(from, vec![])];
        let mut visited = RoaringBitmap::new();
        visited.insert(from);

        while let Some((current, path)) = queue.pop() {
            if path.len() >= max_depth {
                continue;
            }

            for rel in &self.relations.relations {
                if rel.confidence < min_confidence {
                    continue;
                }
                if rel.source == current && !visited.contains(rel.target) {
                    let mut new_path = path.clone();
                    new_path.push(rel.rel_type);

                    if rel.target == to {
                        results.push(new_path);
                    } else {
                        visited.insert(rel.target);
                        queue.push((rel.target, new_path));
                    }
                }
            }
        }

        results
    }

    /// Find equivalent entities
    pub fn find_equivalent(&self, entity: u32) -> Vec<(u32, StrId)> {
        self.equivalences.get(&entity).cloned().unwrap_or_default()
    }

    /// Join two entity sets (bitmap intersection)
    pub fn join(&self, a: &RoaringBitmap, b: &RoaringBitmap) -> RoaringBitmap {
        a & b
    }

    /// Union two entity sets
    pub fn union(&self, a: &RoaringBitmap, b: &RoaringBitmap) -> RoaringBitmap {
        a | b
    }

    /// Filter by minimum confidence
    pub fn filter_by_confidence(
        &self,
        rel_ids: impl Iterator<Item = u32>,
        min_conf: f32,
    ) -> Vec<u32> {
        rel_ids
            .filter(|&id| {
                self.confidence_index
                    .get(id as usize)
                    .copied()
                    .unwrap_or(0.0)
                    >= min_conf
            })
            .collect()
    }

    // ========================================================================
    // Serialization
    // ========================================================================

    /// Serialize to binary format
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let interner_bytes = self.interner.to_bytes();
        let db_bytes = bincode::serialize(&(
            &self.entities,
            &self.relations,
            &self.path_index,
            &self.equivalences,
            &self.confidence_index,
        ))?;

        let mut result = Vec::new();
        // Header: magic number + version
        result.extend_from_slice(b"AXPD"); // Axiograph PathDB
        result.extend_from_slice(&1u32.to_le_bytes()); // version 1

        // Interner
        result.extend_from_slice(&(interner_bytes.len() as u64).to_le_bytes());
        result.extend_from_slice(&interner_bytes);

        // DB
        result.extend_from_slice(&(db_bytes.len() as u64).to_le_bytes());
        result.extend_from_slice(&db_bytes);

        Ok(result)
    }

    /// Deserialize from binary format
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        // Check header
        if bytes.len() < 8 || &bytes[0..4] != b"AXPD" {
            return Err(anyhow::anyhow!("Invalid PathDB file"));
        }

        let version = u32::from_le_bytes(bytes[4..8].try_into()?);
        if version != 1 {
            return Err(anyhow::anyhow!("Unsupported PathDB version: {}", version));
        }

        let mut offset = 8;

        // Interner
        let interner_len = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?) as usize;
        offset += 8;
        let interner = StringInterner::from_bytes(&bytes[offset..offset + interner_len])?;
        offset += interner_len;

        // DB
        let db_len = u64::from_le_bytes(bytes[offset..offset + 8].try_into()?) as usize;
        offset += 8;
        let (entities, relations, path_index, equivalences, confidence_index): (
            EntityStore,
            RelationStore,
            PathIndex,
            HashMap<u32, Vec<(u32, StrId)>>,
            Vec<f32>,
        ) = bincode::deserialize(&bytes[offset..offset + db_len])?;

        Ok(Self {
            db_token: DbToken::new(),
            interner,
            entities,
            relations,
            path_index,
            equivalences,
            confidence_index,
            fact_index: FactIndexCache::default(),
            text_index: TextIndexCache::default(),
        })
    }
}

impl Default for PathDB {
    fn default() -> Self {
        Self::new()
    }
}

impl PathDB {
    /// Fact nodes whose `axi_relation` attribute matches `relation_name`.
    ///
    /// This is a fast path for AxQL "fact atom" queries; it uses a cached
    /// in-memory index rather than scanning the attribute column repeatedly.
    pub fn fact_nodes_by_axi_relation(&self, relation_name: &str) -> RoaringBitmap {
        let Some(relation_id) = self.interner.id_of(relation_name) else {
            return RoaringBitmap::new();
        };
        self.fact_index.with_index(self, |idx| {
            idx.facts_by_relation(relation_id)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// Fact nodes whose `(axi_schema, axi_relation)` match the provided names.
    pub fn fact_nodes_by_axi_schema_relation(
        &self,
        schema_name: &str,
        relation_name: &str,
    ) -> RoaringBitmap {
        let Some(schema_id) = self.interner.id_of(schema_name) else {
            return RoaringBitmap::new();
        };
        let Some(relation_id) = self.interner.id_of(relation_name) else {
            return RoaringBitmap::new();
        };
        self.fact_index.with_index(self, |idx| {
            idx.facts_by_schema_relation(schema_id, relation_id)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// Fact nodes scoped to a specific context/world (by entity id).
    ///
    /// Context scoping is optional: facts without a `axi_fact_in_context` edge
    /// are simply absent from all context-specific indexes.
    pub fn fact_nodes_by_context(&self, context_entity_id: u32) -> RoaringBitmap {
        self.fact_index.with_index(self, |idx| {
            idx.facts_by_context(context_entity_id)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// Fact nodes scoped to a context/world and constrained to a `(axi_schema, axi_relation)` pair.
    pub fn fact_nodes_by_context_axi_schema_relation(
        &self,
        context_entity_id: u32,
        schema_name: &str,
        relation_name: &str,
    ) -> RoaringBitmap {
        let Some(schema_id) = self.interner.id_of(schema_name) else {
            return RoaringBitmap::new();
        };
        let Some(relation_id) = self.interner.id_of(relation_name) else {
            return RoaringBitmap::new();
        };
        self.fact_index.with_index(self, |idx| {
            idx.facts_by_context_schema_relation(context_entity_id, schema_id, relation_id)
                .cloned()
                .unwrap_or_default()
        })
    }

    /// Key-based fact lookup (best-effort).
    ///
    /// Returns `None` when the key index is not available (e.g. no key
    /// constraint was imported for that relation).
    ///
    /// When present, this can turn some AxQL fact queries into near-index
    /// lookups by avoiding full attribute scans and join search.
    pub fn fact_nodes_by_axi_key(
        &self,
        schema_name: &str,
        relation_name: &str,
        key_fields_in_order: &[&str],
        key_values_in_order: &[u32],
    ) -> Option<Vec<u32>> {
        let Some(schema_id) = self.interner.id_of(schema_name) else {
            return Some(Vec::new());
        };
        let Some(relation_id) = self.interner.id_of(relation_name) else {
            return Some(Vec::new());
        };
        if key_fields_in_order.len() != key_values_in_order.len() {
            return Some(Vec::new());
        }

        let mut key_fields: Vec<StrId> = Vec::with_capacity(key_fields_in_order.len());
        for f in key_fields_in_order {
            let Some(fid) = self.interner.id_of(f) else {
                return Some(Vec::new());
            };
            key_fields.push(fid);
        }

        let sig = crate::fact_index::FactKeySignature {
            schema_id,
            relation_id,
            key_fields,
        };

        self.fact_index.with_index(self, |idx| {
            idx.lookup_key(&sig, key_values_in_order).cloned()
        })
    }
}

/// Levenshtein distance with an early-exit cap.
///
/// Returns a value in `[0, max_dist]` when the true distance is within the
/// bound; otherwise returns `max_dist + 1`.
///
/// Notes:
/// - This implementation uses standard dynamic programming with an early-exit
///   row-min bound, which is sufficient given our small `max_dist` cap (≤ 16).
/// - Callers are expected to normalize case if they want case-insensitive
///   behavior (we do so in `entities_with_attr_fuzzy`).
fn levenshtein_with_max(value: &str, needle_chars: &[char], max_dist: usize) -> usize {
    if max_dist == 0 {
        return if value.chars().eq(needle_chars.iter().copied()) {
            0
        } else {
            1
        };
    }

    let n = needle_chars.len();
    if n == 0 {
        return 0;
    }

    // DP rows: distances between `value[..i]` and `needle[..j]`.
    //
    // We keep only two rows; values will never exceed `max_dist + value_len`,
    // but for early exit we only care whether the row minimum exceeds
    // `max_dist`.
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for (i, c) in value.chars().enumerate() {
        curr[0] = i + 1;
        let mut row_min = curr[0];

        for j in 1..=n {
            let cost = if c == needle_chars[j - 1] { 0 } else { 1 };
            let deletion = prev[j] + 1;
            let insertion = curr[j - 1] + 1;
            let substitution = prev[j - 1] + cost;
            let d = deletion.min(insertion).min(substitution);
            curr[j] = d;
            row_min = row_min.min(d);
        }

        if row_min > max_dist {
            return max_dist + 1;
        }

        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// ============================================================================
// SQL-like Query Interface
// ============================================================================

/// SQL-like query for PathDB
#[derive(Debug, Clone)]
pub enum PathQuery {
    /// SELECT * FROM entities WHERE type = ?
    SelectByType(String),
    /// SELECT target FROM relations WHERE source = ? AND rel_type = ?
    SelectRelated(u32, String),
    /// Follow path: source -[rel1]-> -[rel2]-> ... -> targets
    FollowPath { start: u32, path: Vec<String> },
    /// Find paths between two entities
    FindPaths {
        from: u32,
        to: u32,
        max_depth: usize,
    },
    /// Join two queries (intersection)
    Join(Box<PathQuery>, Box<PathQuery>),
    /// Union two queries
    Union(Box<PathQuery>, Box<PathQuery>),
    /// Filter by confidence
    WithConfidence {
        base: Box<PathQuery>,
        min_confidence: f32,
    },
}

/// Optional execution trace events (recorded only when proofs are enabled).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum QueryExecutionEvent {
    SelectByType {
        type_name: String,
    },
    SelectRelated {
        source: u32,
        rel_type: String,
    },
    FollowPath {
        start: u32,
        path: Vec<String>,
    },
    FindPaths {
        from: u32,
        to: u32,
        max_depth: usize,
    },
    Join,
    Union,
    WithConfidence {
        min_confidence: f32,
    },
}

impl PathDB {
    /// Execute a PathQuery
    pub fn execute(&self, query: &PathQuery) -> RoaringBitmap {
        use crate::proof_mode::{NoProof, ProofJournal};
        let mut journal: ProofJournal<NoProof, QueryExecutionEvent> = ProofJournal::new();
        self.execute_with_journal(query, &mut journal)
    }

    /// Execute a PathQuery and optionally capture a trace (generic over `ProofMode`).
    pub fn execute_with_mode<M: crate::proof_mode::ProofMode>(
        &self,
        query: &PathQuery,
    ) -> crate::proof_mode::Proved<M, RoaringBitmap, Vec<QueryExecutionEvent>> {
        use crate::proof_mode::{ProofJournal, Proved};
        let mut journal: ProofJournal<M, QueryExecutionEvent> = ProofJournal::new();
        let result = self.execute_with_journal(query, &mut journal);
        Proved {
            value: result,
            proof: journal.into_entries(),
        }
    }

    fn execute_with_journal<M: crate::proof_mode::ProofMode>(
        &self,
        query: &PathQuery,
        journal: &mut crate::proof_mode::ProofJournal<M, QueryExecutionEvent>,
    ) -> RoaringBitmap {
        self.execute_with_journal_conf(query, journal, None)
    }

    fn execute_with_journal_conf<M: crate::proof_mode::ProofMode>(
        &self,
        query: &PathQuery,
        journal: &mut crate::proof_mode::ProofJournal<M, QueryExecutionEvent>,
        min_confidence: Option<f32>,
    ) -> RoaringBitmap {
        match query {
            PathQuery::SelectByType(type_name) => {
                journal.record(|| QueryExecutionEvent::SelectByType {
                    type_name: type_name.clone(),
                });
                self.find_by_type(type_name).cloned().unwrap_or_default()
            }
            PathQuery::SelectRelated(source, rel_type) => {
                journal.record(|| QueryExecutionEvent::SelectRelated {
                    source: *source,
                    rel_type: rel_type.clone(),
                });
                match min_confidence {
                    None => self.follow_one(*source, rel_type),
                    Some(min) => self.follow_one_with_min_confidence(*source, rel_type, min),
                }
            }
            PathQuery::FollowPath { start, path } => {
                journal.record(|| QueryExecutionEvent::FollowPath {
                    start: *start,
                    path: path.clone(),
                });
                let path_refs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
                match min_confidence {
                    None => self.follow_path(*start, &path_refs),
                    Some(min) => self.follow_path_with_min_confidence(*start, &path_refs, min),
                }
            }
            PathQuery::FindPaths {
                from,
                to,
                max_depth,
            } => {
                journal.record(|| QueryExecutionEvent::FindPaths {
                    from: *from,
                    to: *to,
                    max_depth: *max_depth,
                });
                // Returns entities at the end of paths (just the target)
                let paths = match min_confidence {
                    None => self.find_paths(*from, *to, *max_depth),
                    Some(min) => self.find_paths_with_min_confidence(*from, *to, *max_depth, min),
                };
                let mut result = RoaringBitmap::new();
                if !paths.is_empty() {
                    result.insert(*to);
                }
                result
            }
            PathQuery::Join(left, right) => {
                journal.record(|| QueryExecutionEvent::Join);
                let left_result = self.execute_with_journal_conf(left, journal, min_confidence);
                let right_result = self.execute_with_journal_conf(right, journal, min_confidence);
                self.join(&left_result, &right_result)
            }
            PathQuery::Union(left, right) => {
                journal.record(|| QueryExecutionEvent::Union);
                let left_result = self.execute_with_journal_conf(left, journal, min_confidence);
                let right_result = self.execute_with_journal_conf(right, journal, min_confidence);
                self.union(&left_result, &right_result)
            }
            PathQuery::WithConfidence {
                base,
                min_confidence: edge_min_confidence,
            } => {
                journal.record(|| QueryExecutionEvent::WithConfidence {
                    min_confidence: *edge_min_confidence,
                });
                let next_min = match min_confidence {
                    None => *edge_min_confidence,
                    Some(prev) => prev.max(*edge_min_confidence),
                };
                self.execute_with_journal_conf(base, journal, Some(next_min))
            }
        }
    }
}

// ============================================================================
// Vector DB Bridge (for hybrid queries)
// ============================================================================

/// Chunk reference for vector DB integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRef {
    pub chunk_id: String,
    pub entity_id: u32,
    pub text: String,
    pub embedding_id: Option<u64>, // ID in vector DB
}

/// Vector search result
#[derive(Debug, Clone)]
pub struct VectorResult {
    pub chunk_id: String,
    pub entity_id: u32,
    pub similarity: f32,
}

impl PathDB {
    /// Combine vector search results with path query
    /// Returns entities that match both vector search AND path constraints
    pub fn hybrid_query(
        &self,
        vector_results: Vec<VectorResult>,
        path_query: &PathQuery,
    ) -> Vec<(u32, f32)> {
        // Get path query results as bitmap
        let path_entities = self.execute(path_query);

        // Filter vector results to those matching path query
        vector_results
            .into_iter()
            .filter(|vr| path_entities.contains(vr.entity_id))
            .map(|vr| (vr.entity_id, vr.similarity))
            .collect()
    }

    /// Expand vector search results via paths
    /// "Find related entities to these vector matches"
    pub fn expand_by_path(
        &self,
        vector_results: Vec<VectorResult>,
        path: &[&str],
    ) -> RoaringBitmap {
        let mut expanded = RoaringBitmap::new();
        for vr in vector_results {
            expanded |= self.follow_path(vr.entity_id, path);
        }
        expanded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut db = PathDB::new();

        // Add entities
        let alice = db.add_entity("Person", vec![("name", "Alice")]);
        let bob = db.add_entity("Person", vec![("name", "Bob")]);
        let carol = db.add_entity("Person", vec![("name", "Carol")]);

        // Add relations
        db.add_relation("knows", alice, bob, 1.0, vec![]);
        db.add_relation("knows", bob, carol, 0.8, vec![]);

        // Build indexes
        db.build_indexes();

        // Query
        let knows_bob = db.follow_one(alice, "knows");
        assert!(knows_bob.contains(bob));

        // Path query
        let two_hop = db.follow_path(alice, &["knows", "knows"]);
        assert!(two_hop.contains(carol));
    }
}
