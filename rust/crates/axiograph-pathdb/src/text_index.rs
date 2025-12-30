//! TextIndex: lightweight inverted index for entity attribute values.
//!
//! Axiograph intentionally treats full-text search as an **extension layer**:
//!
//! - It is useful for discovery workflows (GraphRAG-style exploration, LLM grounding).
//! - It is not part of the certified core query semantics.
//! - It should be fast enough for interactive use without adding heavy deps.
//!
//! Design:
//! - Build an inverted index for one attribute column at a time:
//!   `attr_key_id -> token -> {entity_ids}`
//! - Cache per-attribute indexes in-memory, rebuilding on DB mutation.
//!
//! Tokenization is intentionally simple and deterministic (but "name-aware"):
//! - Split on non-alphanumeric characters (including `_` and `.`).
//! - Split camelCase/PascalCase boundaries (PaymentService → payment + service).
//! - Lowercase everything.
//! - Ignore very short tokens and common stopwords.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock, Weak};

use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::{IndexSidecarWriter, PathDB, StrId};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InvertedIndex {
    pub token_to_entities: HashMap<String, RoaringBitmap>,
}

#[derive(Debug)]
pub(crate) struct TextIndexCache {
    generation: AtomicU64,
    // attr_key_id -> (built_generation, inverted index)
    indexes: RwLock<HashMap<StrId, (u64, InvertedIndex)>>,
    building: Mutex<HashSet<StrId>>,
    async_source: Mutex<Option<Weak<PathDB>>>,
    sidecar: Mutex<Option<Arc<IndexSidecarWriter>>>,
}

impl Default for TextIndexCache {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            indexes: RwLock::new(HashMap::new()),
            building: Mutex::new(HashSet::new()),
            async_source: Mutex::new(None),
            sidecar: Mutex::new(None),
        }
    }
}

impl TextIndexCache {
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
    pub(crate) fn attach_async_source(&self, source: Weak<PathDB>) {
        let mut guard = self.async_source.lock().expect("text index source poisoned");
        *guard = Some(source);
    }

    pub(crate) fn attach_sidecar_writer(&self, writer: Arc<IndexSidecarWriter>) {
        let mut guard = self.sidecar.lock().expect("text index sidecar poisoned");
        *guard = Some(writer);
    }

    pub(crate) fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn query_any_tokens(
        &self,
        db: &PathDB,
        attr_key_id: StrId,
        tokens: &[String],
    ) -> RoaringBitmap {
        if tokens.is_empty() {
            return RoaringBitmap::new();
        }
        let gen = self.generation.load(Ordering::SeqCst);
        if self.is_ready(attr_key_id, gen) {
            let guard = self.indexes.read().expect("text index lock poisoned");
            let Some((_, index)) = guard.get(&attr_key_id) else {
                return RoaringBitmap::new();
            };
            return query_any(index, tokens);
        }
        if self.schedule_build_async(db, attr_key_id, gen) {
            return fallback_any(db, attr_key_id, tokens);
        }
        self.ensure_built_sync(db, attr_key_id, gen);
        let guard = self.indexes.read().expect("text index lock poisoned");
        let Some((_, index)) = guard.get(&attr_key_id) else {
            return RoaringBitmap::new();
        };
        query_any(index, tokens)
    }

    pub(crate) fn query_all_tokens(
        &self,
        db: &PathDB,
        attr_key_id: StrId,
        tokens: &[String],
    ) -> RoaringBitmap {
        if tokens.is_empty() {
            return RoaringBitmap::new();
        }
        let gen = self.generation.load(Ordering::SeqCst);
        if self.is_ready(attr_key_id, gen) {
            let guard = self.indexes.read().expect("text index lock poisoned");
            let Some((_, index)) = guard.get(&attr_key_id) else {
                return RoaringBitmap::new();
            };
            return query_all(index, tokens);
        }
        if self.schedule_build_async(db, attr_key_id, gen) {
            return fallback_all(db, attr_key_id, tokens);
        }
        self.ensure_built_sync(db, attr_key_id, gen);
        let guard = self.indexes.read().expect("text index lock poisoned");
        let Some((_, index)) = guard.get(&attr_key_id) else {
            return RoaringBitmap::new();
        };
        query_all(index, tokens)
    }

    pub(crate) fn is_ready(&self, attr_key_id: StrId, gen: u64) -> bool {
        let guard = self.indexes.read().expect("text index lock poisoned");
        guard
            .get(&attr_key_id)
            .is_some_and(|(built, _)| *built == gen)
    }

    pub(crate) fn load_indexes(&self, generation: u64, indexes: HashMap<StrId, InvertedIndex>) {
        let mut guard = self.indexes.write().expect("text index lock poisoned");
        for (k, v) in indexes {
            guard.insert(k, (generation, v));
        }
    }

    pub(crate) fn snapshot(&self, generation: u64) -> HashMap<StrId, InvertedIndex> {
        let guard = self.indexes.read().expect("text index lock poisoned");
        guard
            .iter()
            .filter_map(|(k, (built, idx))| {
                if *built == generation {
                    Some((*k, idx.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    fn schedule_build_async(&self, db: &PathDB, attr_key_id: StrId, gen: u64) -> bool {
        {
            let guard = self.indexes.read().expect("text index lock poisoned");
            if guard
                .get(&attr_key_id)
                .is_some_and(|(built, _)| *built == gen)
            {
                return false;
            }
        }

        let source = self
            .async_source
            .lock()
            .expect("text index source poisoned")
            .clone();
        let Some(source) = source else {
            return false;
        };

        {
            let mut building = self.building.lock().expect("text index build poisoned");
            if building.contains(&attr_key_id) {
                return true;
            }
            building.insert(attr_key_id);
        }

        std::thread::Builder::new()
            .name("axiograph_text_index".to_string())
            .spawn(move || {
                let Some(db) = source.upgrade() else {
                    return;
                };
                let new_index = build_inverted_index(&db, attr_key_id);
                let cache = &db.text_index;
                if cache.generation.load(Ordering::SeqCst) == gen {
                    cache.load_indexes(gen, [(attr_key_id, new_index)].into());
                    if let Some(writer) = cache
                        .sidecar
                        .lock()
                        .expect("text index sidecar poisoned")
                        .as_ref()
                    {
                        writer.mark_dirty();
                    }
                }
                let mut building = cache.building.lock().expect("text index build poisoned");
                building.remove(&attr_key_id);
            })
            .expect("failed to spawn text index build thread");

        true
    }

    fn ensure_built_sync(&self, db: &PathDB, attr_key_id: StrId, gen: u64) {
        let new_index = build_inverted_index(db, attr_key_id);
        let mut guard = self.indexes.write().expect("text index lock poisoned");
        guard.insert(attr_key_id, (gen, new_index));
    }
}

fn query_any(index: &InvertedIndex, tokens: &[String]) -> RoaringBitmap {
    let mut out = RoaringBitmap::new();
    for t in tokens {
        if let Some(bm) = index.token_to_entities.get(t) {
            out |= bm;
        }
    }
    out
}

fn query_all(index: &InvertedIndex, tokens: &[String]) -> RoaringBitmap {
    let mut out: Option<RoaringBitmap> = None;
    for t in tokens {
        let Some(bm) = index.token_to_entities.get(t) else {
            return RoaringBitmap::new();
        };
        out = Some(match out {
            None => bm.clone(),
            Some(mut acc) => {
                acc &= bm;
                acc
            }
        });
    }
    out.unwrap_or_default()
}

fn fallback_any(db: &PathDB, attr_key_id: StrId, tokens: &[String]) -> RoaringBitmap {
    let Some(col) = db.entities.attrs.get(&attr_key_id) else {
        return RoaringBitmap::new();
    };
    let mut out = RoaringBitmap::new();
    for (&entity_id, &value_id) in col {
        let Some(value) = db.interner.lookup(value_id) else {
            continue;
        };
        let value_tokens = tokenize_text(&value);
        if tokens.iter().any(|t| value_tokens.contains(t)) {
            out.insert(entity_id);
        }
    }
    out
}

fn fallback_all(db: &PathDB, attr_key_id: StrId, tokens: &[String]) -> RoaringBitmap {
    let Some(col) = db.entities.attrs.get(&attr_key_id) else {
        return RoaringBitmap::new();
    };
    let mut out = RoaringBitmap::new();
    for (&entity_id, &value_id) in col {
        let Some(value) = db.interner.lookup(value_id) else {
            continue;
        };
        let value_tokens = tokenize_text(&value);
        if tokens.iter().all(|t| value_tokens.contains(t)) {
            out.insert(entity_id);
        }
    }
    out
}

fn build_inverted_index(db: &PathDB, attr_key_id: StrId) -> InvertedIndex {
    let mut out = InvertedIndex::default();

    let Some(col) = db.entities.attrs.get(&attr_key_id) else {
        return out;
    };

    for (&entity_id, &value_id) in col {
        let Some(value) = db.interner.lookup(value_id) else {
            continue;
        };
        for token in tokenize_text(&value) {
            out.token_to_entities
                .entry(token)
                .or_insert_with(RoaringBitmap::new)
                .insert(entity_id);
        }
    }

    out
}

pub(crate) fn tokenize_query(query: &str) -> Vec<String> {
    tokenize_text(query)
}

fn tokenize_text(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev_was_lower = false;

    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            // Split on camelCase boundaries: "paymentService" -> "payment" "service".
            if c.is_ascii_uppercase() && prev_was_lower && !current.is_empty() {
                push_token_if_interesting(&mut tokens, &mut current);
            }

            let lc = c.to_ascii_lowercase();
            if current.len() < 64 {
                current.push(lc);
            }
            prev_was_lower = lc.is_ascii_lowercase();
            continue;
        }

        if !current.is_empty() {
            push_token_if_interesting(&mut tokens, &mut current);
        }
        prev_was_lower = false;
    }

    if !current.is_empty() {
        push_token_if_interesting(&mut tokens, &mut current);
    }

    tokens
}

fn push_token_if_interesting(tokens: &mut Vec<String>, current: &mut String) {
    // Ignore very short tokens (keeps the index smaller and avoids matching lots of noise),
    // but allow "id"/"ga" style tokens (use stopwords to keep common English noise down).
    const MIN_TOKEN_LEN: usize = 2;
    const STOPWORDS: &[&str] = &[
        "a", "an", "and", "as", "at", "by", "for", "in", "is", "of", "on", "or", "the", "to",
        "with",
    ];

    if current.len() >= MIN_TOKEN_LEN && !STOPWORDS.contains(&current.as_str()) {
        tokens.push(std::mem::take(current));
    } else {
        current.clear();
    }
}
