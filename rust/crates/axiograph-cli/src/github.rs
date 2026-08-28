//! GitHub/repository import helpers (untrusted boundary tooling).
//!
//! Goal: provide a single entrypoint to ingest a repo’s:
//! - code/document structure (repo chunks + repo edges),
//! - protobuf/gRPC APIs (Buf descriptor sets → proto proposals),
//!   and merge them into one `proposals.json` + typed `EvidenceChunkBundleV1` bundle.
//!
//! Network access is optional:
//! - If the `repo` argument is a local path, this command is fully offline.
//! - If it is a GitHub URL / `owner/name`, we shell out to `git clone`.

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Subcommand)]
pub enum GithubCommands {
    /// Import a GitHub repo (or local repo path) into merged `proposals.json` + typed chunk evidence.
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

        /// Optional branch or tag to checkout. Remote imports are always depth-one clones.
        #[arg(long)]
        r#ref: Option<String>,

        /// Skip repo indexing (chunks + lightweight code graph).
        #[arg(long)]
        no_repo_index: bool,

        /// Skip protobuf ingestion.
        #[arg(long)]
        no_proto: bool,

        /// Path to an existing binary Buf descriptor set (skip `buf build`).
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
        #[arg(long, default_value_t = 10_000)]
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

#[allow(clippy::too_many_arguments)]
fn cmd_github_import(
    repo: &str,
    out_dir: &PathBuf,
    clone_dir: Option<&PathBuf>,
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

    let repo_path = prepare_repo_checkout(repo, out_dir, clone_dir, git_ref)?;
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
        axiograph_ingest_docs::validate_proposals_file_v1(&proposals_file)?;

        crate::security::write_output_bounded(
            &repo_chunks_path,
            axiograph_ingest_docs::chunks_to_json_for_chunks(
                "github_repo_index",
                repo_path.display().to_string(),
                chunks.clone(),
            )?,
            "CLI output",
        )?;
        crate::security::write_output_bounded(
            &repo_edges_path,
            serde_json::to_string_pretty(&edges)?,
            "CLI output",
        )?;
        crate::security::write_output_bounded(
            &repo_proposals_path,
            serde_json::to_string_pretty(&proposals_file)?,
            "CLI output",
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
        axiograph_ingest_docs::validate_proposals_file_v1(&proposals_file)?;

        crate::security::write_output_bounded(
            &proto_chunks_path,
            axiograph_ingest_docs::chunks_to_json_for_chunks(
                "github_proto_ingest",
                repo_path.display().to_string(),
                chunks.clone(),
            )?,
            "CLI output",
        )?;
        crate::security::write_output_bounded(
            &proto_proposals_path,
            serde_json::to_string_pretty(&proposals_file)?,
            "CLI output",
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

    crate::security::write_output_bounded(
        &merged_chunks_path,
        axiograph_ingest_docs::chunks_to_json_for_chunks(
            "github_import",
            repo.to_string(),
            merged_chunks.clone(),
        )?,
        "CLI output",
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
    axiograph_ingest_docs::validate_proposals_file_v1(&merged_file)?;
    crate::security::write_output_bounded(
        &merged_proposals_path,
        serde_json::to_string_pretty(&merged_file)?,
        "CLI output",
    )?;

    println!("  {} {}", "→".cyan(), merged_chunks_path.display());
    println!("  {} {}", "→".cyan(), merged_proposals_path.display());

    Ok(())
}

fn prepare_repo_checkout(
    repo: &str,
    out_dir: &Path,
    clone_dir: Option<&PathBuf>,
    git_ref: Option<&str>,
) -> Result<PathBuf> {
    // Local path mode (fully offline).
    let as_path = PathBuf::from(repo);
    if let Ok(metadata) = fs::symlink_metadata(&as_path) {
        if metadata.file_type().is_symlink() {
            return Err(anyhow!("local repository path must not be a symlink"));
        }
        if metadata.file_type().is_dir() {
            if git_ref.is_some() {
                return Err(anyhow!(
                    "local repository mode does not accept --ref; provide the exact checked-out directory"
                ));
            }
            return Ok(as_path);
        }
    }

    let clone_dir = clone_dir.cloned().unwrap_or_else(|| out_dir.join("repo"));
    if let Some(git_ref) = git_ref {
        validate_git_ref(git_ref)?;
    }

    match fs::symlink_metadata(&clone_dir) {
        Ok(_) => {
            return Err(anyhow!(
                "clone destination `{}` already exists; GitHub import requires a fresh directory so repository-local config cannot execute during checkout",
                clone_dir.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let url = normalize_github_repo_spec(repo)?;
    let parsed = url::Url::parse(&url)?;
    let (_, addresses) = crate::web::resolve_public_addresses(&parsed, Duration::from_secs(5))?;
    let mut ips = addresses
        .iter()
        .map(|address| address.ip())
        .collect::<Vec<_>>();
    ips.sort();
    ips.dedup();
    let pinned = ips
        .into_iter()
        .map(|ip| match ip {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("[{ip}]"),
        })
        .collect::<Vec<_>>()
        .join(",");

    let parent = clone_dir.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(anyhow!("clone parent must be a real directory"));
    }

    fs::create_dir(&clone_dir)
        .with_context(|| format!("reserve empty clone destination `{}`", clone_dir.display()))?;
    let clone_metadata = fs::symlink_metadata(&clone_dir)?;
    if clone_metadata.file_type().is_symlink() || !clone_metadata.file_type().is_dir() {
        return Err(anyhow!("clone destination must be a real directory"));
    }

    let mut cmd = Command::new("git");
    harden_git_command(&mut cmd);
    cmd.arg("-c")
        .arg("protocol.allow=never")
        .arg("-c")
        .arg("protocol.https.allow=always")
        .arg("-c")
        .arg("http.proxy=")
        .arg("-c")
        .arg("http.followRedirects=false")
        .arg("-c")
        .arg("credential.helper=")
        .arg("-c")
        .arg(format!("http.curloptResolve=github.com:443:{pinned}"))
        .arg("clone")
        .arg("--no-checkout")
        .arg("--no-recurse-submodules")
        .arg("--depth")
        .arg("1")
        .arg("--single-branch");
    if let Some(git_ref) = git_ref {
        cmd.arg("--branch").arg(git_ref);
    }
    cmd.arg(&url).arg(&clone_dir);

    let limits = crate::security::ProcessLimits::plugin(std::time::Duration::from_secs(600))?;
    let out =
        crate::security::run_command_bounded(cmd, b"", limits, &format!("git clone for {url}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "git clone failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    checkout_repository(&clone_dir, git_ref.unwrap_or("HEAD"))?;

    let clone_metadata = fs::symlink_metadata(&clone_dir)?;
    if clone_metadata.file_type().is_symlink() || !clone_metadata.file_type().is_dir() {
        return Err(anyhow!("clone destination changed during checkout"));
    }
    let git_metadata = fs::symlink_metadata(clone_dir.join(".git"))?;
    if git_metadata.file_type().is_symlink() || !git_metadata.file_type().is_dir() {
        return Err(anyhow!("cloned .git entry must be a real directory"));
    }
    Ok(clone_dir)
}

fn checkout_repository(clone_dir: &Path, git_ref: &str) -> Result<()> {
    validate_git_ref(git_ref)?;
    let parent = clone_dir.parent().unwrap_or(Path::new("."));
    let hooks = tempfile::Builder::new()
        .prefix(".axiograph-empty-hooks-")
        .tempdir_in(parent)?;
    let mut checkout = Command::new("git");
    harden_git_command(&mut checkout);
    checkout
        .arg("-c")
        .arg(format!("core.hooksPath={}", hooks.path().display()))
        .arg("-c")
        .arg("protocol.allow=never")
        .arg("-C")
        .arg(clone_dir)
        .arg("checkout")
        .arg("--detach")
        .arg(git_ref);
    let limits = crate::security::ProcessLimits::plugin(Duration::from_secs(600))?;
    let output = crate::security::run_command_bounded(
        checkout,
        b"",
        limits,
        &format!("git checkout {git_ref}"),
    )?;
    if !output.status.success() {
        return Err(anyhow!(
            "git checkout failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn harden_git_command(command: &mut Command) {
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
    ] {
        command.env_remove(name);
    }
    command
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_EXEC_PATH")
        .env_remove("GIT_TEMPLATE_DIR")
        .env_remove("GIT_SSH")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_ASKPASS")
        .env_remove("SSH_ASKPASS")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_SSL_NO_VERIFY")
        .env_remove("GIT_SSL_CAINFO")
        .env_remove("GIT_SSL_CAPATH")
        .env_remove("GIT_PROXY_COMMAND")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "https")
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        );
}

fn normalize_github_repo_spec(repo: &str) -> Result<String> {
    let input = repo.trim();
    let candidate = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://github.com/{input}")
    };
    let parsed = url::Url::parse(&candidate)
        .with_context(|| format!("invalid GitHub repository URL `{input}`"))?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || parsed.port_or_known_default() != Some(443)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(anyhow!(
            "GitHub imports require an exact credential-free https://github.com/owner/repository URL"
        ));
    }
    let parts = parsed
        .path_segments()
        .ok_or_else(|| anyhow!("GitHub repository URL has no path"))?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(anyhow!(
            "GitHub repository path must be exactly owner/repository"
        ));
    }
    let owner = parts[0];
    let repository = parts[1].strip_suffix(".git").unwrap_or(parts[1]);
    for (label, value) in [("owner", owner), ("repository", repository)] {
        if value.is_empty()
            || value.len() > 100
            || matches!(value, "." | "..")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err(anyhow!("GitHub {label} is not canonical ASCII"));
        }
    }
    let canonical_path = format!("/{owner}/{repository}");
    if parsed.path() != canonical_path && parsed.path() != format!("{canonical_path}.git") {
        return Err(anyhow!("GitHub repository URL path is not canonical"));
    }
    Ok(format!("https://github.com/{owner}/{repository}.git"))
}

fn validate_git_ref(git_ref: &str) -> Result<()> {
    if git_ref.is_empty()
        || git_ref.len() > 256
        || git_ref.starts_with('-')
        || git_ref.starts_with('/')
        || git_ref.ends_with('/')
        || git_ref.contains("..")
        || git_ref.contains("@{")
        || git_ref.contains("//")
        || git_ref
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
        || !git_ref
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        return Err(anyhow!("Git branch/tag is not a canonical option-safe ref"));
    }
    Ok(())
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
    let (descriptor_bytes, descriptor_locator) = if let Some(p) = descriptor {
        let descriptor_path = resolve_maybe_relative(repo_root, p);
        (
            crate::security::read_file_bounded(
                &descriptor_path,
                crate::security::MAX_BINARY_INPUT_BYTES,
                "protobuf descriptor set",
            )?,
            descriptor_path.display().to_string(),
        )
    } else {
        // Default: treat repo root as a buf module if it contains a regular
        // no-follow `buf.yaml`. Buf output is staged outside the untrusted repo.
        let root = buf_root.map(|p| resolve_maybe_relative(repo_root, p));
        let buf_root = root.unwrap_or_else(|| repo_root.to_path_buf());
        let config = buf_root.join("buf.yaml");
        let config_metadata = fs::symlink_metadata(&config).with_context(|| {
            "proto ingest: missing regular buf.yaml (pass --proto-descriptor or --buf-root)"
        })?;
        if config_metadata.file_type().is_symlink() || !config_metadata.file_type().is_file() {
            return Err(anyhow!("proto ingest: buf.yaml must be a regular file"));
        }
        let staging = tempfile::tempdir()?;
        let out = staging
            .path()
            .join("axiograph_github_import_descriptor.binpb");
        (
            crate::proto::build_descriptor_set_binpb(&buf_root, &out, false, false)?,
            format!("buf:{}", buf_root.display()),
        )
    };

    let ingest = axiograph_ingest_proto::ingest_descriptor_set_bytes(
        &descriptor_bytes,
        Some(descriptor_locator.clone()),
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
            locator: descriptor_locator,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_repo_spec_is_exact_and_credential_free() {
        assert_eq!(
            normalize_github_repo_spec("axiograph/example").unwrap(),
            "https://github.com/axiograph/example.git"
        );
        assert_eq!(
            normalize_github_repo_spec("https://github.com/axiograph/example.git").unwrap(),
            "https://github.com/axiograph/example.git"
        );
        for invalid in [
            "http://github.com/axiograph/example",
            "https://user@github.com/axiograph/example",
            "https://github.com/axiograph/example/",
            "https://github.com/axiograph//example",
            "https://github.com/axiograph/example?ref=main",
            "https://evil.example/axiograph/example",
            "axiograph/example/extra",
        ] {
            assert!(
                normalize_github_repo_spec(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn git_ref_rejects_option_and_revision_expression_injection() {
        for valid in ["main", "release/v1.2.3", "v2.0.0"] {
            assert!(validate_git_ref(valid).is_ok(), "rejected {valid}");
        }
        for invalid in [
            "--orphan",
            "main^{tree}",
            "main~1",
            "main@{1}",
            "refs//heads/main",
            "../main",
            "main:evil",
            "main\n--help",
        ] {
            assert!(validate_git_ref(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn existing_clone_and_local_ref_modes_are_rejected_without_network() {
        let temp = tempfile::tempdir().unwrap();
        let clone_dir = temp.path().join("repo");
        fs::create_dir(&clone_dir).unwrap();
        let error = prepare_repo_checkout("axiograph/example", temp.path(), Some(&clone_dir), None)
            .expect_err("existing clone directory must never be reused");
        assert!(error.to_string().contains("requires a fresh directory"));

        let local = temp.path().join("local");
        fs::create_dir(&local).unwrap();
        let error = prepare_repo_checkout(local.to_str().unwrap(), temp.path(), None, Some("main"))
            .expect_err("local mode must not silently ignore a requested ref");
        assert!(error.to_string().contains("does not accept --ref"));
    }
}
