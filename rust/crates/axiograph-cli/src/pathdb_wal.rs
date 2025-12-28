//! PathDB WAL snapshots (append-only) under an accepted-plane directory.
//!
//! Motivation
//! ----------
//! The accepted `.axi` plane is canonical and already versioned via:
//! - append-only module storage (`modules/<name>/<digest>.axi`)
//! - content-derived accepted snapshot ids (`snapshots/<id>.json` + `HEAD`)
//!
//! PathDB (`.axpd`) is *derived* and can be rebuilt from accepted snapshots.
//! However, real workflows also want:
//! - incremental overlays (doc chunks, heuristic edges, entity-resolution links),
//! - continuous ingest without rebuilding everything from scratch, and
//! - snapshot ids for the *full* query substrate used in the REPL.
//!
//! This module adds a pragmatic WAL-like layer:
//! - an append-only JSONL log of PathDB "commits",
//! - a content-derived PathDB snapshot id (stable),
//! - immutable input blobs (e.g. `chunks.json`) stored by digest (with optional
//!   derived CBOR sidecars for fast replay), and
//! - optional `.axpd` checkpoints stored per snapshot id for fast checkout.
//!
//! Design constraints
//! ------------------
//! - `.axi` remains canonical: PathDB snapshots always declare the accepted-plane
//!   snapshot id they are derived from.
//! - Extension-layer ops (chunks, fuzzy links) are explicitly non-certified.
//! - The format is intentionally small and readable; we can extend `PathDbWalOpV1`
//!   as new mutation types become stable.

use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::accepted_plane::AcceptedPlaneSnapshotV1;

const PATHDB_WAL_DIR: &str = "pathdb";
const PATHDB_WAL_LOG_V1: &str = "pathdb_wal.log.jsonl";
const PATHDB_WAL_HEAD_FILE: &str = "HEAD";
const PATHDB_WAL_SNAPSHOTS_DIR: &str = "snapshots";
const PATHDB_WAL_BLOBS_DIR: &str = "blobs";
const PATHDB_WAL_CHECKPOINTS_DIR: &str = "checkpoints";

const PATHDB_SNAPSHOT_VERSION_V1: &str = "pathdb_snapshot_v1";
const PATHDB_EVENT_VERSION_V1: &str = "pathdb_event_v1";
const PATHDB_WAL_VERSION_V1: &str = "pathdb_wal_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathDbSnapshotV1 {
    pub version: String,
    pub snapshot_id: String,
    pub previous_snapshot_id: Option<String>,
    /// Accepted-plane snapshot id this PathDB snapshot is derived from.
    pub accepted_snapshot_id: String,
    pub created_at_unix_secs: u64,
    /// Cumulative list of ops applied to the base accepted snapshot.
    pub ops: Vec<PathDbWalOpV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "tag", rename_all = "snake_case")]
pub enum PathDbWalOpV1 {
    /// Import a `chunks.json` blob (array of `Chunk`) into the snapshot as `DocChunk` + `Document` nodes.
    ///
    /// For fast replay, the commit step may also store a derived `chunks.cbor`
    /// sidecar. Replays will prefer CBOR when present.
    ///
    /// This is an extension-layer operation intended for discovery workflows.
    ImportChunksV1 {
        chunks_digest: String,
        stored_path: String,
    },
    /// Import snapshot-scoped embeddings (CBOR) into the WAL.
    ///
    /// Note: embeddings are not currently stored inside `.axpd`; they are a
    /// sidecar blob referenced by the snapshot manifest.
    ImportEmbeddingsV1 {
        embeddings_digest: String,
        stored_path: String,
    },
    /// Import a `proposals.json` blob (Evidence/Proposals schema) into the snapshot.
    ///
    /// For fast replay, the commit step may also store a derived `proposals.cbor`
    /// sidecar. Replays will prefer CBOR when present.
    ///
    /// This is an extension-layer operation intended for cross-domain data
    /// preservation and ontology engineering workflows. Canonical `.axi` is
    /// still the trusted meaning plane; proposals are the evidence plane.
    ImportProposalsV1 {
        proposals_digest: String,
        stored_path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathDbWalEventV1 {
    pub version: String,
    pub created_at_unix_secs: u64,
    pub action: String,
    pub snapshot_id: String,
    pub previous_snapshot_id: Option<String>,
    pub accepted_snapshot_id: String,
    pub ops_appended: Vec<PathDbWalOpV1>,
    #[serde(default)]
    pub message: Option<String>,
}

pub struct PathDbCommitResult {
    pub snapshot_id: String,
    pub accepted_snapshot_id: String,
    pub ops_added: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PathdbCommitOptions {
    pub timings: bool,
    pub timings_json: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct PathdbBuildOptions {
    pub timings: bool,
    pub timings_json: Option<PathBuf>,
    /// Ignore checkpoints and force a rebuild from accepted + ops.
    pub rebuild: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PhaseTimingV1 {
    name: String,
    millis: u128,
}

#[derive(Debug, Clone, Serialize)]
struct PathdbOperationTimingsV1 {
    version: String,
    operation: String,
    snapshot_id: Option<String>,
    accepted_snapshot_id: Option<String>,
    used_checkpoint: Option<bool>,
    phases: Vec<PhaseTimingV1>,
    notes: Vec<String>,
}

fn write_timings_json(path: &Path, timings: &PathdbOperationTimingsV1) -> Result<()> {
    let json = serde_json::to_string_pretty(timings)?;
    fs::write(path, json)?;
    Ok(())
}

fn print_timings(t: &PathdbOperationTimingsV1) {
    eprintln!("-- timings ({})", t.operation);
    for p in &t.phases {
        eprintln!("  {:28} {:>8} ms", p.name, p.millis);
    }
    for n in &t.notes {
        eprintln!("  note: {n}");
    }
}

/// Initialize the PathDB WAL directory layout under an accepted-plane directory.
///
/// This is idempotent and safe to run even if the directory already exists.
pub(crate) fn init_pathdb_wal_dir(accepted_dir: &Path) -> Result<()> {
    ensure_layout(accepted_dir)
}

pub(crate) fn read_pathdb_snapshot_for_cli(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<PathDbSnapshotV1> {
    ensure_layout(accepted_dir)?;
    let id = resolve_pathdb_snapshot_id(accepted_dir, snapshot_id_or_latest)?;
    read_pathdb_snapshot(accepted_dir, &id)
}

#[allow(dead_code)]
pub fn commit_pathdb_snapshot(
    accepted_dir: &Path,
    accepted_snapshot_id_or_latest: &str,
    chunks: &[PathBuf],
    message: Option<&str>,
) -> Result<PathDbCommitResult> {
    commit_pathdb_snapshot_with_overlays(
        accepted_dir,
        accepted_snapshot_id_or_latest,
        chunks,
        &[],
        message,
    )
}

pub fn commit_pathdb_snapshot_with_overlays(
    accepted_dir: &Path,
    accepted_snapshot_id_or_latest: &str,
    chunks: &[PathBuf],
    proposals: &[PathBuf],
    message: Option<&str>,
) -> Result<PathDbCommitResult> {
    commit_pathdb_snapshot_with_overlays_with_options(
        accepted_dir,
        accepted_snapshot_id_or_latest,
        chunks,
        proposals,
        message,
        PathdbCommitOptions::default(),
    )
}

pub fn commit_pathdb_snapshot_with_overlays_with_options(
    accepted_dir: &Path,
    accepted_snapshot_id_or_latest: &str,
    chunks: &[PathBuf],
    proposals: &[PathBuf],
    message: Option<&str>,
    options: PathdbCommitOptions,
) -> Result<PathDbCommitResult> {
    ensure_layout(accepted_dir)?;

    let mut timings = if options.timings || options.timings_json.is_some() {
        Some(PathdbOperationTimingsV1 {
            version: "pathdb_timings_v1".to_string(),
            operation: "pathdb_commit".to_string(),
            snapshot_id: None,
            accepted_snapshot_id: None,
            used_checkpoint: None,
            phases: Vec::new(),
            notes: Vec::new(),
        })
    } else {
        None
    };

    let phase_start = Instant::now();
    let accepted_snapshot_id =
        resolve_accepted_snapshot_id(accepted_dir, accepted_snapshot_id_or_latest)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "resolve_accepted_snapshot".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
        t.accepted_snapshot_id = Some(accepted_snapshot_id.clone());
    }

    let phase_start = Instant::now();
    let previous_snapshot_id = read_pathdb_head(accepted_dir)?;
    let previous_snapshot = match previous_snapshot_id.as_deref() {
        Some(prev) => Some(read_pathdb_snapshot(accepted_dir, prev)?),
        None => None,
    };
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "load_previous_snapshot".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
        if let Some(prev_id) = previous_snapshot_id.as_deref() {
            t.notes.push(format!("previous_snapshot_id={prev_id}"));
        } else {
            t.notes.push("previous_snapshot_id=(none)".to_string());
        }
    }

    let existing_ops: Vec<PathDbWalOpV1> = previous_snapshot
        .as_ref()
        .map(|s| s.ops.clone())
        .unwrap_or_default();

    // Fast path: if we have a previous checkpoint with the same accepted base,
    // load it and only apply *new* ops. Otherwise, rebuild from accepted and
    // replay existing ops first.
    let phase_start = Instant::now();
    let mut base_used_checkpoint = false;
    let mut db = if let Some(prev) = previous_snapshot.as_ref() {
        if prev.accepted_snapshot_id == accepted_snapshot_id {
            if let Some(db) = try_load_checkpoint(accepted_dir, &prev.snapshot_id)? {
                base_used_checkpoint = true;
                db
            } else {
                rebuild_from_accepted_and_ops(accepted_dir, &accepted_snapshot_id, &existing_ops)?
            }
        } else {
            rebuild_from_accepted_and_ops(accepted_dir, &accepted_snapshot_id, &existing_ops)?
        }
    } else {
        rebuild_from_accepted_and_ops(accepted_dir, &accepted_snapshot_id, &[])?
        // base only
    };
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "load_or_rebuild_base".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
        t.used_checkpoint = Some(base_used_checkpoint);
        if base_used_checkpoint {
            t.notes.push("base_load=checkpoint".to_string());
        } else {
            t.notes.push("base_load=rebuild".to_string());
        }
    }

    let mut new_ops: Vec<PathDbWalOpV1> = Vec::new();
    for p in chunks {
        let phase_start = Instant::now();
        let op = store_chunks_blob(accepted_dir, p)?;
        if let Some(t) = timings.as_mut() {
            t.phases.push(PhaseTimingV1 {
                name: "store_chunks_blob".to_string(),
                millis: phase_start.elapsed().as_millis(),
            });
        }

        let phase_start = Instant::now();
        apply_op(&mut db, accepted_dir, &op)?;
        if let Some(t) = timings.as_mut() {
            t.phases.push(PhaseTimingV1 {
                name: "apply_chunks_op".to_string(),
                millis: phase_start.elapsed().as_millis(),
            });
        }
        new_ops.push(op);
    }
    for p in proposals {
        let phase_start = Instant::now();
        let op = store_proposals_blob(accepted_dir, p)?;
        if let Some(t) = timings.as_mut() {
            t.phases.push(PhaseTimingV1 {
                name: "store_proposals_blob".to_string(),
                millis: phase_start.elapsed().as_millis(),
            });
        }

        let phase_start = Instant::now();
        apply_op(&mut db, accepted_dir, &op)?;
        if let Some(t) = timings.as_mut() {
            t.phases.push(PhaseTimingV1 {
                name: "apply_proposals_op".to_string(),
                millis: phase_start.elapsed().as_millis(),
            });
        }
        new_ops.push(op);
    }

    let mut ops_total = existing_ops;
    ops_total.extend(new_ops.clone());

    // Build/update indexes after all ops.
    let phase_start = Instant::now();
    db.build_indexes();
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "build_indexes".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
    }

    let snapshot_id = pathdb_snapshot_id_v1(
        previous_snapshot_id.as_deref(),
        &accepted_snapshot_id,
        &ops_total,
    );
    let snapshot = PathDbSnapshotV1 {
        version: PATHDB_SNAPSHOT_VERSION_V1.to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id: previous_snapshot_id.clone(),
        accepted_snapshot_id: accepted_snapshot_id.clone(),
        created_at_unix_secs: now_unix_secs(),
        ops: ops_total,
    };

    write_pathdb_snapshot(accepted_dir, &snapshot)?;
    if let Some(t) = timings.as_mut() {
        t.snapshot_id = Some(snapshot_id.clone());
    }

    let phase_start = Instant::now();
    write_checkpoint_if_missing(accepted_dir, &snapshot_id, &db.to_bytes()?)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "write_checkpoint".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
    }

    let phase_start = Instant::now();
    write_pathdb_head(accepted_dir, &snapshot_id)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "write_head".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
    }

    let ops_added = new_ops.len();

    let event = PathDbWalEventV1 {
        version: PATHDB_EVENT_VERSION_V1.to_string(),
        created_at_unix_secs: now_unix_secs(),
        action: "commit".to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id,
        accepted_snapshot_id: accepted_snapshot_id.clone(),
        ops_appended: new_ops,
        message: message.map(|s| s.to_string()),
    };
    append_event(accepted_dir, &event)?;

    if let Some(mut t) = timings {
        t.notes.push(format!("ops_added={ops_added}"));
        t.notes.push(format!("ops_total={}", snapshot.ops.len()));
        if options.timings {
            print_timings(&t);
        }
        if let Some(path) = options.timings_json.as_ref() {
            write_timings_json(path, &t)?;
        }
    }

    Ok(PathDbCommitResult {
        snapshot_id,
        accepted_snapshot_id,
        ops_added,
    })
}

/// Commit snapshot-scoped embedding blobs (CBOR) on top of an existing PathDB WAL snapshot.
///
/// This is intended for a "full embed" mode where:
/// - embeddings are computed by a local model (e.g. Ollama `/api/embeddings`),
/// - stored in `pathdb/blobs/` by digest,
/// - and referenced from the PathDB snapshot manifest.
///
/// Note: this does not currently mutate the `.axpd` checkpoint contents; it
/// writes a new checkpoint file only to keep checkout fast/consistent.
pub(crate) fn commit_pathdb_snapshot_with_embedding_bytes(
    accepted_dir: &Path,
    base_snapshot_id_or_latest: &str,
    embedding_blobs: &[Vec<u8>],
    message: Option<&str>,
) -> Result<PathDbCommitResult> {
    ensure_layout(accepted_dir)?;

    if embedding_blobs.is_empty() {
        return Err(anyhow!("commit embeddings requires at least one embedding blob"));
    }

    let base_snapshot_id = resolve_pathdb_snapshot_id(accepted_dir, base_snapshot_id_or_latest)?;
    let base_snapshot = read_pathdb_snapshot(accepted_dir, &base_snapshot_id)?;

    // Load checkpoint or rebuild so we can write a checkpoint for the new snapshot id.
    let mut db = if let Some(db) = try_load_checkpoint(accepted_dir, &base_snapshot.snapshot_id)? {
        db
    } else {
        rebuild_from_accepted_and_ops(
            accepted_dir,
            &base_snapshot.accepted_snapshot_id,
            &base_snapshot.ops,
        )?
    };

    let existing_ops: Vec<PathDbWalOpV1> = base_snapshot.ops.clone();

    let mut new_ops: Vec<PathDbWalOpV1> = Vec::new();
    for bytes in embedding_blobs {
        let op = store_embeddings_blob_bytes(accepted_dir, bytes)?;
        // Apply for digest validation (currently a no-op).
        apply_op(&mut db, accepted_dir, &op)?;
        new_ops.push(op);
    }

    let mut ops_total = existing_ops;
    ops_total.extend(new_ops.clone());

    db.build_indexes();

    let snapshot_id = pathdb_snapshot_id_v1(
        Some(&base_snapshot_id),
        &base_snapshot.accepted_snapshot_id,
        &ops_total,
    );
    let snapshot = PathDbSnapshotV1 {
        version: PATHDB_SNAPSHOT_VERSION_V1.to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id: Some(base_snapshot_id.clone()),
        accepted_snapshot_id: base_snapshot.accepted_snapshot_id.clone(),
        created_at_unix_secs: now_unix_secs(),
        ops: ops_total,
    };

    write_pathdb_snapshot(accepted_dir, &snapshot)?;
    write_checkpoint_if_missing(accepted_dir, &snapshot_id, &db.to_bytes()?)?;
    write_pathdb_head(accepted_dir, &snapshot_id)?;

    let event = PathDbWalEventV1 {
        version: PATHDB_EVENT_VERSION_V1.to_string(),
        created_at_unix_secs: now_unix_secs(),
        action: "commit_embeddings".to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id: Some(base_snapshot_id),
        accepted_snapshot_id: snapshot.accepted_snapshot_id.clone(),
        ops_appended: new_ops.clone(),
        message: message.map(|s| s.to_string()),
    };
    append_event(accepted_dir, &event)?;

    Ok(PathDbCommitResult {
        snapshot_id,
        accepted_snapshot_id: snapshot.accepted_snapshot_id,
        ops_added: new_ops.len(),
    })
}

pub fn build_pathdb_from_pathdb_snapshot(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
    out_axpd: &Path,
) -> Result<()> {
    build_pathdb_from_pathdb_snapshot_with_options(
        accepted_dir,
        snapshot_id_or_latest,
        out_axpd,
        PathdbBuildOptions::default(),
    )
}

pub fn build_pathdb_from_pathdb_snapshot_with_options(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
    out_axpd: &Path,
    options: PathdbBuildOptions,
) -> Result<()> {
    ensure_layout(accepted_dir)?;

    let mut timings = if options.timings || options.timings_json.is_some() {
        Some(PathdbOperationTimingsV1 {
            version: "pathdb_timings_v1".to_string(),
            operation: "pathdb_build".to_string(),
            snapshot_id: None,
            accepted_snapshot_id: None,
            used_checkpoint: None,
            phases: Vec::new(),
            notes: Vec::new(),
        })
    } else {
        None
    };

    let phase_start = Instant::now();
    let snapshot_id = resolve_pathdb_snapshot_id(accepted_dir, snapshot_id_or_latest)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "resolve_pathdb_snapshot".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
        t.snapshot_id = Some(snapshot_id.clone());
    }

    // Fast path: if we have a checkpoint for this snapshot, just copy it out.
    let checkpoint = checkpoint_path(accepted_dir, &snapshot_id);
    if checkpoint.exists() && !options.rebuild {
        let phase_start = Instant::now();
        let size = checkpoint.metadata().ok().map(|m| m.len());

        // Prefer hard-links (fast "checkout") when possible; fall back to a
        // physical copy across filesystems.
        if out_axpd.exists() {
            let _ = fs::remove_file(out_axpd);
        }
        let mut used_hardlink = false;
        match fs::hard_link(&checkpoint, out_axpd) {
            Ok(()) => {
                used_hardlink = true;
            }
            Err(_) => {
                fs::copy(&checkpoint, out_axpd)?;
            }
        }

        if let Some(t) = timings.as_mut() {
            t.used_checkpoint = Some(true);
            t.phases.push(PhaseTimingV1 {
                name: "materialize_checkpoint".to_string(),
                millis: phase_start.elapsed().as_millis(),
            });
            if let Some(sz) = size {
                t.notes.push(format!("checkpoint_bytes={sz}"));
            }
            t.notes.push(format!(
                "checkpoint_materialize_mode={}",
                if used_hardlink { "hardlink" } else { "copy" }
            ));
        }

        if let Some(t) = timings.as_ref() {
            if options.timings {
                print_timings(t);
            }
            if let Some(path) = options.timings_json.as_ref() {
                write_timings_json(path, t)?;
            }
        }
        return Ok(());
    }

    if let Some(t) = timings.as_mut() {
        t.used_checkpoint = Some(false);
        if checkpoint.exists() && options.rebuild {
            t.notes.push("rebuild=true (ignored checkpoint)".to_string());
        }
    }

    let phase_start = Instant::now();
    let snapshot = read_pathdb_snapshot(accepted_dir, &snapshot_id)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "read_snapshot_manifest".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
        t.accepted_snapshot_id = Some(snapshot.accepted_snapshot_id.clone());
        t.notes.push(format!("ops_total={}", snapshot.ops.len()));
    }

    let phase_start = Instant::now();
    let mut db = build_base_from_accepted(accepted_dir, &snapshot.accepted_snapshot_id)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "build_base_from_accepted".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
    }

    let phase_start = Instant::now();
    for op in &snapshot.ops {
        apply_op(&mut db, accepted_dir, op)?;
    }
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "apply_ops".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
    }

    let phase_start = Instant::now();
    db.build_indexes();
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "build_indexes".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
    }

    let phase_start = Instant::now();
    fs::write(out_axpd, db.to_bytes()?)?;
    if let Some(t) = timings.as_mut() {
        t.phases.push(PhaseTimingV1 {
            name: "write_axpd".to_string(),
            millis: phase_start.elapsed().as_millis(),
        });
        if let Ok(m) = out_axpd.metadata() {
            t.notes.push(format!("out_bytes={}", m.len()));
        }
    }

    if let Some(t) = timings.as_ref() {
        if options.timings {
            print_timings(t);
        }
        if let Some(path) = options.timings_json.as_ref() {
            write_timings_json(path, t)?;
        }
    }
    Ok(())
}

// =============================================================================
// Layout + IO
// =============================================================================

fn ensure_layout(accepted_dir: &Path) -> Result<()> {
    let dir = accepted_dir.join(PATHDB_WAL_DIR);
    fs::create_dir_all(dir.join(PATHDB_WAL_SNAPSHOTS_DIR))?;
    fs::create_dir_all(dir.join(PATHDB_WAL_BLOBS_DIR))?;
    fs::create_dir_all(dir.join(PATHDB_WAL_CHECKPOINTS_DIR))?;

    let log_path = dir.join(PATHDB_WAL_LOG_V1);
    if !log_path.exists() {
        fs::write(&log_path, "")?;
    }
    Ok(())
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn digest_to_filename(digest: &str) -> String {
    digest.replace(':', "_")
}

fn resolve_accepted_snapshot_id(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<String> {
    crate::accepted_plane::resolve_snapshot_id_for_cli(accepted_dir, snapshot_id_or_latest)
}

fn pathdb_dir(accepted_dir: &Path) -> PathBuf {
    accepted_dir.join(PATHDB_WAL_DIR)
}

fn read_pathdb_head(accepted_dir: &Path) -> Result<Option<String>> {
    let path = pathdb_dir(accepted_dir).join(PATHDB_WAL_HEAD_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let id = text.trim().to_string();
    if id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(id))
    }
}

fn write_pathdb_head(accepted_dir: &Path, snapshot_id: &str) -> Result<()> {
    fs::write(
        pathdb_dir(accepted_dir).join(PATHDB_WAL_HEAD_FILE),
        format!("{snapshot_id}\n"),
    )?;
    Ok(())
}

fn resolve_pathdb_snapshot_id(accepted_dir: &Path, snapshot_id_or_latest: &str) -> Result<String> {
    let s = snapshot_id_or_latest.trim();
    if s.eq_ignore_ascii_case("latest") || s.eq_ignore_ascii_case("head") {
        return read_pathdb_head(accepted_dir)?
            .ok_or_else(|| anyhow!("pathdb wal has no HEAD snapshot yet"));
    }

    // Fast path: full id (manifest exists).
    if snapshot_manifest_path(accepted_dir, s).exists() {
        return Ok(s.to_string());
    }

    fn matches_snapshot_id(query: &str, id: &str) -> bool {
        if id.starts_with(query) {
            return true;
        }
        if let Some((_algo, rest)) = id.split_once(':') {
            if rest.starts_with(query) {
                return true;
            }
        }
        if query.contains('_') && !query.contains(':') {
            let query2 = query.replacen('_', ":", 1);
            if id.starts_with(&query2) {
                return true;
            }
            if let Some((_algo, rest)) = id.split_once(':') {
                if rest.starts_with(&query2) {
                    return true;
                }
            }
        }
        false
    }

    let snapshots_dir = pathdb_dir(accepted_dir).join(PATHDB_WAL_SNAPSHOTS_DIR);
    let rd = fs::read_dir(&snapshots_dir).map_err(|e| {
        anyhow!(
            "failed to read pathdb snapshots dir `{}`: {e}",
            snapshots_dir.display()
        )
    })?;

    let mut matches: Vec<String> = Vec::new();
    for entry in rd {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(snap) = serde_json::from_str::<PathDbSnapshotV1>(&text) else {
            continue;
        };
        if matches_snapshot_id(s, &snap.snapshot_id) {
            matches.push(snap.snapshot_id);
        }
    }
    matches.sort();
    matches.dedup();

    if matches.is_empty() {
        return Err(anyhow!(
            "unknown pathdb snapshot `{s}` (no matching manifest in `{}`)",
            snapshots_dir.display()
        ));
    }
    if matches.len() > 1 {
        let preview = matches
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow!(
            "ambiguous pathdb snapshot `{s}` (matches {}): {preview}",
            matches.len()
        ));
    }

    Ok(matches[0].clone())
}

fn snapshot_manifest_path(accepted_dir: &Path, snapshot_id: &str) -> PathBuf {
    let file = format!("{}.json", digest_to_filename(snapshot_id));
    pathdb_dir(accepted_dir)
        .join(PATHDB_WAL_SNAPSHOTS_DIR)
        .join(file)
}

fn read_pathdb_snapshot(accepted_dir: &Path, snapshot_id: &str) -> Result<PathDbSnapshotV1> {
    let path = snapshot_manifest_path(accepted_dir, snapshot_id);
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read pathdb snapshot manifest `{}`: {e}",
            path.display()
        )
    })?;
    let snapshot: PathDbSnapshotV1 = serde_json::from_str(&text)?;
    if snapshot.snapshot_id != snapshot_id {
        return Err(anyhow!(
            "pathdb snapshot manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            snapshot_id,
            snapshot.snapshot_id
        ));
    }
    Ok(snapshot)
}

fn write_pathdb_snapshot(accepted_dir: &Path, snapshot: &PathDbSnapshotV1) -> Result<()> {
    let path = snapshot_manifest_path(accepted_dir, &snapshot.snapshot_id);
    if path.exists() {
        // Idempotency: if it already exists, it must match.
        let existing = read_pathdb_snapshot(accepted_dir, &snapshot.snapshot_id)?;
        if existing != *snapshot {
            return Err(anyhow!(
                "pathdb snapshot id collision: `{}` already exists with different contents",
                snapshot.snapshot_id
            ));
        }
        return Ok(());
    }

    let json = serde_json::to_string_pretty(snapshot)?;
    fs::write(path, json)?;
    Ok(())
}

fn checkpoint_path(accepted_dir: &Path, snapshot_id: &str) -> PathBuf {
    let file = format!("{}.axpd", digest_to_filename(snapshot_id));
    pathdb_dir(accepted_dir)
        .join(PATHDB_WAL_CHECKPOINTS_DIR)
        .join(file)
}

fn write_checkpoint_if_missing(accepted_dir: &Path, snapshot_id: &str, bytes: &[u8]) -> Result<()> {
    let path = checkpoint_path(accepted_dir, snapshot_id);
    if path.exists() {
        return Ok(());
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn try_load_checkpoint(
    accepted_dir: &Path,
    snapshot_id: &str,
) -> Result<Option<axiograph_pathdb::PathDB>> {
    let path = checkpoint_path(accepted_dir, snapshot_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes)?;
    Ok(Some(db))
}

fn append_event(accepted_dir: &Path, event: &PathDbWalEventV1) -> Result<()> {
    let log_path = pathdb_dir(accepted_dir).join(PATHDB_WAL_LOG_V1);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| {
            anyhow!(
                "failed to open pathdb wal log `{}`: {e}",
                log_path.display()
            )
        })?;

    let line = serde_json::to_string(event)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

// =============================================================================
// Snapshot id + ops
// =============================================================================

fn pathdb_snapshot_id_v1(
    previous_snapshot_id: Option<&str>,
    accepted_snapshot_id: &str,
    ops: &[PathDbWalOpV1],
) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();
    let _ = write!(&mut s, "{PATHDB_WAL_VERSION_V1};");
    let _ = write!(&mut s, "prev={};", previous_snapshot_id.unwrap_or("(none)"));
    let _ = write!(&mut s, "accepted={accepted_snapshot_id};");
    for op in ops {
        match op {
            PathDbWalOpV1::ImportChunksV1 {
                chunks_digest,
                stored_path,
            } => {
                let _ = write!(
                    &mut s,
                    "op=import_chunks_v1;digest={chunks_digest};path={stored_path};"
                );
            }
            PathDbWalOpV1::ImportProposalsV1 {
                proposals_digest,
                stored_path,
            } => {
                let _ = write!(
                    &mut s,
                    "op=import_proposals_v1;digest={proposals_digest};path={stored_path};"
                );
            }
            PathDbWalOpV1::ImportEmbeddingsV1 {
                embeddings_digest,
                stored_path,
            } => {
                let _ = write!(
                    &mut s,
                    "op=import_embeddings_v1;digest={embeddings_digest};path={stored_path};"
                );
            }
        }
    }
    axiograph_dsl::digest::axi_digest_v1(&s)
}

fn store_chunks_blob(accepted_dir: &Path, chunks_path: &Path) -> Result<PathDbWalOpV1> {
    let bytes = fs::read(chunks_path)?;
    let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);

    let file_name = format!("{}.chunks.json", digest_to_filename(&digest));
    let stored_path = pathdb_dir(accepted_dir)
        .join(PATHDB_WAL_BLOBS_DIR)
        .join(file_name);

    if stored_path.exists() {
        let existing = fs::read(&stored_path)?;
        let existing_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&existing);
        if existing_digest != digest {
            return Err(anyhow!(
                "chunks blob collision: `{}` exists but digest mismatches (expected {digest}, got {existing_digest})",
                stored_path.display()
            ));
        }
    } else {
        fs::write(&stored_path, &bytes)?;
    }

    // Best-effort derived CBOR sidecar for faster WAL replay.
    let cbor_path = stored_path.with_extension("cbor");
    if !cbor_path.exists() {
        if let Ok(chunks) = serde_json::from_slice::<Vec<axiograph_ingest_docs::Chunk>>(&bytes) {
            let mut cbor_bytes: Vec<u8> = Vec::new();
            if ciborium::ser::into_writer(&chunks, &mut cbor_bytes).is_ok() {
                let _ = fs::write(&cbor_path, &cbor_bytes);
            }
        }
    }

    let rel = stored_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&stored_path)
        .to_string_lossy()
        .to_string();

    Ok(PathDbWalOpV1::ImportChunksV1 {
        chunks_digest: digest,
        stored_path: rel,
    })
}

fn store_proposals_blob(accepted_dir: &Path, proposals_path: &Path) -> Result<PathDbWalOpV1> {
    let bytes = fs::read(proposals_path)?;
    let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);

    let file_name = format!("{}.proposals.json", digest_to_filename(&digest));
    let stored_path = pathdb_dir(accepted_dir)
        .join(PATHDB_WAL_BLOBS_DIR)
        .join(file_name);

    if stored_path.exists() {
        let existing = fs::read(&stored_path)?;
        let existing_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&existing);
        if existing_digest != digest {
            return Err(anyhow!(
                "proposals blob collision: `{}` exists but digest mismatches (expected {digest}, got {existing_digest})",
                stored_path.display()
            ));
        }
    } else {
        fs::write(&stored_path, &bytes)?;
    }

    // Best-effort derived CBOR sidecar for faster WAL replay.
    let cbor_path = stored_path.with_extension("cbor");
    if !cbor_path.exists() {
        if let Ok(file) = serde_json::from_slice::<axiograph_ingest_docs::ProposalsFileV1>(&bytes) {
            let mut cbor_bytes: Vec<u8> = Vec::new();
            if ciborium::ser::into_writer(&file, &mut cbor_bytes).is_ok() {
                let _ = fs::write(&cbor_path, &cbor_bytes);
            }
        }
    }

    let rel = stored_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&stored_path)
        .to_string_lossy()
        .to_string();

    Ok(PathDbWalOpV1::ImportProposalsV1 {
        proposals_digest: digest,
        stored_path: rel,
    })
}

fn store_embeddings_blob_bytes(accepted_dir: &Path, bytes: &[u8]) -> Result<PathDbWalOpV1> {
    let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(bytes);

    let file_name = format!("{}.embeddings.cbor", digest_to_filename(&digest));
    let stored_path = pathdb_dir(accepted_dir)
        .join(PATHDB_WAL_BLOBS_DIR)
        .join(file_name);

    if stored_path.exists() {
        let existing = fs::read(&stored_path)?;
        let existing_digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&existing);
        if existing_digest != digest {
            return Err(anyhow!(
                "embeddings blob collision: `{}` exists but digest mismatches (expected {digest}, got {existing_digest})",
                stored_path.display()
            ));
        }
    } else {
        fs::write(&stored_path, bytes)?;
    }

    let rel = stored_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&stored_path)
        .to_string_lossy()
        .to_string();

    Ok(PathDbWalOpV1::ImportEmbeddingsV1 {
        embeddings_digest: digest,
        stored_path: rel,
    })
}

fn apply_op(
    db: &mut axiograph_pathdb::PathDB,
    accepted_dir: &Path,
    op: &PathDbWalOpV1,
) -> Result<()> {
    match op {
        PathDbWalOpV1::ImportChunksV1 {
            chunks_digest,
            stored_path,
        } => {
            let path = accepted_dir.join(stored_path);
            let bytes = fs::read(&path).map_err(|e| {
                anyhow!(
                    "failed to read chunks blob {} at `{}`: {e}",
                    chunks_digest,
                    path.display()
                )
            })?;
            let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
            if &digest != chunks_digest {
                return Err(anyhow!(
                    "chunks blob digest mismatch: manifest={} file={}",
                    chunks_digest,
                    digest
                ));
            }

            let cbor_path = path.with_extension("cbor");
            let mut parsed_from_json = false;
            let chunks: Vec<axiograph_ingest_docs::Chunk> = if cbor_path.exists() {
                match fs::read(&cbor_path)
                    .ok()
                    .and_then(|cbor_bytes| ciborium::de::from_reader(cbor_bytes.as_slice()).ok())
                {
                    Some(chunks) => chunks,
                    None => {
                        parsed_from_json = true;
                        serde_json::from_slice(&bytes)?
                    }
                }
            } else {
                parsed_from_json = true;
                serde_json::from_slice(&bytes)?
            };

            // Best-effort: if we had to parse JSON, cache a CBOR sidecar so future
            // replays can avoid JSON parsing.
            if parsed_from_json && !cbor_path.exists() {
                let mut cbor_bytes: Vec<u8> = Vec::new();
                if ciborium::ser::into_writer(&chunks, &mut cbor_bytes).is_ok() {
                    let _ = fs::write(&cbor_path, &cbor_bytes);
                }
            }
            let _summary = crate::doc_chunks::import_chunks_into_pathdb(db, &chunks)?;
            Ok(())
        }
        PathDbWalOpV1::ImportProposalsV1 {
            proposals_digest,
            stored_path,
        } => {
            let path = accepted_dir.join(stored_path);
            let bytes = fs::read(&path).map_err(|e| {
                anyhow!(
                    "failed to read proposals blob {} at `{}`: {e}",
                    proposals_digest,
                    path.display()
                )
            })?;
            let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
            if &digest != proposals_digest {
                return Err(anyhow!(
                    "proposals blob digest mismatch: manifest={} file={}",
                    proposals_digest,
                    digest
                ));
            }

            let cbor_path = path.with_extension("cbor");
            let mut parsed_from_json = false;
            let file: axiograph_ingest_docs::ProposalsFileV1 = if cbor_path.exists() {
                match fs::read(&cbor_path)
                    .ok()
                    .and_then(|cbor_bytes| ciborium::de::from_reader(cbor_bytes.as_slice()).ok())
                {
                    Some(file) => file,
                    None => {
                        parsed_from_json = true;
                        serde_json::from_slice(&bytes)?
                    }
                }
            } else {
                parsed_from_json = true;
                serde_json::from_slice(&bytes)?
            };

            // Best-effort: if we had to parse JSON, cache a CBOR sidecar so future
            // replays can avoid JSON parsing.
            if parsed_from_json && !cbor_path.exists() {
                let mut cbor_bytes: Vec<u8> = Vec::new();
                if ciborium::ser::into_writer(&file, &mut cbor_bytes).is_ok() {
                    let _ = fs::write(&cbor_path, &cbor_bytes);
                }
            }
            crate::proposals_import::import_proposals_file_into_pathdb(
                db,
                &file,
                proposals_digest,
                )?;
            Ok(())
        }
        PathDbWalOpV1::ImportEmbeddingsV1 {
            embeddings_digest,
            stored_path,
        } => {
            let path = accepted_dir.join(stored_path);
            let bytes = fs::read(&path).map_err(|e| {
                anyhow!(
                    "failed to read embeddings blob {} at `{}`: {e}",
                    embeddings_digest,
                    path.display()
                )
            })?;
            let digest = axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes);
            if &digest != embeddings_digest {
                return Err(anyhow!(
                    "embeddings blob digest mismatch: manifest={} file={}",
                    embeddings_digest,
                    digest
                ));
            }

            // Embeddings are currently a sidecar artifact; `.axpd` stays lean.
            Ok(())
        }
    }
}

fn rebuild_from_accepted_and_ops(
    accepted_dir: &Path,
    accepted_snapshot_id: &str,
    ops: &[PathDbWalOpV1],
) -> Result<axiograph_pathdb::PathDB> {
    let mut db = build_base_from_accepted(accepted_dir, accepted_snapshot_id)?;
    for op in ops {
        apply_op(&mut db, accepted_dir, op)?;
    }
    Ok(db)
}

fn build_base_from_accepted(
    accepted_dir: &Path,
    accepted_snapshot_id: &str,
) -> Result<axiograph_pathdb::PathDB> {
    let snapshot = read_accepted_snapshot(accepted_dir, accepted_snapshot_id)?;
    let mut db = axiograph_pathdb::PathDB::new();

    let mut module_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    for (module_name, module_ref) in &snapshot.modules {
        let path = accepted_dir.join(&module_ref.stored_path);
        let text = fs::read_to_string(&path).map_err(|e| {
            anyhow!(
                "failed to read module `{}` at `{}`: {e}",
                module_name,
                path.display()
            )
        })?;

        let digest = axiograph_dsl::digest::axi_digest_v1(&text);
        if digest != module_ref.module_digest {
            return Err(anyhow!(
                "module `{}` digest mismatch: manifest={} file={}",
                module_name,
                module_ref.module_digest,
                digest
            ));
        }

        let module = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            &mut db, &module,
        )?;

        module_chunks.push(crate::doc_chunks::chunk_from_axi_module_text(
            module_name,
            &digest,
            &text,
        ));
    }

    let _ = crate::doc_chunks::import_chunks_into_pathdb(&mut db, &module_chunks);
    Ok(db)
}

fn read_accepted_snapshot(
    accepted_dir: &Path,
    snapshot_id: &str,
) -> Result<AcceptedPlaneSnapshotV1> {
    let file = format!("{}.json", digest_to_filename(snapshot_id));
    let path = accepted_dir.join("snapshots").join(file);
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read accepted snapshot manifest `{}`: {e}",
            path.display()
        )
    })?;
    let snapshot: AcceptedPlaneSnapshotV1 = serde_json::from_str(&text)?;
    if snapshot.snapshot_id != snapshot_id {
        return Err(anyhow!(
            "accepted snapshot manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            snapshot_id,
            snapshot.snapshot_id
        ));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let pid = std::process::id();
            let path = std::env::temp_dir().join(format!("{prefix}_{pid}_{ts}"));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn pathdb_commit_with_chunks_imports_docchunks() {
        let tmp = TempDirGuard::new("axiograph_pathdb_wal_chunks_test");
        let accepted_dir = &tmp.path;

        let axi_path = accepted_dir.join("Test.axi");
        fs::write(
            &axi_path,
            r#"module Test

schema S:
  object Person
  relation Parent(parent: Person, child: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {
    (parent=Alice, child=Bob)
  }
"#,
        )
        .expect("write .axi");

        let accepted_snapshot_id = crate::accepted_plane::promote_reviewed_module(
            &axi_path,
            accepted_dir,
            Some("test: promote"),
            "off",
        )
        .expect("promote accepted snapshot");

        let chunks_path = accepted_dir.join("chunks.json");
        fs::write(
            &chunks_path,
            r#"[
  {
    "chunk_id": "doc_test_0",
    "document_id": "Test.axi",
    "page": null,
    "span_id": "para_0",
    "text": "Alice is Bob's parent.",
    "bbox": null,
    "metadata": {"kind":"demo_note"}
  }
]
"#,
        )
        .expect("write chunks.json");

        let res = commit_pathdb_snapshot_with_overlays(
            accepted_dir,
            &accepted_snapshot_id,
            &[chunks_path],
            &[],
            Some("test: commit chunks"),
        )
        .expect("commit pathdb snapshot with chunks");
        assert_eq!(res.accepted_snapshot_id, accepted_snapshot_id);

        // Ensure we wrote a CBOR sidecar for fast replay.
        let snap = read_pathdb_snapshot(accepted_dir, &res.snapshot_id).expect("read snapshot");
        let mut saw_chunks_op = false;
        for op in &snap.ops {
            if let PathDbWalOpV1::ImportChunksV1 { stored_path, .. } = op {
                saw_chunks_op = true;
                let json_path = accepted_dir.join(stored_path);
                assert!(json_path.exists(), "expected chunks blob at {}", json_path.display());
                let cbor_path = json_path.with_extension("cbor");
                assert!(
                    cbor_path.exists(),
                    "expected chunks.cbor sidecar at {}",
                    cbor_path.display()
                );
            }
        }
        assert!(saw_chunks_op, "expected ImportChunksV1 op in snapshot");

        let out_axpd = accepted_dir.join("out.axpd");
        build_pathdb_from_pathdb_snapshot(accepted_dir, &res.snapshot_id, &out_axpd)
            .expect("build pathdb from wal snapshot");

        let bytes = fs::read(&out_axpd).expect("read out.axpd");
        let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("decode .axpd");

        let Some(chunks) = db.find_by_type("DocChunk") else {
            panic!("expected DocChunk entities in the PathDB snapshot");
        };
        assert!(!chunks.is_empty(), "expected at least one DocChunk");
    }

    #[test]
    fn pathdb_commit_with_proposals_writes_cbor_sidecar() {
        use std::collections::HashMap;

        let tmp = TempDirGuard::new("axiograph_pathdb_wal_proposals_test");
        let accepted_dir = &tmp.path;

        let axi_path = accepted_dir.join("Test.axi");
        fs::write(
            &axi_path,
            r#"module Test

schema S:
  object Person
  relation Parent(parent: Person, child: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {
    (parent=Alice, child=Bob)
  }
"#,
        )
        .expect("write .axi");

        let accepted_snapshot_id = crate::accepted_plane::promote_reviewed_module(
            &axi_path,
            accepted_dir,
            Some("test: promote"),
            "off",
        )
        .expect("promote accepted snapshot");

        let proposals = axiograph_ingest_docs::ProposalsFileV1 {
            version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
            generated_at: "0".to_string(),
            source: axiograph_ingest_docs::ProposalSourceV1 {
                source_type: "test".to_string(),
                locator: "pathdb_wal.rs".to_string(),
            },
            schema_hint: None,
            proposals: vec![axiograph_ingest_docs::ProposalV1::Relation {
                meta: axiograph_ingest_docs::ProposalMetaV1 {
                    proposal_id: "rel::Parent::Alice::Bob".to_string(),
                    confidence: 0.9,
                    evidence: Vec::new(),
                    public_rationale: "test relation".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: None,
                },
                relation_id: "rel::Parent::Alice::Bob".to_string(),
                rel_type: "Parent".to_string(),
                source: "Alice".to_string(),
                target: "Bob".to_string(),
                attributes: HashMap::new(),
            }],
        };

        let proposals_path = accepted_dir.join("proposals.json");
        fs::write(
            &proposals_path,
            serde_json::to_string_pretty(&proposals).unwrap_or_default(),
        )
        .expect("write proposals.json");

        let res = commit_pathdb_snapshot_with_overlays(
            accepted_dir,
            &accepted_snapshot_id,
            &[],
            &[proposals_path],
            Some("test: commit proposals"),
        )
        .expect("commit pathdb snapshot with proposals");
        assert_eq!(res.accepted_snapshot_id, accepted_snapshot_id);

        let snap = read_pathdb_snapshot(accepted_dir, &res.snapshot_id).expect("read snapshot");
        let mut saw_proposals_op = false;
        for op in &snap.ops {
            if let PathDbWalOpV1::ImportProposalsV1 { stored_path, .. } = op {
                saw_proposals_op = true;
                let json_path = accepted_dir.join(stored_path);
                assert!(
                    json_path.exists(),
                    "expected proposals blob at {}",
                    json_path.display()
                );
                let cbor_path = json_path.with_extension("cbor");
                assert!(
                    cbor_path.exists(),
                    "expected proposals.cbor sidecar at {}",
                    cbor_path.display()
                );
            }
        }
        assert!(saw_proposals_op, "expected ImportProposalsV1 op in snapshot");
    }
}
