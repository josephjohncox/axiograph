//! Durable index sidecars (fact/text caches + path LRU).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Weak};
use std::time::Duration;

use anyhow::{ensure, Result};
use serde::{Deserialize, Deserializer, Serialize};

use ahash::AHashMap;

use crate::fact_index::FactIndex;
use crate::text_index::InvertedIndex;
use crate::{PathDB, PathSig, PathdbSnapshotId, StrId};

pub const PATHDB_INDEX_SIDECAR_VERSION_V1: &str = "pathdb_index_sidecar_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LruSnapshot {
    pub capacity: usize,
    pub order: Vec<PathSig>,
    pub entries: std::collections::HashMap<PathSig, AHashMap<u32, roaring::RoaringBitmap>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathDbIndexSidecarV1 {
    pub version: String,
    #[serde(default)]
    pub snapshot_id: Option<PathdbSnapshotId>,
    #[serde(default)]
    pub fact_index: Option<FactIndex>,
    #[serde(default)]
    pub text_indexes: std::collections::HashMap<StrId, InvertedIndex>,
    #[serde(default)]
    pub path_lru: Option<LruSnapshot>,
}

fn unsupported_sidecar_version_message(version: &str) -> String {
    format!(
        "unsupported PathDB index sidecar version `{version}`; expected `{PATHDB_INDEX_SIDECAR_VERSION_V1}`"
    )
}

impl PathDbIndexSidecarV1 {
    pub fn new(snapshot_id: Option<PathdbSnapshotId>) -> Self {
        Self {
            version: PATHDB_INDEX_SIDECAR_VERSION_V1.to_string(),
            snapshot_id,
            fact_index: None,
            text_indexes: std::collections::HashMap::new(),
            path_lru: None,
        }
    }

    pub fn validate_version(&self) -> Result<()> {
        ensure!(
            self.version == PATHDB_INDEX_SIDECAR_VERSION_V1,
            unsupported_sidecar_version_message(&self.version)
        );
        Ok(())
    }
}

impl<'de> Deserialize<'de> for PathDbIndexSidecarV1 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct PathDbIndexSidecarPayloadV1 {
            version: String,
            #[serde(default)]
            snapshot_id: Option<PathdbSnapshotId>,
            #[serde(default)]
            fact_index: Option<FactIndex>,
            #[serde(default)]
            text_indexes: std::collections::HashMap<StrId, InvertedIndex>,
            #[serde(default)]
            path_lru: Option<LruSnapshot>,
        }

        let payload = PathDbIndexSidecarPayloadV1::deserialize(deserializer)?;
        if payload.version != PATHDB_INDEX_SIDECAR_VERSION_V1 {
            return Err(serde::de::Error::custom(
                unsupported_sidecar_version_message(&payload.version),
            ));
        }

        Ok(Self {
            version: payload.version,
            snapshot_id: payload.snapshot_id,
            fact_index: payload.fact_index,
            text_indexes: payload.text_indexes,
            path_lru: payload.path_lru,
        })
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
    pub fn new(path: PathBuf, db: Weak<PathDB>, snapshot_id: Option<PathdbSnapshotId>) -> Self {
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

fn write_sidecar(
    path: &Path,
    db: &Weak<PathDB>,
    snapshot_id: Option<&PathdbSnapshotId>,
) -> Result<()> {
    let Some(db) = db.upgrade() else {
        return Ok(());
    };
    let sidecar = db.snapshot_index_sidecar(snapshot_id.cloned());
    write_sidecar_file(path, &sidecar)
}

pub fn write_sidecar_file(path: &Path, sidecar: &PathDbIndexSidecarV1) -> Result<()> {
    sidecar.validate_version()?;
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
    sidecar.validate_version()?;
    Ok(sidecar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sidecar_file_round_trips_current_typed_snapshot_id() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("sidecar.cbor");
        let sidecar = PathDbIndexSidecarV1::new(Some(PathdbSnapshotId::new("pathdb:snap-42")));

        write_sidecar_file(&path, &sidecar).expect("sidecar should write");
        let round_trip = read_sidecar_file(&path).expect("sidecar should read");

        assert_eq!(
            round_trip.snapshot_id,
            Some(PathdbSnapshotId::new("pathdb:snap-42"))
        );
    }

    #[test]
    fn sidecar_reader_rejects_wrong_version_payload() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("stale-sidecar.cbor");
        let stale = PathDbIndexSidecarV1 {
            version: "pathdb_index_sidecar_v0".to_string(),
            snapshot_id: Some(PathdbSnapshotId::new("pathdb:stale")),
            fact_index: None,
            text_indexes: std::collections::HashMap::new(),
            path_lru: None,
        };

        let mut file = fs::File::create(&path).expect("stale sidecar file");
        ciborium::ser::into_writer(&stale, &mut file).expect("stale sidecar should serialize");

        let err = read_sidecar_file(&path).expect_err("stale sidecar version should be rejected");
        assert!(
            err.to_string()
                .contains("unsupported PathDB index sidecar version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn sidecar_writer_rejects_wrong_version_payload() {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("stale-sidecar.cbor");
        let stale = PathDbIndexSidecarV1 {
            version: "pathdb_index_sidecar_v0".to_string(),
            snapshot_id: Some(PathdbSnapshotId::new("pathdb:stale")),
            fact_index: None,
            text_indexes: std::collections::HashMap::new(),
            path_lru: None,
        };

        let err =
            write_sidecar_file(&path, &stale).expect_err("stale sidecar version should not write");
        assert!(
            err.to_string()
                .contains("unsupported PathDB index sidecar version"),
            "unexpected error: {err}"
        );
        assert!(
            !path.exists(),
            "writer should not create stale sidecar file"
        );
    }
}
