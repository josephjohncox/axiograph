//! Durable index sidecars (fact/text caches + path LRU).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Weak};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use ahash::AHashMap;

use crate::fact_index::FactIndex;
use crate::text_index::InvertedIndex;
use crate::{PathDB, PathSig, StrId};

pub const PATHDB_INDEX_SIDECAR_VERSION_V1: &str = "pathdb_index_sidecar_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LruSnapshot {
    pub capacity: usize,
    pub order: Vec<PathSig>,
    pub entries: std::collections::HashMap<PathSig, AHashMap<u32, roaring::RoaringBitmap>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathDbIndexSidecarV1 {
    pub version: String,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub fact_index: Option<FactIndex>,
    #[serde(default)]
    pub text_indexes: std::collections::HashMap<StrId, InvertedIndex>,
    #[serde(default)]
    pub path_lru: Option<LruSnapshot>,
}

impl PathDbIndexSidecarV1 {
    pub fn new(snapshot_id: Option<String>) -> Self {
        Self {
            version: PATHDB_INDEX_SIDECAR_VERSION_V1.to_string(),
            snapshot_id,
            fact_index: None,
            text_indexes: std::collections::HashMap::new(),
            path_lru: None,
        }
    }
}

const INDEX_SIDECAR_DEBOUNCE: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct IndexSidecarWriter {
    tx: mpsc::Sender<IndexSidecarCommand>,
}

enum IndexSidecarCommand {
    MarkDirty,
    Shutdown,
}

impl IndexSidecarWriter {
    pub fn new(path: PathBuf, db: Weak<PathDB>, snapshot_id: Option<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("axiograph_index_sidecar".to_string())
            .spawn(move || {
                let mut dirty = false;
                loop {
                    match rx.recv_timeout(INDEX_SIDECAR_DEBOUNCE) {
                        Ok(IndexSidecarCommand::MarkDirty) => {
                            dirty = true;
                        }
                        Ok(IndexSidecarCommand::Shutdown) => {
                            if dirty {
                                let _ = write_sidecar(&path, &db, snapshot_id.as_ref());
                            }
                            break;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if dirty {
                                if write_sidecar(&path, &db, snapshot_id.as_ref()).is_ok() {
                                    dirty = false;
                                }
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("failed to spawn index sidecar writer");
        Self { tx }
    }

    pub fn mark_dirty(&self) {
        let _ = self.tx.send(IndexSidecarCommand::MarkDirty);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(IndexSidecarCommand::Shutdown);
    }
}

fn write_sidecar(path: &Path, db: &Weak<PathDB>, snapshot_id: Option<&String>) -> Result<()> {
    let Some(db) = db.upgrade() else {
        return Ok(());
    };
    let sidecar = db.snapshot_index_sidecar(snapshot_id.cloned());
    write_sidecar_file(path, &sidecar)
}

pub fn write_sidecar_file(path: &Path, sidecar: &PathDbIndexSidecarV1) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("cbor.tmp");
    let mut f = fs::File::create(&tmp)?;
    ciborium::ser::into_writer(sidecar, &mut f)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub fn read_sidecar_file(path: &Path) -> Result<PathDbIndexSidecarV1> {
    let f = fs::File::open(path)?;
    let sidecar: PathDbIndexSidecarV1 = ciborium::de::from_reader(f)?;
    Ok(sidecar)
}
