//! FactIndex: fast lookup structures for canonical `.axi` "fact nodes".
//!
//! Canonical `.axi` instances are imported into PathDB by reifying each n-ary
//! relation tuple as a dedicated **fact node**:
//!
//! - `axi_relation = <relation name>`
//! - `axi_schema   = <schema name>`
//! - edges `fact -field-> value` for each declared field
//!
//! AxQL frequently filters on `axi_relation` and then joins through the field
//! edges. Scanning attribute columns repeatedly can become a bottleneck for
//! interactive use (REPL) and for large snapshots.
//!
//! This module builds a rebuildable in-memory index that supports:
//! - `(axi_schema, axi_relation) -> {fact nodes}`
//! - `axi_relation -> {fact nodes}` (union across schemas)
//! - optional key-based lookups derived from meta-plane key constraints:
//!   `(axi_schema, axi_relation, key_fields, key_values) -> {fact nodes}`
//! - optional context scoping:
//!   `context_entity_id -> {fact nodes}`

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::axi_meta::{ATTR_AXI_RELATION, ATTR_AXI_SCHEMA, REL_AXI_FACT_IN_CONTEXT};
use crate::axi_semantics::{ConstraintDecl, MetaPlaneIndex};
use crate::{IndexSidecarWriter, PathDB, StrId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactKeySignature {
    pub schema_id: StrId,
    pub relation_id: StrId,
    pub key_fields: Vec<StrId>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FactIndex {
    /// Union across schemas: `axi_relation -> {fact nodes}`.
    by_relation: HashMap<StrId, RoaringBitmap>,
    /// Precise: `(axi_schema, axi_relation) -> {fact nodes}`.
    by_schema_relation: HashMap<(StrId, StrId), RoaringBitmap>,
    /// Optional context scoping: `ctx_entity_id -> {fact nodes}`.
    by_context: HashMap<u32, RoaringBitmap>,
    /// Optional context scoping: `(ctx_entity_id, axi_schema, axi_relation) -> {fact nodes}`.
    by_context_schema_relation: HashMap<(u32, StrId, StrId), RoaringBitmap>,
    /// Optional key indexes (built from meta-plane constraints when present).
    key_index: HashMap<FactKeySignature, HashMap<Vec<u32>, Vec<u32>>>,
}

impl FactIndex {
    pub(crate) fn build(db: &PathDB) -> Self {
        let mut out = FactIndex::default();

        let Some(relation_key_id) = db.interner.id_of(ATTR_AXI_RELATION) else {
            return out;
        };
        let schema_key_id = db.interner.id_of(ATTR_AXI_SCHEMA);

        let Some(rel_col) = db.entities.attrs.get(&relation_key_id) else {
            return out;
        };

        let context_rel_id = db.interner.id_of(REL_AXI_FACT_IN_CONTEXT);

        // Pass 1: gather fact nodes by relation and by (schema, relation).
        for (&entity_id, &relation_id) in rel_col {
            out.by_relation
                .entry(relation_id)
                .or_insert_with(RoaringBitmap::new)
                .insert(entity_id);

            // Optional: context/world scoping. If present, we index it alongside
            // the canonical `(schema, relation)` lookups.
            if let Some(context_rel_id) = context_rel_id {
                for &rid in db
                    .relations
                    .outgoing_relation_ids(entity_id, context_rel_id)
                {
                    let Some(rel) = db.relations.get_relation(rid) else {
                        continue;
                    };
                    out.by_context
                        .entry(rel.target)
                        .or_insert_with(RoaringBitmap::new)
                        .insert(entity_id);
                }
            }

            if let Some(schema_key_id) = schema_key_id {
                if let Some(schema_id) = db.entities.get_attr(entity_id, schema_key_id) {
                    out.by_schema_relation
                        .entry((schema_id, relation_id))
                        .or_insert_with(RoaringBitmap::new)
                        .insert(entity_id);

                    if let Some(context_rel_id) = context_rel_id {
                        for &rid in db
                            .relations
                            .outgoing_relation_ids(entity_id, context_rel_id)
                        {
                            let Some(rel) = db.relations.get_relation(rid) else {
                                continue;
                            };
                            out.by_context_schema_relation
                                .entry((rel.target, schema_id, relation_id))
                                .or_insert_with(RoaringBitmap::new)
                                .insert(entity_id);
                        }
                    }
                }
            }
        }

        // Pass 2: build key lookups from meta-plane constraints.
        //
        // This is best-effort:
        // - if the schema/theory plane is missing, we just don't build key indexes
        // - if a fact node is missing a key field edge, we skip indexing it for that key
        let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
        if meta.schemas.is_empty() {
            return out;
        }

        // `(schema_id, relation_id, key_fields)` -> key index map.
        let mut key_builders: HashMap<FactKeySignature, HashMap<Vec<u32>, Vec<u32>>> =
            HashMap::new();

        for (schema_name, schema) in &meta.schemas {
            let Some(schema_id) = db.interner.id_of(schema_name) else {
                continue;
            };

            for (relation_name, constraints) in &schema.constraints_by_relation {
                let Some(relation_id) = db.interner.id_of(relation_name) else {
                    continue;
                };

                // Only build key indexes when we have some facts for the pair.
                let Some(facts) = out.by_schema_relation.get(&(schema_id, relation_id)) else {
                    continue;
                };

                for c in constraints {
                    let ConstraintDecl::Key { fields, .. } = c else {
                        continue;
                    };
                    if fields.is_empty() {
                        continue;
                    }
                    // Keep the initial release conservative: don't build huge composite-key
                    // maps unless users explicitly need them.
                    if fields.len() > 8 {
                        continue;
                    }

                    let mut key_fields: Vec<StrId> = Vec::with_capacity(fields.len());
                    let mut ok = true;
                    for field in fields {
                        let Some(fid) = db.interner.id_of(field) else {
                            ok = false;
                            break;
                        };
                        key_fields.push(fid);
                    }
                    if !ok {
                        continue;
                    }

                    let sig = FactKeySignature {
                        schema_id,
                        relation_id,
                        key_fields: key_fields.clone(),
                    };

                    let index = key_builders.entry(sig).or_default();

                    for fact in facts.iter() {
                        let mut key_values: Vec<u32> = Vec::new();
                        let mut missing = false;

                        for &field_rel_id in &key_fields {
                            let ids = db.relations.outgoing_relation_ids(fact, field_rel_id);
                            let Some(&rid) = ids.first() else {
                                missing = true;
                                break;
                            };
                            let Some(rel) = db.relations.get_relation(rid) else {
                                missing = true;
                                break;
                            };
                            key_values.push(rel.target);
                        }

                        if missing {
                            continue;
                        }

                        index.entry(key_values).or_default().push(fact);
                    }
                }
            }
        }

        // Normalize key index value lists for determinism.
        for facts in key_builders.values_mut() {
            for ids in facts.values_mut() {
                ids.sort_unstable();
            }
        }

        out.key_index = key_builders;
        out
    }

    pub(crate) fn facts_by_relation(&self, relation_id: StrId) -> Option<&RoaringBitmap> {
        self.by_relation.get(&relation_id)
    }

    pub(crate) fn facts_by_schema_relation(
        &self,
        schema_id: StrId,
        relation_id: StrId,
    ) -> Option<&RoaringBitmap> {
        self.by_schema_relation.get(&(schema_id, relation_id))
    }

    pub(crate) fn facts_by_context(&self, context_entity_id: u32) -> Option<&RoaringBitmap> {
        self.by_context.get(&context_entity_id)
    }

    pub(crate) fn facts_by_context_schema_relation(
        &self,
        context_entity_id: u32,
        schema_id: StrId,
        relation_id: StrId,
    ) -> Option<&RoaringBitmap> {
        self.by_context_schema_relation
            .get(&(context_entity_id, schema_id, relation_id))
    }

    pub(crate) fn lookup_key(&self, sig: &FactKeySignature, values: &[u32]) -> Option<&Vec<u32>> {
        self.key_index.get(sig)?.get(values)
    }
}

#[derive(Debug)]
pub(crate) struct FactIndexCache {
    generation: AtomicU64,
    built_generation: AtomicU64,
    building_generation: AtomicU64,
    index: RwLock<FactIndex>,
    async_source: Mutex<Option<Weak<PathDB>>>,
    sidecar: Mutex<Option<Arc<IndexSidecarWriter>>>,
}

impl Default for FactIndexCache {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            built_generation: AtomicU64::new(u64::MAX),
            building_generation: AtomicU64::new(u64::MAX),
            index: RwLock::new(FactIndex::default()),
            async_source: Mutex::new(None),
            sidecar: Mutex::new(None),
        }
    }
}

impl FactIndexCache {
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    pub(crate) fn attach_async_source(&self, source: Weak<PathDB>) {
        let mut guard = self.async_source.lock().expect("fact index source poisoned");
        *guard = Some(source);
    }

    pub(crate) fn attach_sidecar_writer(&self, writer: Arc<IndexSidecarWriter>) {
        let mut guard = self.sidecar.lock().expect("fact index sidecar poisoned");
        *guard = Some(writer);
    }

    pub(crate) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn load_index(&self, index: FactIndex, generation: u64) {
        let mut guard = self.index.write().expect("fact index lock poisoned");
        *guard = index;
        self.built_generation.store(generation, Ordering::SeqCst);
    }

    pub(crate) fn snapshot(&self, generation: u64) -> Option<FactIndex> {
        if self.built_generation.load(Ordering::SeqCst) != generation {
            return None;
        }
        let guard = self.index.read().expect("fact index lock poisoned");
        Some(guard.clone())
    }

    fn schedule_build_async(&self, gen: u64) -> bool {
        let source = self
            .async_source
            .lock()
            .expect("fact index source poisoned")
            .clone();
        let Some(source) = source else {
            return false;
        };
        if self
            .building_generation
            .compare_exchange(u64::MAX, gen, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return true;
        }

        std::thread::Builder::new()
            .name("axiograph_fact_index".to_string())
            .spawn(move || {
                let Some(db) = source.upgrade() else {
                    return;
                };
                let new_index = FactIndex::build(&db);
                let cache = &db.fact_index;
                if cache.generation.load(Ordering::SeqCst) == gen {
                    cache.load_index(new_index, gen);
                    if let Some(writer) = cache
                        .sidecar
                        .lock()
                        .expect("fact index sidecar poisoned")
                        .as_ref()
                    {
                        writer.mark_dirty();
                    }
                }
                cache
                    .building_generation
                    .store(u64::MAX, Ordering::SeqCst);
            })
            .expect("failed to spawn fact index build thread");
        true
    }

    pub(crate) fn with_index_or_fallback<R>(
        &self,
        db: &PathDB,
        fallback: impl FnOnce(&PathDB) -> R,
        f: impl FnOnce(&FactIndex) -> R,
    ) -> R {
        let gen = self.generation.load(Ordering::SeqCst);
        if self.built_generation.load(Ordering::SeqCst) == gen {
            let guard = self.index.read().expect("fact index lock poisoned");
            return f(&guard);
        }

        if self.schedule_build_async(gen) {
            return fallback(db);
        }

        let new_index = FactIndex::build(db);
        self.load_index(new_index, gen);
        let guard = self.index.read().expect("fact index lock poisoned");
        f(&guard)
    }
}
