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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use roaring::RoaringBitmap;

use crate::{PathDB, StrId};

#[derive(Debug, Default, Clone)]
pub(crate) struct InvertedIndex {
    pub token_to_entities: HashMap<String, RoaringBitmap>,
}

#[derive(Debug, Default)]
pub(crate) struct TextIndexCache {
    generation: AtomicU64,
    // attr_key_id -> (built_generation, inverted index)
    indexes: RwLock<HashMap<StrId, (u64, InvertedIndex)>>,
}

impl TextIndexCache {
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
        self.ensure_built(db, attr_key_id);
        let guard = self.indexes.read().expect("text index lock poisoned");
        let Some((_, index)) = guard.get(&attr_key_id) else {
            return RoaringBitmap::new();
        };

        let mut out = RoaringBitmap::new();
        for t in tokens {
            if let Some(bm) = index.token_to_entities.get(t) {
                out |= bm;
            }
        }
        out
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
        self.ensure_built(db, attr_key_id);
        let guard = self.indexes.read().expect("text index lock poisoned");
        let Some((_, index)) = guard.get(&attr_key_id) else {
            return RoaringBitmap::new();
        };

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

    fn ensure_built(&self, db: &PathDB, attr_key_id: StrId) {
        let gen = self.generation.load(Ordering::SeqCst);

        // Fast path: already built for this generation.
        {
            let guard = self.indexes.read().expect("text index lock poisoned");
            if guard
                .get(&attr_key_id)
                .is_some_and(|(built, _)| *built == gen)
            {
                return;
            }
        }

        // Build outside the write lock (keeps lock-hold time small).
        let new_index = build_inverted_index(db, attr_key_id);

        let mut guard = self.indexes.write().expect("text index lock poisoned");
        guard.insert(attr_key_id, (gen, new_index));
    }
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
