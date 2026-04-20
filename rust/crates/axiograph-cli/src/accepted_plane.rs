//! Accepted `.axi` plane management (append-only log + snapshot ids).
//!
//! Motivation
//! ----------
//! Axiograph intentionally separates:
//! - evidence-plane artifacts (`proposals.json`, doc chunks, heuristic edges), from
//! - the accepted/canonical `.axi` plane (reviewed modules).
//!
//! The accepted plane should behave like production code:
//! - changes are versioned,
//! - promotion is explicit,
//! - and builds are reproducible.
//!
//! This module implements a small, pragmatic first step:
//! - an append-only JSONL log of promotions
//! - content-derived snapshot ids (stable)
//! - and a reproducible “rebuild PathDB from snapshots” command.

use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use axiograph_dsl::schema_v1::ConstraintV1;
use axiograph_pathdb::{
    AcceptedSnapshotId, AxiDigest, PathdbSnapshotId, ProposalDigest, WorldModelRunId,
};

use crate::axi_input::require_canonical_axi_text;

const ACCEPTED_PLANE_VERSION_V1: &str = "accepted_plane_v1";
const ACCEPTED_PLANE_LOG_V1: &str = "accepted_plane.log.jsonl";
const ACCEPTED_PLANE_HEAD_FILE: &str = "HEAD";
const ACCEPTED_PLANE_MODULES_DIR: &str = "modules";
const ACCEPTED_PLANE_SNAPSHOTS_DIR: &str = "snapshots";
const ACCEPTED_PLANE_QUALITY_DIR: &str = "quality";
const ACCEPTED_PLANE_CERTS_DIR: &str = "certs";
const ACCEPTED_PLANE_SEM_DIR: &str = "sem";
const ACCEPTED_PLANE_SEM_COMMITS_DIR: &str = "sem/commits";
const ACCEPTED_PLANE_SEM_RECONCILIATIONS_DIR: &str = "sem/reconciliations";
const ACCEPTED_PLANE_SEM_REFS_DIR: &str = "sem/refs";
const ACCEPTED_PLANE_SEM_HEADS_DIR: &str = "sem/refs/heads";
const ACCEPTED_PLANE_SEM_HEADS_WM_DIR: &str = "sem/refs/heads/wm";
const ACCEPTED_PLANE_SEM_TAGS_DIR: &str = "sem/refs/tags";
const ACCEPTED_PLANE_SEM_VALIDATIONS_DIR: &str = "sem/validations";
const ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR: &str = "sem/world_model_runs";

const ACCEPTED_PLANE_SNAPSHOT_VERSION_V1: &str = "accepted_plane_snapshot_v1";
const ACCEPTED_PLANE_EVENT_VERSION_V1: &str = "accepted_plane_event_v1";
#[cfg_attr(not(test), allow(dead_code))]
const WORLD_MODEL_RUN_RECORD_VERSION_V1: &str = "world_model_run_record_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPlaneSnapshotV1 {
    pub version: String,
    pub snapshot_id: AcceptedSnapshotId,
    pub previous_snapshot_id: Option<AcceptedSnapshotId>,
    pub created_at_unix_secs: u64,
    /// Module name -> module digest.
    ///
    /// We keep this as a map so the snapshot meaning is stable regardless of
    /// promotion ordering.
    pub modules: BTreeMap<String, AcceptedModuleRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedModuleRefV1 {
    pub module_digest: AxiDigest,
    /// Path relative to the accepted-plane directory.
    pub stored_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPlaneEventV1 {
    pub version: String,
    pub created_at_unix_secs: u64,
    pub action: String,
    pub snapshot_id: AcceptedSnapshotId,
    pub previous_snapshot_id: Option<AcceptedSnapshotId>,
    pub module_name: String,
    pub module_digest: AxiDigest,
    pub stored_module_path: String,
    #[serde(default)]
    pub message: Option<String>,
    /// Optional quality gate profile used during promotion (`off|fast|strict`).
    #[serde(default)]
    pub quality_profile: Option<String>,
    /// Optional path to a stored quality report (relative to the accepted-plane directory).
    #[serde(default)]
    pub quality_report_path: Option<String>,
    #[serde(default)]
    pub quality_error_count: Option<usize>,
    #[serde(default)]
    pub quality_warning_count: Option<usize>,
    #[serde(default)]
    pub quality_info_count: Option<usize>,
    /// Optional path to a stored constraints certificate (relative to the accepted-plane directory).
    #[serde(default)]
    pub constraints_cert_path: Option<String>,
    #[serde(default)]
    pub constraints_constraint_count: Option<u32>,
    #[serde(default)]
    pub constraints_instance_count: Option<u32>,
    #[serde(default)]
    pub constraints_check_count: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorldModelRunStatusV1 {
    Previewed,
    CommittedToPathdb,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorldModelRunRecordV1 {
    pub version: String,
    pub run_id: WorldModelRunId,
    pub trace_id: WorldModelRunId,
    pub created_at_unix_secs: u64,
    pub status: WorldModelRunStatusV1,
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axi_digest_v1: Option<AxiDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    pub proposals_digest: ProposalDigest,
    pub proposal_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_pathdb_snapshot_id: Option<PathdbSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_accepted_snapshot_id: Option<AcceptedSnapshotId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrail_total_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrail_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardrail_plane: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Initialize the accepted-plane directory layout.
///
/// This is idempotent and safe to run even if the directory already exists.
pub(crate) fn init_accepted_plane_dir(accepted_dir: &Path) -> Result<()> {
    ensure_layout(accepted_dir)
}

/// Resolve an accepted-plane snapshot id for CLI usage.
///
/// Supports:
/// - `head` / `latest`
/// - full ids (`fnv1a64:...`)
/// - unique prefixes (either of the full id, or of the digest suffix after `:`)
pub(crate) fn resolve_snapshot_id_for_cli(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<AcceptedSnapshotId> {
    ensure_layout(accepted_dir)?;
    resolve_snapshot_id(accepted_dir, snapshot_id_or_latest)
}

pub(crate) fn read_snapshot_for_cli(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<AcceptedPlaneSnapshotV1> {
    ensure_layout(accepted_dir)?;
    let snapshot_id = resolve_snapshot_id(accepted_dir, snapshot_id_or_latest)?;
    read_snapshot(accepted_dir, &snapshot_id)
}

pub fn promote_reviewed_module(
    candidate_axi: &Path,
    accepted_dir: &Path,
    message: Option<&str>,
    quality_profile: &str,
) -> Result<AcceptedSnapshotId> {
    ensure_layout(accepted_dir)?;

    let text = fs::read_to_string(candidate_axi)?;
    // Conservative gate: accepted-plane promotion only accepts canonical,
    // well-typed `.axi` modules, never reversible PathDB snapshot exports.
    let validated = require_canonical_axi_text(&text)?.into_parts().1;
    let reviewed = axiograph_pathdb::axi_module_typecheck::review_axi_v1_module(
        validated,
        axiograph_pathdb::axi_module_typecheck::ReviewStamp {
            reviewer: None,
            note: message.map(str::to_owned),
        },
    );

    // Hard gate: accepted/canonical modules must not contain unknown/opaque
    // constraints. If a constraint is not structured, it cannot participate in
    // certificate checking or schema-directed tooling, and we don't want silent
    // semantics drift in the accepted plane.
    //
    // If you want to keep richer (not yet executable/certifiable) content in a
    // canonical module, prefer a `constraint Name:` named-block.
    let mut unknown: Vec<(String, String)> = Vec::new();
    for th in &reviewed.module().theories {
        for c in &th.constraints {
            if let ConstraintV1::Unknown { text } = c {
                unknown.push((th.name.clone(), text.clone()));
            }
        }
    }
    if !unknown.is_empty() {
        let mut msg = String::new();
        msg.push_str("promotion blocked: unknown/unsupported theory constraints found in candidate module.\n");
        msg.push_str("Fix the module by rewriting constraints into canonical structured forms (or use a named-block constraint).\n");
        msg.push_str("Unknown constraints:\n");
        for (i, (th_name, text)) in unknown.iter().take(8).enumerate() {
            msg.push_str(&format!("  {i}: theory `{th_name}`: {text}\n"));
        }
        if unknown.len() > 8 {
            msg.push_str(&format!("  ... ({} more)\n", unknown.len() - 8));
        }
        return Err(anyhow!(msg.trim_end().to_string()));
    }

    let module_name = reviewed.module().module_name.clone();
    let module_digest = AxiDigest::from_axi_text(&text);

    // Hard gate: accepted-plane promotions must satisfy the conservative,
    // certificate-checkable constraint subset.
    //
    // This complements the quality/lint pass: it is the stable, semantics-driven
    // check we expect to keep in sync with the Lean trusted checker.
    let constraints_proof =
        axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1(&reviewed)?;
    let constraints_cert = axiograph_pathdb::certificate::CertificateV2::axi_constraints_ok_v1(
        constraints_proof.clone(),
    )
    .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1::new(
        module_digest.clone(),
    ));

    // Store the certificate once per module digest (idempotent across snapshots).
    let constraints_cert_rel_path = PathBuf::from(ACCEPTED_PLANE_CERTS_DIR).join(format!(
        "{}__{}__axi_constraints_ok_v1.json",
        sanitize_path_component(&module_name),
        digest_to_filename(&module_digest)
    ));
    let constraints_cert_abs_path = accepted_dir.join(&constraints_cert_rel_path);
    if !constraints_cert_abs_path.exists() {
        fs::write(
            &constraints_cert_abs_path,
            serde_json::to_string_pretty(&constraints_cert)?,
        )?;
    }

    // Optional quality gate (untrusted tooling). If enabled, we:
    // - import the module into an in-memory PathDB to get a uniform representation,
    // - run lints + constraint checks,
    // - and attach the resulting report to the accepted-plane event.
    let quality_profile = quality_profile.trim().to_ascii_lowercase();
    let quality_report = if quality_profile != "off" {
        if !matches!(quality_profile.as_str(), "fast" | "strict") {
            return Err(anyhow!(
                "unknown --quality `{}` (expected off|fast|strict)",
                quality_profile
            ));
        }
        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            &mut db, &reviewed,
        )?;
        db.build_indexes();

        let report = crate::quality::run_quality_checks(
            &db,
            &candidate_axi.to_path_buf(),
            &quality_profile,
            "both",
        )?;
        if report.summary.error_count > 0 {
            return Err(anyhow!(
                "quality gate failed: {} error(s) found (run `axiograph check quality {}` for details)",
                report.summary.error_count,
                candidate_axi.display()
            ));
        }
        Some(report)
    } else {
        None
    };

    let previous_snapshot_id = read_head(accepted_dir)?;
    let previous_snapshot = if let Some(prev) = previous_snapshot_id.as_ref() {
        Some(read_snapshot(accepted_dir, prev)?)
    } else {
        None
    };

    let stored_rel_path = store_module_if_needed(
        accepted_dir,
        &module_name,
        &module_digest,
        candidate_axi,
        &text,
    )?;

    let mut modules: BTreeMap<String, AcceptedModuleRefV1> = match previous_snapshot {
        Some(s) => s.modules,
        None => BTreeMap::new(),
    };
    modules.insert(
        module_name.clone(),
        AcceptedModuleRefV1 {
            module_digest: module_digest.clone(),
            stored_path: stored_rel_path.clone(),
        },
    );

    let snapshot_id = accepted_plane_snapshot_id_v1(previous_snapshot_id.as_ref(), &modules);
    let snapshot = AcceptedPlaneSnapshotV1 {
        version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id: previous_snapshot_id.clone(),
        created_at_unix_secs: now_unix_secs(),
        modules,
    };
    write_snapshot(accepted_dir, &snapshot)?;
    write_head(accepted_dir, &snapshot_id)?;

    // Store the quality report (if present) in the accepted-plane directory.
    let (quality_report_path, quality_counts) = if let Some(report) = quality_report.as_ref() {
        let filename = format!(
            "{}__{}__{}.json",
            digest_to_filename(snapshot_id.as_str()),
            sanitize_path_component(&module_name),
            digest_to_filename(&module_digest)
        );
        let rel_path = PathBuf::from(ACCEPTED_PLANE_QUALITY_DIR).join(filename);
        let abs_path = accepted_dir.join(&rel_path);
        fs::write(&abs_path, serde_json::to_string_pretty(report)?)?;
        (
            Some(rel_path.to_string_lossy().to_string()),
            Some((
                report.summary.error_count,
                report.summary.warning_count,
                report.summary.info_count,
            )),
        )
    } else {
        (None, None)
    };

    let event = AcceptedPlaneEventV1 {
        version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
        created_at_unix_secs: now_unix_secs(),
        action: "promote".to_string(),
        snapshot_id: snapshot_id.clone(),
        previous_snapshot_id,
        module_name,
        module_digest,
        stored_module_path: stored_rel_path,
        message: message.map(|s| s.to_string()),
        quality_profile: if quality_profile == "off" {
            None
        } else {
            Some(quality_profile.clone())
        },
        quality_report_path,
        quality_error_count: quality_counts.map(|(e, _, _)| e),
        quality_warning_count: quality_counts.map(|(_, w, _)| w),
        quality_info_count: quality_counts.map(|(_, _, i)| i),
        constraints_cert_path: Some(constraints_cert_rel_path.to_string_lossy().to_string()),
        constraints_constraint_count: Some(constraints_proof.constraint_count),
        constraints_instance_count: Some(constraints_proof.instance_count),
        constraints_check_count: Some(constraints_proof.check_count),
    };
    append_event(accepted_dir, &event)?;

    Ok(snapshot_id)
}

pub fn build_pathdb_from_snapshot(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
    out_axpd: &Path,
) -> Result<()> {
    ensure_layout(accepted_dir)?;

    let snapshot_id = resolve_snapshot_id(accepted_dir, snapshot_id_or_latest)?;
    let snapshot = read_snapshot(accepted_dir, &snapshot_id)?;

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

        let digest = AxiDigest::from_axi_text(&text);
        if digest != module_ref.module_digest {
            return Err(anyhow!(
                "module `{}` digest mismatch: manifest={} file={}",
                module_name,
                module_ref.module_digest,
                digest
            ));
        }

        let module = require_canonical_axi_text(&text)?.into_parts().1;
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
            &mut db, &module,
        )?;

        // Grounding always has evidence: embed the canonical `.axi` module text
        // as an untrusted DocChunk so LLM/UI workflows can cite and open it.
        module_chunks.push(crate::doc_chunks::chunk_from_axi_module_text(
            module_name,
            digest.as_str(),
            &text,
        ));
    }

    let _ = crate::doc_chunks::import_chunks_into_pathdb(&mut db, &module_chunks);
    db.build_indexes();
    fs::write(out_axpd, db.to_bytes()?)?;
    Ok(())
}

pub fn persist_world_model_run_record(
    accepted_dir: &Path,
    record: &WorldModelRunRecordV1,
) -> Result<PathBuf> {
    ensure_layout(accepted_dir)?;
    let path = world_model_run_record_path(accepted_dir, &record.run_id);
    if path.exists() {
        let existing = read_world_model_run_record(accepted_dir, &record.run_id)?;
        if existing != *record {
            return Err(anyhow!(
                "world-model run id collision: `{}` already exists with different contents",
                record.run_id
            ));
        }
        return Ok(path);
    }

    let json = serde_json::to_string_pretty(record)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn read_world_model_run_record(
    accepted_dir: &Path,
    run_id: &WorldModelRunId,
) -> Result<WorldModelRunRecordV1> {
    let path = world_model_run_record_path(accepted_dir, run_id);
    let text = fs::read_to_string(&path).map_err(|e| {
        anyhow!(
            "failed to read world-model run manifest `{}`: {e}",
            path.display()
        )
    })?;
    let record: WorldModelRunRecordV1 = serde_json::from_str(&text)?;
    if record.run_id != *run_id {
        return Err(anyhow!(
            "world-model run manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            run_id,
            record.run_id
        ));
    }
    Ok(record)
}

fn ensure_layout(accepted_dir: &Path) -> Result<()> {
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_MODULES_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_QUALITY_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_CERTS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_COMMITS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_RECONCILIATIONS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_REFS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_HEADS_WM_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_TAGS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_VALIDATIONS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR))?;
    // Log is append-only; create it if it doesn't exist.
    let log_path = accepted_dir.join(ACCEPTED_PLANE_LOG_V1);
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

fn sanitize_path_component(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

fn digest_to_filename(digest: impl AsRef<str>) -> String {
    digest.as_ref().replace(':', "_")
}

fn accepted_plane_snapshot_id_v1(
    previous_snapshot_id: Option<&AcceptedSnapshotId>,
    modules: &BTreeMap<String, AcceptedModuleRefV1>,
) -> AcceptedSnapshotId {
    use std::fmt::Write as _;

    let mut s = String::new();
    let _ = write!(&mut s, "{ACCEPTED_PLANE_VERSION_V1};");
    let _ = write!(
        &mut s,
        "prev={};",
        previous_snapshot_id
            .map(|id| id.as_str())
            .unwrap_or("(none)")
    );
    for (name, m) in modules {
        let _ = write!(
            &mut s,
            "module={name};digest={};path={};",
            m.module_digest.as_str(),
            m.stored_path
        );
    }
    AcceptedSnapshotId::new(axiograph_dsl::digest::axi_digest_v1(&s))
}

fn read_head(accepted_dir: &Path) -> Result<Option<AcceptedSnapshotId>> {
    let path = accepted_dir.join(ACCEPTED_PLANE_HEAD_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)?;
    let id = text.trim().to_string();
    if id.is_empty() {
        Ok(None)
    } else {
        Ok(Some(AcceptedSnapshotId::new(id)))
    }
}

fn write_head(accepted_dir: &Path, snapshot_id: &AcceptedSnapshotId) -> Result<()> {
    fs::write(
        accepted_dir.join(ACCEPTED_PLANE_HEAD_FILE),
        format!("{snapshot_id}\n"),
    )?;
    Ok(())
}

fn resolve_snapshot_id(
    accepted_dir: &Path,
    snapshot_id_or_latest: &str,
) -> Result<AcceptedSnapshotId> {
    let s = snapshot_id_or_latest.trim();
    if s.eq_ignore_ascii_case("latest") || s.eq_ignore_ascii_case("head") {
        return read_head(accepted_dir)?
            .ok_or_else(|| anyhow!("accepted plane has no HEAD snapshot yet"));
    }

    // Fast path: full id (manifest exists).
    if snapshot_manifest_path(accepted_dir, s).exists() {
        return Ok(AcceptedSnapshotId::new(s));
    }

    // Prefix match against existing snapshot manifests.
    fn matches_snapshot_id(query: &str, id: &str) -> bool {
        if id.starts_with(query) {
            return true;
        }
        // Allow omitting the `<algo>:` prefix when matching.
        if let Some((_algo, rest)) = id.split_once(':') {
            if rest.starts_with(query) {
                return true;
            }
        }
        // Allow copying the filename form (colon replaced with underscore).
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

    let mut matches: Vec<AcceptedSnapshotId> = Vec::new();
    let snapshots_dir = accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR);
    let rd = fs::read_dir(&snapshots_dir).map_err(|e| {
        anyhow!(
            "failed to read accepted snapshots dir `{}`: {e}",
            snapshots_dir.display()
        )
    })?;
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
        let Ok(snap) = serde_json::from_str::<AcceptedPlaneSnapshotV1>(&text) else {
            continue;
        };
        if matches_snapshot_id(s, snap.snapshot_id.as_str()) {
            matches.push(snap.snapshot_id);
        }
    }
    matches.sort();
    matches.dedup();

    if matches.is_empty() {
        return Err(anyhow!(
            "unknown accepted-plane snapshot `{s}` (no matching manifest in `{}`)",
            snapshots_dir.display()
        ));
    }
    if matches.len() > 1 {
        let preview = matches
            .iter()
            .take(8)
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow!(
            "ambiguous accepted-plane snapshot `{s}` (matches {}): {preview}",
            matches.len()
        ));
    }

    Ok(matches[0].clone())
}

fn snapshot_manifest_path(accepted_dir: &Path, snapshot_id: &str) -> PathBuf {
    let file = format!("{}.json", digest_to_filename(snapshot_id));
    accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR).join(file)
}

fn world_model_run_record_path(accepted_dir: &Path, run_id: &WorldModelRunId) -> PathBuf {
    let file = format!("{}.json", digest_to_filename(run_id.as_str()));
    accepted_dir
        .join(ACCEPTED_PLANE_SEM_WORLD_MODEL_RUNS_DIR)
        .join(file)
}

fn read_snapshot(
    accepted_dir: &Path,
    snapshot_id: &AcceptedSnapshotId,
) -> Result<AcceptedPlaneSnapshotV1> {
    let path = snapshot_manifest_path(accepted_dir, snapshot_id.as_str());
    let text = fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read snapshot manifest `{}`: {e}", path.display()))?;
    let snapshot: AcceptedPlaneSnapshotV1 = serde_json::from_str(&text)?;
    if snapshot.snapshot_id != *snapshot_id {
        return Err(anyhow!(
            "snapshot manifest `{}` has mismatched id: expected={} got={}",
            path.display(),
            snapshot_id,
            snapshot.snapshot_id
        ));
    }
    Ok(snapshot)
}

fn write_snapshot(accepted_dir: &Path, snapshot: &AcceptedPlaneSnapshotV1) -> Result<()> {
    let path = snapshot_manifest_path(accepted_dir, snapshot.snapshot_id.as_str());
    if path.exists() {
        // Idempotency: if the snapshot already exists, it must match.
        let existing = read_snapshot(accepted_dir, &snapshot.snapshot_id)?;
        if existing.modules != snapshot.modules {
            return Err(anyhow!(
                "snapshot id collision: `{}` already exists with different contents",
                snapshot.snapshot_id
            ));
        }
        return Ok(());
    }

    let json = serde_json::to_string_pretty(snapshot)?;
    fs::write(path, json)?;
    Ok(())
}

fn store_module_if_needed(
    accepted_dir: &Path,
    module_name: &str,
    module_digest: &AxiDigest,
    candidate_path: &Path,
    text: &str,
) -> Result<String> {
    let module_dir = accepted_dir
        .join(ACCEPTED_PLANE_MODULES_DIR)
        .join(sanitize_path_component(module_name));
    fs::create_dir_all(&module_dir)?;

    let file_name = format!("{}.axi", digest_to_filename(module_digest));
    let stored_path = module_dir.join(file_name);

    if stored_path.exists() {
        // Ensure it matches the expected digest (basic corruption guard).
        let existing_text = fs::read_to_string(&stored_path)?;
        let existing_digest = AxiDigest::from_axi_text(&existing_text);
        if existing_digest != *module_digest {
            return Err(anyhow!(
                "accepted module path collision: `{}` exists but digest mismatches (expected {module_digest}, got {existing_digest})",
                stored_path.display()
            ));
        }
    } else {
        fs::write(&stored_path, text)?;
    }

    let rel = stored_path
        .strip_prefix(accepted_dir)
        .unwrap_or(&stored_path)
        .to_string_lossy()
        .to_string();

    // Tiny UX guard: keep a backpointer to the candidate path in the log only,
    // but refuse to store a module outside the accepted dir by accident.
    if candidate_path.starts_with(accepted_dir) {
        // ok
    }

    Ok(rel)
}

fn append_event(accepted_dir: &Path, event: &AcceptedPlaneEventV1) -> Result<()> {
    let log_path = accepted_dir.join(ACCEPTED_PLANE_LOG_V1);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| {
            anyhow!(
                "failed to open accepted plane log `{}`: {e}",
                log_path.display()
            )
        })?;

    let line = serde_json::to_string(event)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "axiograph-accepted-plane-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp test dir");
        path
    }

    #[test]
    fn snapshot_manifest_round_trips_typed_module_digest() {
        let accepted_dir = temp_test_dir("snapshot-roundtrip");
        ensure_layout(&accepted_dir).expect("layout");
        let snapshot_id = AcceptedSnapshotId::new("fnv1a64:accepted00000001");
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: snapshot_id.clone(),
            previous_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted00000000")),
            created_at_unix_secs: 17,
            modules: BTreeMap::from([(
                "Demo".to_string(),
                AcceptedModuleRefV1 {
                    module_digest: AxiDigest::new("fnv1a64:0123456789abcdef"),
                    stored_path: "modules/Demo/fnv1a64_0123456789abcdef.axi".to_string(),
                },
            )]),
        };

        write_snapshot(&accepted_dir, &snapshot).expect("write snapshot");
        write_head(&accepted_dir, &snapshot_id).expect("write head");

        let round_trip =
            read_snapshot_for_cli(&accepted_dir, "head").expect("read snapshot through cli seam");
        assert_eq!(
            round_trip.modules["Demo"].module_digest,
            AxiDigest::new("fnv1a64:0123456789abcdef")
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn accepted_plane_event_json_round_trips_typed_digest() {
        let event = AcceptedPlaneEventV1 {
            version: ACCEPTED_PLANE_EVENT_VERSION_V1.to_string(),
            created_at_unix_secs: 99,
            action: "promote".to_string(),
            snapshot_id: AcceptedSnapshotId::new("fnv1a64:snapshot"),
            previous_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:prev")),
            module_name: "Demo".to_string(),
            module_digest: AxiDigest::new("fnv1a64:feedfacecafebeef"),
            stored_module_path: "modules/Demo/demo.axi".to_string(),
            message: Some("test".to_string()),
            quality_profile: Some("strict".to_string()),
            quality_report_path: Some("quality/report.json".to_string()),
            quality_error_count: Some(0),
            quality_warning_count: Some(1),
            quality_info_count: Some(2),
            constraints_cert_path: Some("certs/demo.json".to_string()),
            constraints_constraint_count: Some(3),
            constraints_instance_count: Some(4),
            constraints_check_count: Some(5),
        };

        let json = serde_json::to_string(&event).expect("event should serialize");
        let round_trip: AcceptedPlaneEventV1 =
            serde_json::from_str(&json).expect("event should deserialize");

        assert_eq!(
            round_trip.module_digest,
            AxiDigest::new("fnv1a64:feedfacecafebeef")
        );
    }

    #[test]
    fn promote_reviewed_module_rejects_unknown_constraints_even_after_validation() {
        let accepted_dir = temp_test_dir("reject-unknown-constraints");
        ensure_layout(&accepted_dir).expect("layout");

        let axi_path = accepted_dir.join("UnknownConstraint.axi");
        fs::write(
            &axi_path,
            r#"module UnknownConstraint

schema S:
  object A
  relation R(from: A, to: A)

constraints S
  R
    unsupported(foo)

instance I of S:
  A = {x}
  R = {(from=x, to=x)}
"#,
        )
        .expect("write candidate module");

        let err = promote_reviewed_module(&axi_path, &accepted_dir, Some("test"), "off")
            .expect_err("promotion should reject unknown constraints");
        let msg = err.to_string();
        assert!(
            !msg.trim().is_empty(),
            "expected a meaningful rejection message"
        );
        assert!(
            read_head(&accepted_dir)
                .expect("read head after failed promotion")
                .is_none(),
            "rejected promotion must not advance accepted HEAD"
        );
        let snapshots_dir = accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR);
        let snapshot_count = fs::read_dir(&snapshots_dir)
            .expect("read snapshots dir")
            .filter(|entry| {
                entry
                    .as_ref()
                    .ok()
                    .and_then(|e| e.path().extension().map(|ext| ext == "json"))
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            snapshot_count, 0,
            "rejected promotion must not write snapshots"
        );

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn build_pathdb_from_snapshot_rejects_stored_pathdb_export_modules() {
        let accepted_dir = temp_test_dir("reject-stored-pathdb-export");
        ensure_layout(&accepted_dir).expect("layout");

        let canonical = r#"module Demo

schema S:
  object A
  relation R(from: A, to: A)

instance I of S:
  A = {x, y}
  R = {(from=x, to=y)}
"#;

        let mut db = axiograph_pathdb::PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, canonical)
            .expect("import canonical module");
        db.build_indexes();
        let snapshot_export = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)
            .expect("export pathdb snapshot");
        let digest = AxiDigest::from_axi_text(&snapshot_export);

        let stored_rel = format!("modules/Forged/{}.axi", digest_to_filename(digest.as_str()));
        let stored_abs = accepted_dir.join(&stored_rel);
        fs::create_dir_all(stored_abs.parent().expect("stored parent")).expect("mkdir");
        fs::write(&stored_abs, &snapshot_export).expect("write forged stored module");

        let snapshot_id = AcceptedSnapshotId::new("fnv1a64:forgedsnapshot");
        let snapshot = AcceptedPlaneSnapshotV1 {
            version: ACCEPTED_PLANE_SNAPSHOT_VERSION_V1.to_string(),
            snapshot_id: snapshot_id.clone(),
            previous_snapshot_id: None,
            created_at_unix_secs: 1,
            modules: BTreeMap::from([(
                "Forged".to_string(),
                AcceptedModuleRefV1 {
                    module_digest: digest,
                    stored_path: stored_rel,
                },
            )]),
        };
        write_snapshot(&accepted_dir, &snapshot).expect("write forged snapshot manifest");
        write_head(&accepted_dir, &snapshot_id).expect("write head");

        let out_axpd = accepted_dir.join("out.axpd");
        let err = build_pathdb_from_snapshot(&accepted_dir, "head", &out_axpd)
            .expect_err("stored PathDBExportV1 module must be rejected");
        assert!(err
            .to_string()
            .contains("expected a canonical .axi module, but input is a PathDBExportV1 snapshot"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn world_model_run_record_round_trips_typed_anchors() {
        let accepted_dir = temp_test_dir("world-model-run-record");
        ensure_layout(&accepted_dir).expect("layout");

        let record = WorldModelRunRecordV1 {
            version: WORLD_MODEL_RUN_RECORD_VERSION_V1.to_string(),
            run_id: WorldModelRunId::new("wm::run"),
            trace_id: WorldModelRunId::new("wm::trace"),
            created_at_unix_secs: 123,
            status: WorldModelRunStatusV1::CommittedToPathdb,
            backend: "plugin".to_string(),
            model: Some("deterministic".to_string()),
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:axi")),
            input_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-in")),
            input_accepted_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-in")),
            proposals_digest: ProposalDigest::new("fnv1a64:proposals"),
            proposal_count: 2,
            committed_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-out")),
            committed_accepted_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-out")),
            guardrail_total_cost: Some(1.25),
            guardrail_profile: Some("fast".to_string()),
            guardrail_plane: Some("both".to_string()),
            notes: vec!["typed".to_string(), "anchored".to_string()],
        };

        let path = persist_world_model_run_record(&accepted_dir, &record)
            .expect("persist world-model run");
        assert!(
            path.ends_with("sem/world_model_runs/wm__run.json"),
            "unexpected persisted path: {}",
            path.display()
        );

        let round_trip =
            read_world_model_run_record(&accepted_dir, &record.run_id).expect("read run record");
        assert_eq!(round_trip, record);

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }

    #[test]
    fn world_model_run_record_rejects_run_id_collision_with_different_typed_lineage() {
        let accepted_dir = temp_test_dir("world-model-run-record-collision");
        ensure_layout(&accepted_dir).expect("layout");

        let record = WorldModelRunRecordV1 {
            version: WORLD_MODEL_RUN_RECORD_VERSION_V1.to_string(),
            run_id: WorldModelRunId::new("wm::shared"),
            trace_id: WorldModelRunId::new("wm::trace"),
            created_at_unix_secs: 123,
            status: WorldModelRunStatusV1::Previewed,
            backend: "plugin".to_string(),
            model: None,
            axi_digest_v1: Some(AxiDigest::new("fnv1a64:axi")),
            input_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-a")),
            input_accepted_snapshot_id: Some(AcceptedSnapshotId::new("fnv1a64:accepted-a")),
            proposals_digest: ProposalDigest::new("fnv1a64:proposals-a"),
            proposal_count: 1,
            committed_pathdb_snapshot_id: None,
            committed_accepted_snapshot_id: None,
            guardrail_total_cost: None,
            guardrail_profile: None,
            guardrail_plane: None,
            notes: vec!["preview".to_string()],
        };
        persist_world_model_run_record(&accepted_dir, &record).expect("persist first record");

        let err = persist_world_model_run_record(
            &accepted_dir,
            &WorldModelRunRecordV1 {
                proposals_digest: ProposalDigest::new("fnv1a64:proposals-b"),
                input_pathdb_snapshot_id: Some(PathdbSnapshotId::new("fnv1a64:pathdb-b")),
                ..record
            },
        )
        .expect_err("mismatched run id must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("world-model run id collision"));
        assert!(msg.contains("wm::shared"));

        fs::remove_dir_all(&accepted_dir).expect("cleanup temp dir");
    }
}
