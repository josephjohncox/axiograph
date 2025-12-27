//! GitHub/repository import helpers (untrusted boundary tooling).
//!
//! Goal: provide a single entrypoint to ingest a repo’s:
//! - code/document structure (repo chunks + repo edges),
//! - protobuf/gRPC APIs (Buf descriptor sets → proto proposals),
//! and merge them into one `proposals.json` + `chunks.json` bundle.
//!
//! Network access is optional:
//! - If the `repo` argument is a local path, this command is fully offline.
//! - If it is a GitHub URL / `owner/name`, we shell out to `git clone`.

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Subcommand)]
pub enum GithubCommands {
    /// Import a GitHub repo (or local repo path) into merged `proposals.json` + `chunks.json`.
    ///
    /// The `repo` argument can be:
    /// - a local directory path, or
    /// - `https://github.com/<owner>/<name>` (or `.git`), or
    /// - `<owner>/<name>` (expanded to `https://github.com/<owner>/<name>.git`).
    Import {
        /// Repo spec (path/URL/owner/name)
        repo: String,

        /// Output directory for artifacts
        #[arg(short, long, default_value = "build/github_import")]
        out_dir: PathBuf,

        /// Clone dir (defaults to `<out_dir>/repo`)
        #[arg(long)]
        clone_dir: Option<PathBuf>,

        /// Shallow clone (faster; may not support arbitrary refs).
        #[arg(long, default_value_t = true)]
        shallow: bool,

        /// Optional git ref to checkout (branch/tag/commit).
        #[arg(long)]
        r#ref: Option<String>,

        /// Skip repo indexing (chunks + lightweight code graph).
        #[arg(long)]
        no_repo_index: bool,

        /// Skip protobuf ingestion.
        #[arg(long)]
        no_proto: bool,

        /// Path to an existing Buf descriptor set JSON (skip `buf build`).
        ///
        /// If relative, it is resolved relative to the repo root.
        #[arg(long)]
        proto_descriptor: Option<PathBuf>,

        /// Buf module root (directory containing `buf.yaml`).
        ///
        /// If not set, we use the repo root if it contains `buf.yaml`.
        #[arg(long)]
        buf_root: Option<PathBuf>,

        /// Max file size to read during repo indexing (bytes).
        #[arg(long, default_value_t = 524_288)]
        max_file_bytes: u64,

        /// Max number of files to index during repo indexing.
        #[arg(long, default_value_t = 50_000)]
        max_files: usize,

        /// Lines per code chunk (non-markdown) during repo indexing.
        #[arg(long, default_value_t = 80)]
        lines_per_chunk: usize,
    },
}

pub fn cmd_github(command: GithubCommands) -> Result<()> {
    match command {
        GithubCommands::Import {
            repo,
            out_dir,
            clone_dir,
            shallow,
            r#ref,
            no_repo_index,
            no_proto,
            proto_descriptor,
            buf_root,
            max_file_bytes,
            max_files,
            lines_per_chunk,
        } => cmd_github_import(
            &repo,
            &out_dir,
            clone_dir.as_ref(),
            shallow,
            r#ref.as_deref(),
            !no_repo_index,
            !no_proto,
            proto_descriptor.as_ref(),
            buf_root.as_ref(),
            max_file_bytes,
            max_files,
            lines_per_chunk,
        ),
    }
}

fn cmd_github_import(
    repo: &str,
    out_dir: &PathBuf,
    clone_dir: Option<&PathBuf>,
    shallow: bool,
    git_ref: Option<&str>,
    do_repo_index: bool,
    do_proto: bool,
    proto_descriptor: Option<&PathBuf>,
    buf_root: Option<&PathBuf>,
    max_file_bytes: u64,
    max_files: usize,
    lines_per_chunk: usize,
) -> Result<()> {
    fs::create_dir_all(out_dir)?;

    let repo_path = prepare_repo_checkout(repo, out_dir, clone_dir, shallow, git_ref)?;
    println!(
        "{} {}",
        "GitHub import repo".green().bold(),
        repo_path.display()
    );

    let repo_chunks_path = out_dir.join("repo_chunks.json");
    let repo_edges_path = out_dir.join("repo_edges.json");
    let repo_proposals_path = out_dir.join("repo_proposals.json");

    let proto_chunks_path = out_dir.join("proto_chunks.json");
    let proto_proposals_path = out_dir.join("proto_proposals.json");

    let merged_chunks_path = out_dir.join("chunks.json");
    let merged_proposals_path = out_dir.join("proposals.json");

    let mut merged_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    let mut merged_proposals: Vec<axiograph_ingest_docs::ProposalV1> = Vec::new();

    if do_repo_index {
        let (chunks, edges, proposals_file) =
            index_repo_to_artifacts(&repo_path, max_file_bytes, max_files, lines_per_chunk)?;

        fs::write(&repo_chunks_path, serde_json::to_string_pretty(&chunks)?)?;
        fs::write(&repo_edges_path, serde_json::to_string_pretty(&edges)?)?;
        fs::write(
            &repo_proposals_path,
            serde_json::to_string_pretty(&proposals_file)?,
        )?;

        merged_chunks.extend(chunks);
        merged_proposals.extend(proposals_file.proposals);
        println!(
            "  {} repo_index: chunks={} edges={} proposals={}",
            "→".yellow(),
            merged_chunks.len(),
            edges.len(),
            merged_proposals.len()
        );
    }

    if do_proto {
        let (chunks, proposals_file) =
            ingest_proto_to_artifacts(&repo_path, proto_descriptor, buf_root)?;

        fs::write(&proto_chunks_path, serde_json::to_string_pretty(&chunks)?)?;
        fs::write(
            &proto_proposals_path,
            serde_json::to_string_pretty(&proposals_file)?,
        )?;

        merged_chunks.extend(chunks);
        merged_proposals.extend(proposals_file.proposals);
        println!(
            "  {} proto_ingest: total_chunks={} total_proposals={}",
            "→".yellow(),
            merged_chunks.len(),
            merged_proposals.len()
        );
    }

    // Dedup and write merged outputs.
    let merged_chunks = dedup_chunks_by_id(merged_chunks);
    let merged_proposals = dedup_proposals_by_id(merged_proposals);

    fs::write(
        &merged_chunks_path,
        serde_json::to_string_pretty(&merged_chunks)?,
    )?;

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let merged_file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "github_import".to_string(),
            locator: repo.to_string(),
        },
        schema_hint: Some("repo".to_string()),
        proposals: merged_proposals,
    };
    fs::write(
        &merged_proposals_path,
        serde_json::to_string_pretty(&merged_file)?,
    )?;

    println!("  {} {}", "→".cyan(), merged_chunks_path.display());
    println!("  {} {}", "→".cyan(), merged_proposals_path.display());

    Ok(())
}

fn prepare_repo_checkout(
    repo: &str,
    out_dir: &Path,
    clone_dir: Option<&PathBuf>,
    shallow: bool,
    git_ref: Option<&str>,
) -> Result<PathBuf> {
    // Local path mode (fully offline).
    let as_path = PathBuf::from(repo);
    if as_path.is_dir() {
        return Ok(as_path);
    }

    let clone_dir = clone_dir.cloned().unwrap_or_else(|| out_dir.join("repo"));

    if clone_dir.exists() {
        // If it already exists, assume it is a usable checkout.
        return Ok(clone_dir);
    }

    let url = normalize_github_repo_spec(repo)?;

    fs::create_dir_all(clone_dir.parent().unwrap_or(Path::new(".")))?;

    let mut cmd = Command::new("git");
    cmd.arg("clone");
    if shallow && git_ref.is_none() {
        cmd.arg("--depth").arg("1");
    }
    cmd.arg(&url).arg(&clone_dir);

    let out = cmd
        .output()
        .with_context(|| format!("failed to run `git clone` for {url}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "git clone failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    if let Some(r#ref) = git_ref {
        let mut checkout = Command::new("git");
        checkout
            .arg("-C")
            .arg(&clone_dir)
            .arg("checkout")
            .arg(r#ref);
        let out = checkout
            .output()
            .with_context(|| format!("failed to run `git checkout {ref}`"))?;
        if !out.status.success() {
            return Err(anyhow!(
                "git checkout failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }

    Ok(clone_dir)
}

fn normalize_github_repo_spec(repo: &str) -> Result<String> {
    let s = repo.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        if s.ends_with(".git") {
            return Ok(s.to_string());
        }
        return Ok(format!("{s}.git"));
    }
    // owner/name → https://github.com/owner/name.git
    if s.split('/').count() == 2 {
        return Ok(format!("https://github.com/{s}.git"));
    }
    Err(anyhow!(
        "unsupported repo spec `{s}` (expected a local dir, URL, or owner/name)"
    ))
}

fn index_repo_to_artifacts(
    repo_root: &Path,
    max_file_bytes: u64,
    max_files: usize,
    lines_per_chunk: usize,
) -> Result<(
    Vec<axiograph_ingest_docs::Chunk>,
    Vec<axiograph_ingest_docs::RepoEdgeV1>,
    axiograph_ingest_docs::ProposalsFileV1,
)> {
    let mut options = axiograph_ingest_docs::RepoIndexOptions {
        max_files,
        max_file_bytes,
        lines_per_chunk,
        ..Default::default()
    };

    // Ensure we include `.proto` sources in code chunking by default.
    if !options
        .include_extensions
        .iter()
        .any(|e| e.eq_ignore_ascii_case("proto"))
    {
        options.include_extensions.push("proto".to_string());
    }

    let result = axiograph_ingest_docs::index_repo(repo_root, &options)?;

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    let proposals = axiograph_ingest_docs::proposals_from_repo_edges_v1(
        &result.edges,
        Some("repo".to_string()),
    );

    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "repo".to_string(),
            locator: repo_root.to_string_lossy().to_string(),
        },
        schema_hint: Some("repo".to_string()),
        proposals,
    };

    Ok((result.extraction.chunks, result.edges, file))
}

fn ingest_proto_to_artifacts(
    repo_root: &Path,
    descriptor: Option<&PathBuf>,
    buf_root: Option<&PathBuf>,
) -> Result<(
    Vec<axiograph_ingest_docs::Chunk>,
    axiograph_ingest_docs::ProposalsFileV1,
)> {
    let descriptor_path = if let Some(p) = descriptor {
        resolve_maybe_relative(repo_root, p)
    } else {
        // Default: treat repo root as a buf module if it contains `buf.yaml`,
        // otherwise skip with a good error (caller can disable proto ingestion).
        let root = buf_root.map(|p| resolve_maybe_relative(repo_root, p));
        let buf_root = root.unwrap_or_else(|| repo_root.to_path_buf());
        if !buf_root.join("buf.yaml").is_file() {
            return Err(anyhow!(
                "proto ingest: missing descriptor and no buf.yaml found (pass --proto-descriptor or --buf-root)"
            ));
        }
        let out = repo_root.join("build/axiograph_github_import_descriptor.json");
        crate::proto::build_descriptor_set_json(&buf_root, &out, false, false)?;
        out
    };

    let descriptor_text = fs::read_to_string(&descriptor_path).with_context(|| {
        format!(
            "proto ingest: failed to read descriptor json: {}",
            descriptor_path.display()
        )
    })?;

    let ingest = axiograph_ingest_proto::ingest_descriptor_set_json(
        &descriptor_text,
        Some(descriptor_path.display().to_string()),
        Some("proto_api".to_string()),
    )?;

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "proto".to_string(),
            locator: descriptor_path.display().to_string(),
        },
        schema_hint: Some("proto_api".to_string()),
        proposals: ingest.proposals,
    };

    Ok((ingest.chunks, file))
}

fn resolve_maybe_relative(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    base.join(p)
}

fn dedup_proposals_by_id(
    proposals: Vec<axiograph_ingest_docs::ProposalV1>,
) -> Vec<axiograph_ingest_docs::ProposalV1> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(proposals.len());
    for p in proposals {
        let id = match &p {
            axiograph_ingest_docs::ProposalV1::Entity { meta, .. } => meta.proposal_id.clone(),
            axiograph_ingest_docs::ProposalV1::Relation { meta, .. } => meta.proposal_id.clone(),
        };
        if seen.insert(id) {
            out.push(p);
        }
    }
    out
}

fn dedup_chunks_by_id(
    chunks: Vec<axiograph_ingest_docs::Chunk>,
) -> Vec<axiograph_ingest_docs::Chunk> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(chunks.len());
    for c in chunks {
        if seen.insert(c.chunk_id.clone()) {
            out.push(c);
        }
    }
    out
}
