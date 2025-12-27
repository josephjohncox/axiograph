//! Snapshot-store syncing (accepted plane + PathDB WAL).
//!
//! This is a **pragmatic first step** toward distribution:
//! - treat the snapshot store as an append-only, content-addressed directory tree
//! - replicate by copying missing immutable objects + updating HEAD pointers
//!
//! This is intentionally filesystem-based (no networking). It enables:
//! - a “write master / read replicas” deployment using rsync/NFS/object-store sync
//! - offline replication by copying a directory
//!
//! Future directions:
//! - HTTP/object-store sync (`accept pull`, `accept serve`)
//! - authenticated membership proofs for offline verification

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncLayer {
    Accepted,
    Pathdb,
    Both,
}

impl SyncLayer {
    pub fn parse(s: &str) -> Result<Self> {
        let norm = s.trim().to_ascii_lowercase();
        match norm.as_str() {
            "accepted" => Ok(Self::Accepted),
            "pathdb" => Ok(Self::Pathdb),
            "both" => Ok(Self::Both),
            other => Err(anyhow!(
                "unknown --layer `{other}` (expected accepted|pathdb|both)"
            )),
        }
    }
}

pub fn sync_snapshot_store_dirs(
    from_dir: &Path,
    to_dir: &Path,
    layer: SyncLayer,
    include_checkpoints: bool,
    include_logs: bool,
    update_head: bool,
    dry_run: bool,
) -> Result<SyncStats> {
    if !from_dir.is_dir() {
        return Err(anyhow!("--from is not a directory: {}", from_dir.display()));
    }
    fs::create_dir_all(to_dir)?;

    // Ensure destination layout exists (idempotent).
    crate::accepted_plane::init_accepted_plane_dir(to_dir)?;
    crate::pathdb_wal::init_pathdb_wal_dir(to_dir)?;

    let mut stats = SyncStats::default();

    // Copy immutable trees first, then update HEAD pointers.
    match layer {
        SyncLayer::Accepted => {
            stats += sync_accepted_plane(from_dir, to_dir, include_logs, update_head, dry_run)?;
        }
        SyncLayer::Pathdb => {
            stats += sync_pathdb_wal(
                from_dir,
                to_dir,
                include_checkpoints,
                include_logs,
                update_head,
                dry_run,
            )?;
        }
        SyncLayer::Both => {
            stats += sync_accepted_plane(from_dir, to_dir, include_logs, update_head, dry_run)?;
            stats += sync_pathdb_wal(
                from_dir,
                to_dir,
                include_checkpoints,
                include_logs,
                update_head,
                dry_run,
            )?;
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SyncStats {
    pub files_copied: usize,
    pub bytes_copied: u64,
}

impl std::ops::AddAssign for SyncStats {
    fn add_assign(&mut self, rhs: Self) {
        self.files_copied = self.files_copied.saturating_add(rhs.files_copied);
        self.bytes_copied = self.bytes_copied.saturating_add(rhs.bytes_copied);
    }
}

fn sync_accepted_plane(
    from_dir: &Path,
    to_dir: &Path,
    include_logs: bool,
    update_head: bool,
    dry_run: bool,
) -> Result<SyncStats> {
    let mut stats = SyncStats::default();

    // Content-addressed trees.
    stats += sync_tree_if_exists(
        from_dir.join("modules"),
        to_dir.join("modules"),
        /*content_addressed=*/ true,
        dry_run,
    )?;
    stats += sync_tree_if_exists(
        from_dir.join("snapshots"),
        to_dir.join("snapshots"),
        /*content_addressed=*/ true,
        dry_run,
    )?;
    stats += sync_tree_if_exists(
        from_dir.join("quality"),
        to_dir.join("quality"),
        /*content_addressed=*/ true,
        dry_run,
    )?;

    // Logs are append-only but not content-addressed.
    if include_logs {
        stats += copy_file_if_exists(
            &from_dir.join("accepted_plane.log.jsonl"),
            &to_dir.join("accepted_plane.log.jsonl"),
            /*only_if_missing=*/ false,
            dry_run,
        )?;
    }

    // HEAD is the only mutable pointer we typically care about.
    if update_head {
        stats += copy_file_if_exists(
            &from_dir.join("HEAD"),
            &to_dir.join("HEAD"),
            /*only_if_missing=*/ false,
            dry_run,
        )?;
    }

    Ok(stats)
}

fn sync_pathdb_wal(
    from_dir: &Path,
    to_dir: &Path,
    include_checkpoints: bool,
    include_logs: bool,
    update_head: bool,
    dry_run: bool,
) -> Result<SyncStats> {
    let mut stats = SyncStats::default();

    let from = from_dir.join("pathdb");
    let to = to_dir.join("pathdb");

    stats += sync_tree_if_exists(
        from.join("blobs"),
        to.join("blobs"),
        /*content_addressed=*/ true,
        dry_run,
    )?;
    stats += sync_tree_if_exists(
        from.join("snapshots"),
        to.join("snapshots"),
        /*content_addressed=*/ true,
        dry_run,
    )?;
    if include_checkpoints {
        stats += sync_tree_if_exists(
            from.join("checkpoints"),
            to.join("checkpoints"),
            /*content_addressed=*/ true,
            dry_run,
        )?;
    }

    if include_logs {
        stats += copy_file_if_exists(
            &from.join("pathdb_wal.log.jsonl"),
            &to.join("pathdb_wal.log.jsonl"),
            /*only_if_missing=*/ false,
            dry_run,
        )?;
    }

    if update_head {
        stats += copy_file_if_exists(
            &from.join("HEAD"),
            &to.join("HEAD"),
            /*only_if_missing=*/ false,
            dry_run,
        )?;
    }

    Ok(stats)
}

fn sync_tree_if_exists(
    from: PathBuf,
    to: PathBuf,
    content_addressed: bool,
    dry_run: bool,
) -> Result<SyncStats> {
    if !from.exists() {
        return Ok(SyncStats::default());
    }
    if !from.is_dir() {
        return Err(anyhow!("expected directory: {}", from.display()));
    }
    if !dry_run {
        fs::create_dir_all(&to)?;
    }

    let mut stats = SyncStats::default();
    for entry in WalkDir::new(&from).follow_links(false) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if entry.file_type().is_dir() {
            continue;
        }

        let src = entry.path();
        let rel = match src.strip_prefix(&from) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let dst = to.join(rel);

        let only_if_missing = content_addressed;
        stats += copy_file_if_exists(src, &dst, only_if_missing, dry_run)?;
    }

    Ok(stats)
}

fn copy_file_if_exists(
    src: &Path,
    dst: &Path,
    only_if_missing: bool,
    dry_run: bool,
) -> Result<SyncStats> {
    if !src.exists() {
        return Ok(SyncStats::default());
    }
    if !src.is_file() {
        return Err(anyhow!("expected file: {}", src.display()));
    }
    if only_if_missing && dst.exists() {
        return Ok(SyncStats::default());
    }

    let bytes = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    if dry_run {
        return Ok(SyncStats {
            files_copied: 1,
            bytes_copied: bytes,
        });
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)
        .with_context(|| format!("failed to copy {} -> {}", src.display(), dst.display()))?;
    Ok(SyncStats {
        files_copied: 1,
        bytes_copied: bytes,
    })
}
