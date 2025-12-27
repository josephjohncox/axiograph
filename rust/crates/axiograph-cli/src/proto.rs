//! Protobuf / gRPC ingestion commands.
//!
//! Pipeline:
//! 1. Run `buf build --as-file-descriptor-set -o descriptor.json`
//! 2. Convert descriptor JSON → `proposals.json` (+ optional chunks)

use anyhow::{anyhow, Context, Result};
use clap::Subcommand;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Subcommand)]
pub enum ProtoCommands {
    /// Build a Buf descriptor set (`google.protobuf.FileDescriptorSet`) as JSON.
    BuildDescriptor {
        /// Buf module root (directory containing `buf.yaml`).
        root: PathBuf,
        /// Output JSON file (descriptor set).
        #[arg(short, long)]
        out: PathBuf,
        /// Exclude imports from the descriptor set.
        #[arg(long)]
        exclude_imports: bool,
        /// Exclude source info (comments + spans) from the descriptor set.
        #[arg(long)]
        exclude_source_info: bool,
    },

    /// Ingest a Buf descriptor set JSON into `proposals.json` (+ optional chunks).
    Ingest {
        /// Buf module root (directory containing `buf.yaml`).
        root: PathBuf,
        /// Output proposals JSON (Evidence/Proposals schema).
        #[arg(short, long)]
        out: PathBuf,
        /// Optional output chunks JSON (for RAG).
        #[arg(long)]
        chunks: Option<PathBuf>,
        /// Optional path to an existing descriptor set JSON (skip `buf build`).
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
            build_descriptor_set_json(&root, &out, exclude_imports, exclude_source_info)?;
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

fn cmd_proto_ingest(
    root: &PathBuf,
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

    let descriptor_path_owned;
    let descriptor_path = if let Some(path) = descriptor_in {
        path
    } else {
        let default_out = out
            .parent()
            .unwrap_or(Path::new("."))
            .join("descriptor.json");
        let out_path = descriptor_out.unwrap_or(&default_out);
        fs::create_dir_all(out_path.parent().unwrap_or(Path::new(".")))?;
        build_descriptor_set_json(root, out_path, exclude_imports, exclude_source_info)?;
        descriptor_path_owned = out_path.clone();
        &descriptor_path_owned
    };

    let descriptor_text = fs::read_to_string(descriptor_path).with_context(|| {
        format!(
            "failed to read descriptor json: {}",
            descriptor_path.display()
        )
    })?;

    let ingest = axiograph_ingest_proto::ingest_descriptor_set_json(
        &descriptor_text,
        Some(descriptor_path.display().to_string()),
        Some(schema_hint.to_string()),
    )?;

    // Always emit chunks for RAG grounding (default: alongside the proposals output).
    let chunks_path = chunks_out
        .cloned()
        .unwrap_or_else(|| out.parent().unwrap_or(Path::new(".")).join("chunks.json"));
    fs::create_dir_all(chunks_path.parent().unwrap_or(Path::new(".")))?;
    let json = serde_json::to_string_pretty(&ingest.chunks)?;
    fs::write(&chunks_path, &json)?;
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
    fs::write(out, &json)?;
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

pub(crate) fn build_descriptor_set_json(
    root: &PathBuf,
    out: &PathBuf,
    exclude_imports: bool,
    exclude_source_info: bool,
) -> Result<()> {
    let mut cmd = Command::new("buf");
    cmd.arg("build")
        .arg(root)
        .arg("--as-file-descriptor-set")
        .arg("-o")
        .arg(out);

    if exclude_imports {
        cmd.arg("--exclude-imports");
    }
    if exclude_source_info {
        cmd.arg("--exclude-source-info");
    }

    // In sandboxed environments, Buf may not be able to write to `$HOME/.cache`.
    // Default to a workspace-local cache to keep `buf build` working.
    let cache_dir = PathBuf::from("build/buf_cache");
    let _ = fs::create_dir_all(&cache_dir);
    cmd.env("XDG_CACHE_HOME", cache_dir);

    let output = cmd.output().with_context(|| "failed to run `buf build`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("buf build failed:\n{stderr}"));
    }
    Ok(())
}
