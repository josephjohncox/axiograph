//! Snapshot-scoped embedding artifacts (extension layer).
//!
//! Goals
//! -----
//! - Keep embeddings **outside the trusted kernel** (Lean certifies semantics, not ANN retrieval).
//! - Store embeddings **snapshot-scoped** in the PathDB WAL so:
//!   - they are reproducible for a given snapshot,
//!   - they can be synced between master/replica stores,
//!   - they can be used by the db server / viz UI for RAG-style grounding.
//!
//! Two modes (recommended to run together):
//! - Deterministic token-hash embeddings (always-on; can be indexed with ANN).
//! - Model embeddings (Ollama `/api/embed` or `/api/embeddings`) stored in the WAL.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_pathdb::DbToken;

pub const EMBEDDINGS_FILE_VERSION_V1: &str = "axiograph_embeddings_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingTargetKindV1 {
    DocChunks,
    Entities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmbeddingKeyV1 {
    DocChunk { chunk_id: String },
    Entity { entity_type: String, name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingItemV1 {
    pub key: EmbeddingKeyV1,
    pub vector: Vec<f32>,
    #[serde(default)]
    pub text_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsFileV1 {
    pub version: String,
    pub created_at_unix_secs: u64,
    pub backend: String,
    pub model: String,
    pub dim: usize,
    pub target: EmbeddingTargetKindV1,
    pub items: Vec<EmbeddingItemV1>,
    /// Optional free-form metadata (e.g. prompt template, truncation settings).
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

pub fn encode_embeddings_file_v1(file: &EmbeddingsFileV1) -> Result<Vec<u8>> {
    if file.version != EMBEDDINGS_FILE_VERSION_V1 {
        return Err(anyhow!(
            "unsupported embeddings file version: {} (expected {EMBEDDINGS_FILE_VERSION_V1})",
            file.version
        ));
    }
    let mut out = Vec::new();
    ciborium::ser::into_writer(file, &mut out)
        .map_err(|e| anyhow!("failed to CBOR-encode embeddings file: {e}"))?;
    Ok(out)
}

pub fn decode_embeddings_file_v1(bytes: &[u8]) -> Result<EmbeddingsFileV1> {
    let file: EmbeddingsFileV1 = ciborium::de::from_reader(bytes)
        .map_err(|e| anyhow!("failed to CBOR-decode embeddings file: {e}"))?;
    if file.version != EMBEDDINGS_FILE_VERSION_V1 {
        return Err(anyhow!(
            "unsupported embeddings file version: {} (expected {EMBEDDINGS_FILE_VERSION_V1})",
            file.version
        ));
    }
    Ok(file)
}

// ============================================================================
// Runtime: resolved embedding rows keyed by snapshot-local entity ids
// ============================================================================

#[derive(Debug, Clone)]
pub struct ResolvedEmbeddingRowV1 {
    pub id: u32,
    pub vector: Vec<f32>,
    #[allow(dead_code)]
    pub text_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedEmbeddingsTargetV1 {
    pub backend: String,
    pub model: String,
    pub dim: usize,
    pub rows: Vec<ResolvedEmbeddingRowV1>,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedEmbeddingsIndexV1 {
    db_token: Option<DbToken>,
    pub docchunks: Option<ResolvedEmbeddingsTargetV1>,
    pub entities: Option<ResolvedEmbeddingsTargetV1>,
}

fn normalize_in_place(v: &mut [f32]) {
    let mut norm2 = 0.0f32;
    for x in v.iter() {
        norm2 += x * x;
    }
    if norm2 <= 0.0 {
        return;
    }
    let inv = 1.0f32 / norm2.sqrt();
    for x in v.iter_mut() {
        *x *= inv;
    }
}

impl ResolvedEmbeddingsIndexV1 {
    pub fn assert_in_db(&self, db: &axiograph_pathdb::PathDB) -> Result<()> {
        let Some(expected) = self.db_token else {
            return Ok(());
        };
        let actual = db.db_token();
        if expected != actual {
            return Err(anyhow!(
                "embeddings index is for db#{} but was used with db#{} (stale snapshot?)",
                expected.raw(),
                actual.raw()
            ));
        }
        Ok(())
    }

    /// Resolve one embeddings file against the currently loaded snapshot DB.
    ///
    /// This converts stable keys (chunk_id, (type,name)) into snapshot-local entity ids,
    /// dropping any entries that can't be resolved.
    pub fn resolve_and_set(
        &mut self,
        db: &axiograph_pathdb::PathDB,
        file: EmbeddingsFileV1,
    ) -> Result<()> {
        let token = db.db_token();
        if let Some(existing) = self.db_token {
            if existing != token {
                return Err(anyhow!(
                    "cannot resolve embeddings into existing index: db token mismatch (expected db#{}, got db#{})",
                    existing.raw(),
                    token.raw()
                ));
            }
        } else {
            self.db_token = Some(token);
        }

        let dim = file.dim;
        if dim == 0 {
            return Err(anyhow!(
                "embeddings file has dim=0 (backend={}, model={})",
                file.backend,
                file.model
            ));
        }

        // Precompute lookup maps once per file.
        let mut chunk_id_to_entity_id: HashMap<String, u32> = HashMap::new();
        if file.target == EmbeddingTargetKindV1::DocChunks {
            if let Some(chunks) = db.find_by_type("DocChunk") {
                for id in chunks.iter() {
                    if let Some(view) = db.get_entity(id) {
                        if let Some(cid) = view.attrs.get("chunk_id") {
                            chunk_id_to_entity_id.insert(cid.to_string(), id);
                        }
                    }
                }
            }
        }

        let mut entity_key_to_id: HashMap<(String, String), u32> = HashMap::new();
        if file.target == EmbeddingTargetKindV1::Entities {
            for id in 0..(db.entities.len() as u32) {
                let Some(view) = db.get_entity(id) else {
                    continue;
                };
                let Some(name) = view.attrs.get("name") else {
                    continue;
                };
                entity_key_to_id.insert((view.entity_type.to_string(), name.to_string()), id);
            }
        }

        let mut rows: Vec<ResolvedEmbeddingRowV1> = Vec::new();
        for item in file.items {
            if item.vector.len() != dim {
                return Err(anyhow!(
                    "embeddings file item has wrong dim: expected {} got {}",
                    dim,
                    item.vector.len()
                ));
            }

            let id = match item.key {
                EmbeddingKeyV1::DocChunk { chunk_id } => {
                    if file.target != EmbeddingTargetKindV1::DocChunks {
                        continue;
                    }
                    let Some(&id) = chunk_id_to_entity_id.get(&chunk_id) else {
                        continue;
                    };
                    id
                }
                EmbeddingKeyV1::Entity { entity_type, name } => {
                    if file.target != EmbeddingTargetKindV1::Entities {
                        continue;
                    }
                    let Some(&id) = entity_key_to_id.get(&(entity_type, name)) else {
                        continue;
                    };
                    id
                }
            };

            let mut v = item.vector;
            normalize_in_place(&mut v);
            rows.push(ResolvedEmbeddingRowV1 {
                id,
                vector: v,
                text_digest: item.text_digest,
            });
        }

        let target = ResolvedEmbeddingsTargetV1 {
            backend: file.backend,
            model: file.model,
            dim,
            rows,
        };

        match file.target {
            EmbeddingTargetKindV1::DocChunks => self.docchunks = Some(target),
            EmbeddingTargetKindV1::Entities => self.entities = Some(target),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec_approx_eq(a: &[f32], b: &[f32], eps: f32) {
        assert_eq!(a.len(), b.len(), "vector length mismatch");
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            let d = (x - y).abs();
            assert!(
                d <= eps,
                "vector mismatch at idx {i}: left={x} right={y} (|d|={d} eps={eps})"
            );
        }
    }

    #[test]
    fn embeddings_file_v1_roundtrip_cbor() {
        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::DocChunk {
                    chunk_id: "doc_0".to_string(),
                },
                vector: vec![3.0, 4.0],
                text_digest: Some("sha256:deadbeef".to_string()),
            }],
            metadata: HashMap::from([("note".to_string(), "test".to_string())]),
        };

        let bytes = encode_embeddings_file_v1(&file).expect("encode");
        let decoded = decode_embeddings_file_v1(&bytes).expect("decode");

        assert_eq!(decoded.version, EMBEDDINGS_FILE_VERSION_V1);
        assert_eq!(decoded.backend, "ollama");
        assert_eq!(decoded.model, "nomic-embed-text");
        assert_eq!(decoded.dim, 2);
        assert_eq!(decoded.target, EmbeddingTargetKindV1::DocChunks);
        assert_eq!(decoded.items.len(), 1);
        assert_eq!(decoded.metadata.get("note").map(|s| s.as_str()), Some("test"));
    }

    #[test]
    fn resolves_docchunk_embeddings_by_chunk_id() {
        let mut db = axiograph_pathdb::PathDB::new();
        let doc_chunk_id = db.add_entity(
            "DocChunk",
            vec![
                ("name", "doc_0"),
                ("chunk_id", "doc_0"),
                ("text", "hello world"),
            ],
        );
        db.build_indexes();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::DocChunk {
                    chunk_id: "doc_0".to_string(),
                },
                vector: vec![3.0, 4.0],
                text_digest: None,
            }],
            metadata: HashMap::new(),
        };

        let mut idx = ResolvedEmbeddingsIndexV1::default();
        idx.resolve_and_set(&db, file).expect("resolve");
        idx.assert_in_db(&db).expect("token ok");

        let target = idx.docchunks.expect("docchunks target set");
        assert_eq!(target.backend, "ollama");
        assert_eq!(target.model, "nomic-embed-text");
        assert_eq!(target.dim, 2);
        assert_eq!(target.rows.len(), 1);
        assert_eq!(target.rows[0].id, doc_chunk_id);
        assert_vec_approx_eq(&target.rows[0].vector, &[0.6, 0.8], 1e-4);
    }

    #[test]
    fn resolves_entity_embeddings_by_type_and_name() {
        let mut db = axiograph_pathdb::PathDB::new();
        let alice_id = db.add_entity(
            "Person",
            vec![("name", "Alice"), ("description", "likes cats")],
        );
        db.build_indexes();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::Entities,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::Entity {
                    entity_type: "Person".to_string(),
                    name: "Alice".to_string(),
                },
                vector: vec![0.0, 5.0],
                text_digest: None,
            }],
            metadata: HashMap::new(),
        };

        let mut idx = ResolvedEmbeddingsIndexV1::default();
        idx.resolve_and_set(&db, file).expect("resolve");
        idx.assert_in_db(&db).expect("token ok");

        let target = idx.entities.expect("entities target set");
        assert_eq!(target.rows.len(), 1);
        assert_eq!(target.rows[0].id, alice_id);
        assert_vec_approx_eq(&target.rows[0].vector, &[0.0, 1.0], 1e-4);
    }

    #[test]
    fn embeddings_index_is_snapshot_scoped() {
        let mut db1 = axiograph_pathdb::PathDB::new();
        db1.add_entity("DocChunk", vec![("name", "doc_0"), ("chunk_id", "doc_0")]);
        db1.build_indexes();

        let file = EmbeddingsFileV1 {
            version: EMBEDDINGS_FILE_VERSION_V1.to_string(),
            created_at_unix_secs: 1,
            backend: "ollama".to_string(),
            model: "nomic-embed-text".to_string(),
            dim: 2,
            target: EmbeddingTargetKindV1::DocChunks,
            items: vec![EmbeddingItemV1 {
                key: EmbeddingKeyV1::DocChunk {
                    chunk_id: "doc_0".to_string(),
                },
                vector: vec![1.0, 0.0],
                text_digest: None,
            }],
            metadata: HashMap::new(),
        };

        let mut idx = ResolvedEmbeddingsIndexV1::default();
        idx.resolve_and_set(&db1, file).expect("resolve");
        idx.assert_in_db(&db1).expect("token ok");

        let db2 = axiograph_pathdb::PathDB::new();
        let err = idx.assert_in_db(&db2).unwrap_err();
        assert!(
            err.to_string().contains("stale snapshot"),
            "unexpected error: {err}"
        );
    }
}
