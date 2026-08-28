//! Protobuf / gRPC ingestion commands.
//!
//! Pipeline:
//! 1. Run `buf build --as-file-descriptor-set -o descriptor.binpb`
//! 2. Decode binary descriptor set → `proposals.json` (+ optional chunks)

use anyhow::{anyhow, Result};
use clap::Subcommand;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MAX_PROTO_SOURCE_ENTRIES: usize = 100_000;
const MAX_PROTO_SOURCE_FILES: usize = 10_000;
const MAX_PROTO_SOURCE_DEPTH: usize = 32;
const MAX_PROTO_SOURCE_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PROTO_SOURCE_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024 * 1024;

#[derive(Subcommand)]
pub enum ProtoCommands {
    /// Build a binary Buf descriptor set (`google.protobuf.FileDescriptorSet`).
    BuildDescriptor {
        /// Buf module root (directory containing `buf.yaml`).
        root: PathBuf,
        /// Output binary descriptor-set file (`*.binpb`).
        #[arg(short, long)]
        out: PathBuf,
        /// Exclude imports from the descriptor set.
        #[arg(long)]
        exclude_imports: bool,
        /// Exclude source info (comments + spans) from the descriptor set.
        #[arg(long)]
        exclude_source_info: bool,
    },

    /// Ingest a binary Buf descriptor set into `proposals.json` (+ optional chunks).
    Ingest {
        /// Buf module root (directory containing `buf.yaml`).
        root: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema).
        #[arg(short, long)]
        out: PathBuf,
        /// Optional output chunks JSON (for RAG).
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Optional path to an existing binary descriptor set (skip `buf build`).
        #[arg(long)]
        descriptor: Option<PathBuf>,
        /// If we build a descriptor set, also write it here.
        #[arg(long)]
        descriptor_out: Option<PathBuf>,
        /// Schema hint for downstream reconciliation (default: `proto_api`).
        #[arg(long, default_value = "proto_api")]
        schema_hint: String,
        /// Exclude imports from the descriptor set.
        #[arg(long)]
        exclude_imports: bool,
        /// Exclude source info (comments + spans) from the descriptor set.
        #[arg(long)]
        exclude_source_info: bool,
    },
}

pub fn cmd_proto(command: ProtoCommands) -> Result<()> {
    match command {
        ProtoCommands::BuildDescriptor {
            root,
            out,
            exclude_imports,
            exclude_source_info,
        } => {
            let _ = build_descriptor_set_binpb(&root, &out, exclude_imports, exclude_source_info)?;
            println!("  {} {}", "→".cyan(), out.display());
            Ok(())
        }
        ProtoCommands::Ingest {
            root,
            out,
            chunks,
            descriptor,
            descriptor_out,
            schema_hint,
            exclude_imports,
            exclude_source_info,
        } => cmd_proto_ingest(
            &root,
            &out,
            chunks.as_ref(),
            descriptor.as_ref(),
            descriptor_out.as_ref(),
            &schema_hint,
            exclude_imports,
            exclude_source_info,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_proto_ingest(
    root: &Path,
    out: &PathBuf,
    chunks_out: Option<&PathBuf>,
    descriptor_in: Option<&PathBuf>,
    descriptor_out: Option<&PathBuf>,
    schema_hint: &str,
    exclude_imports: bool,
    exclude_source_info: bool,
) -> Result<()> {
    println!(
        "{} {}",
        "Ingesting proto API".green().bold(),
        root.display()
    );

    let (descriptor_bytes, descriptor_locator) = if let Some(path) = descriptor_in {
        (
            crate::security::read_file_bounded(
                path,
                crate::security::MAX_BINARY_INPUT_BYTES,
                "protobuf descriptor set",
            )?,
            path.display().to_string(),
        )
    } else {
        let default_out = out
            .parent()
            .unwrap_or(Path::new("."))
            .join("descriptor.binpb");
        let out_path = descriptor_out.unwrap_or(&default_out);
        fs::create_dir_all(out_path.parent().unwrap_or(Path::new(".")))?;
        (
            build_descriptor_set_binpb(root, out_path, exclude_imports, exclude_source_info)?,
            out_path.display().to_string(),
        )
    };

    let ingest = axiograph_ingest_proto::ingest_descriptor_set_bytes(
        &descriptor_bytes,
        Some(descriptor_locator.clone()),
        Some(schema_hint.to_string()),
    )?;

    // Always emit chunks for RAG grounding (default: alongside the proposals output).
    let chunks_path = chunks_out
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_path.parent().unwrap_or(Path::new(".")))?;
    let json = axiograph_ingest_docs::chunks_to_json_for_chunks(
        "proto_descriptor",
        descriptor_locator,
        ingest.chunks.clone(),
    )?;
    crate::security::write_output_bounded(&chunks_path, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), chunks_path.display());

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let proposals_file = axiograph_ingest_docs::ProposalsFileV1 {
        version: axiograph_ingest_docs::PROPOSALS_VERSION_V1,
        generated_at,
        source: axiograph_ingest_docs::ProposalSourceV1 {
            source_type: "proto".to_string(),
            locator: root.to_string_lossy().to_string(),
        },
        schema_hint: Some(schema_hint.to_string()),
        proposals: ingest.proposals,
    };

    let json = serde_json::to_string_pretty(&proposals_file)?;
    fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))?;
    crate::security::write_output_bounded(out, &json, "CLI output")?;
    println!("  {} {}", "→".cyan(), out.display());

    println!(
        "  stats: files={} packages={} services={} rpcs={} messages={} fields={} enums={} chunks={}",
        ingest.stats.files,
        ingest.stats.packages,
        ingest.stats.services,
        ingest.stats.rpcs,
        ingest.stats.messages,
        ingest.stats.fields,
        ingest.stats.enums,
        ingest.stats.chunks
    );

    Ok(())
}

pub(crate) fn build_descriptor_set_binpb(
    root: &Path,
    out: &Path,
    exclude_imports: bool,
    exclude_source_info: bool,
) -> Result<Vec<u8>> {
    let root = validate_buf_source_tree(root)?;
    let parent = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let parent_metadata = fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.file_type().is_dir() {
        return Err(anyhow!(
            "protobuf descriptor output parent must be a real directory"
        ));
    }
    let staging = tempfile::Builder::new()
        .prefix(".axiograph-buf-output-")
        .tempdir_in(parent)?;
    let staged_output = staging.path().join("descriptor.binpb");
    let cache = tempfile::Builder::new()
        .prefix(".axiograph-buf-cache-")
        .tempdir_in(parent)?;

    let mut cmd = Command::new("buf");
    cmd.arg("build")
        .arg(&root)
        .arg("--disable-symlinks")
        .arg("--as-file-descriptor-set")
        .arg("-o")
        .arg(&staged_output);

    if exclude_imports {
        cmd.arg("--exclude-imports");
    }
    if exclude_source_info {
        cmd.arg("--exclude-source-info");
    }
    for name in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "no_proxy",
        "BUF_TOKEN",
    ] {
        cmd.env_remove(name);
    }
    cmd.env("XDG_CACHE_HOME", cache.path());

    let limits = crate::security::ProcessLimits::plugin(std::time::Duration::from_secs(300))?;
    let output = crate::security::run_command_bounded(cmd, b"", limits, "buf build")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("buf build failed:\n{stderr}"));
    }
    let descriptor = crate::security::read_file_bounded(
        &staged_output,
        MAX_DESCRIPTOR_BYTES,
        "generated protobuf descriptor",
    )?;
    axiograph_security::write_file_atomic_bounded(
        out,
        &descriptor,
        MAX_DESCRIPTOR_BYTES,
        "generated protobuf descriptor",
    )?;
    Ok(descriptor)
}

fn validate_buf_source_tree(root: &Path) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(anyhow!(
            "Buf source root must be a real directory, not a symlink or special file"
        ));
    }
    let root = root.canonicalize()?;
    let mut entries = 0_usize;
    let mut files = 0_usize;
    let mut total_bytes = 0_usize;
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .max_depth(MAX_PROTO_SOURCE_DEPTH)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || entry.file_name() != ".git")
    {
        let entry = entry?;
        entries = entries.saturating_add(1);
        if entries > MAX_PROTO_SOURCE_ENTRIES {
            return Err(anyhow!(
                "Buf source tree exceeds {MAX_PROTO_SOURCE_ENTRIES} filesystem entries"
            ));
        }
        if entry.depth() == 0 {
            continue;
        }
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(anyhow!(
                "Buf source tree must not contain symlinks: `{}`",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            return Err(anyhow!(
                "Buf source tree contains a special file: `{}`",
                entry.path().display()
            ));
        }
        let name = entry.file_name().to_string_lossy();
        let relevant = entry
            .path()
            .extension()
            .is_some_and(|value| value == "proto")
            || matches!(name.as_ref(), "buf.yaml" | "buf.work.yaml" | "buf.lock");
        if !relevant {
            continue;
        }
        files = files.saturating_add(1);
        if files > MAX_PROTO_SOURCE_FILES {
            return Err(anyhow!(
                "Buf source tree exceeds {MAX_PROTO_SOURCE_FILES} files"
            ));
        }
        let bytes = crate::security::read_file_bounded(
            entry.path(),
            MAX_PROTO_SOURCE_FILE_BYTES,
            "Buf source file",
        )?;
        total_bytes = total_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow!("Buf source byte count overflow"))?;
        if total_bytes > MAX_PROTO_SOURCE_TOTAL_BYTES {
            return Err(anyhow!(
                "Buf source tree exceeds {MAX_PROTO_SOURCE_TOTAL_BYTES} bytes"
            ));
        }
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn buf_source_validation_rejects_root_and_nested_symlinks() -> Result<()> {
        use std::os::unix::fs::symlink;

        let parent = tempfile::tempdir()?;
        let root = parent.path().join("module");
        fs::create_dir(&root)?;
        fs::write(root.join("buf.yaml"), b"version: v2\n")?;
        fs::write(root.join("service.proto"), b"syntax = \"proto3\";\n")?;
        assert!(validate_buf_source_tree(&root).is_ok());

        let root_link = parent.path().join("module-link");
        symlink(&root, &root_link)?;
        let error =
            validate_buf_source_tree(&root_link).expect_err("symlinked Buf root must reject");
        assert!(error.to_string().contains("real directory"));

        let outside = parent.path().join("outside.proto");
        fs::write(&outside, b"syntax = \"proto3\";\n")?;
        symlink(&outside, root.join("linked.proto"))?;
        let error =
            validate_buf_source_tree(&root).expect_err("nested Buf source symlink must reject");
        assert!(error.to_string().contains("must not contain symlinks"));
        Ok(())
    }
}
