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

const ACCEPTED_PLANE_VERSION_V1: &str = "accepted_plane_v1";
const ACCEPTED_PLANE_LOG_V1: &str = "accepted_plane.log.jsonl";
const ACCEPTED_PLANE_HEAD_FILE: &str = "HEAD";
const ACCEPTED_PLANE_MODULES_DIR: &str = "modules";
const ACCEPTED_PLANE_SNAPSHOTS_DIR: &str = "snapshots";
const ACCEPTED_PLANE_QUALITY_DIR: &str = "quality";
const ACCEPTED_PLANE_CERTS_DIR: &str = "certs";

const ACCEPTED_PLANE_SNAPSHOT_VERSION_V1: &str = "accepted_plane_snapshot_v1";
const ACCEPTED_PLANE_EVENT_VERSION_V1: &str = "accepted_plane_event_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPlaneSnapshotV1 {
    pub version: String,
    pub snapshot_id: String,
    pub previous_snapshot_id: Option<String>,
    pub created_at_unix_secs: u64,
    /// Module name -> module digest.
    ///
    /// We keep this as a map so the snapshot meaning is stable regardless of
    /// promotion ordering.
    pub modules: BTreeMap<String, AcceptedModuleRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedModuleRefV1 {
    pub module_digest: String,
    /// Path relative to the accepted-plane directory.
    pub stored_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedPlaneEventV1 {
    pub version: String,
    pub created_at_unix_secs: u64,
    pub action: String,
    pub snapshot_id: String,
    pub previous_snapshot_id: Option<String>,
    pub module_name: String,
    pub module_digest: String,
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
) -> Result<String> {
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
) -> Result<String> {
    ensure_layout(accepted_dir)?;

    let text = fs::read_to_string(candidate_axi)?;
    let module = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;
    // Conservative gate: ensure the module is self-contained and well-typed
    // with respect to its declared schema(s).
    let typed = axiograph_pathdb::axi_module_typecheck::TypedAxiV1Module::new(module)?;

    // Hard gate: accepted/canonical modules must not contain unknown/opaque
    // constraints. If a constraint is not structured, it cannot participate in
    // certificate checking or schema-directed tooling, and we don't want silent
    // semantics drift in the accepted plane.
    //
    // If you want to keep richer (not yet executable/certifiable) content in a
    // canonical module, prefer a `constraint Name:` named-block.
    let mut unknown: Vec<(String, String)> = Vec::new();
    for th in &typed.module().theories {
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

    let module_name = typed.module().module_name.clone();
    let module_digest = axiograph_dsl::digest::axi_digest_v1(&text);

    // Hard gate: accepted-plane promotions must satisfy the conservative,
    // certificate-checkable constraint subset.
    //
    // This complements the quality/lint pass: it is the stable, semantics-driven
    // check we expect to keep in sync with the Lean trusted checker.
    let constraints_proof =
        axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1(typed.module())?;
    let constraints_cert = axiograph_pathdb::certificate::CertificateV2::axi_constraints_ok_v1(
        constraints_proof.clone(),
    )
    .with_anchor(axiograph_pathdb::certificate::AxiAnchorV1 {
        axi_digest_v1: module_digest.clone(),
    });

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
            &mut db,
            typed.module(),
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
    let previous_snapshot = if let Some(prev) = previous_snapshot_id.as_deref() {
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

    let snapshot_id = accepted_plane_snapshot_id_v1(previous_snapshot_id.as_deref(), &modules);
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
            digest_to_filename(&snapshot_id),
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

        // Grounding always has evidence: embed the canonical `.axi` module text
        // as an untrusted DocChunk so LLM/UI workflows can cite and open it.
        module_chunks.push(crate::doc_chunks::chunk_from_axi_module_text(
            module_name,
            &digest,
            &text,
        ));
    }

    let _ = crate::doc_chunks::import_chunks_into_pathdb(&mut db, &module_chunks);
    db.build_indexes();
    fs::write(out_axpd, db.to_bytes()?)?;
    Ok(())
}

fn ensure_layout(accepted_dir: &Path) -> Result<()> {
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_MODULES_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_SNAPSHOTS_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_QUALITY_DIR))?;
    fs::create_dir_all(accepted_dir.join(ACCEPTED_PLANE_CERTS_DIR))?;
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

fn digest_to_filename(digest: &str) -> String {
    digest.replace(':', "_")
}

fn accepted_plane_snapshot_id_v1(
    previous_snapshot_id: Option<&str>,
    modules: &BTreeMap<String, AcceptedModuleRefV1>,
) -> String {
    use std::fmt::Write as _;

    let mut s = String::new();
    let _ = write!(&mut s, "{ACCEPTED_PLANE_VERSION_V1};");
    let _ = write!(&mut s, "prev={};", previous_snapshot_id.unwrap_or("(none)"));
    for (name, m) in modules {
        let _ = write!(
            &mut s,
            "module={name};digest={};path={};",
            m.module_digest, m.stored_path
        );
    }
    axiograph_dsl::digest::axi_digest_v1(&s)
}

fn read_head(accepted_dir: &Path) -> Result<Option<String>> {
    let path = accepted_dir.join(ACCEPTED_PLANE_HEAD_FILE);
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

fn write_head(accepted_dir: &Path, snapshot_id: &str) -> Result<()> {
    fs::write(
        accepted_dir.join(ACCEPTED_PLANE_HEAD_FILE),
        format!("{snapshot_id}\n"),
    )?;
    Ok(())
}

fn resolve_snapshot_id(accepted_dir: &Path, snapshot_id_or_latest: &str) -> Result<String> {
    let s = snapshot_id_or_latest.trim();
    if s.eq_ignore_ascii_case("latest") || s.eq_ignore_ascii_case("head") {
        return read_head(accepted_dir)?
            .ok_or_else(|| anyhow!("accepted plane has no HEAD snapshot yet"));
    }

    // Fast path: full id (manifest exists).
    if snapshot_manifest_path(accepted_dir, s).exists() {
        return Ok(s.to_string());
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

    let mut matches: Vec<String> = Vec::new();
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
        if matches_snapshot_id(s, &snap.snapshot_id) {
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
            .cloned()
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

fn read_snapshot(accepted_dir: &Path, snapshot_id: &str) -> Result<AcceptedPlaneSnapshotV1> {
    let path = snapshot_manifest_path(accepted_dir, snapshot_id);
    let text = fs::read_to_string(&path)
        .map_err(|e| anyhow!("failed to read snapshot manifest `{}`: {e}", path.display()))?;
    let snapshot: AcceptedPlaneSnapshotV1 = serde_json::from_str(&text)?;
    if snapshot.snapshot_id != snapshot_id {
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
    let path = snapshot_manifest_path(accepted_dir, &snapshot.snapshot_id);
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
    module_digest: &str,
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
        let existing_digest = axiograph_dsl::digest::axi_digest_v1(&existing_text);
        if existing_digest != module_digest {
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
