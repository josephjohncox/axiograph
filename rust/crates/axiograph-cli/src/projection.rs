use crate::axi_input::compile_canonical_axi_path;
use anyhow::{Context, Result};
use axiograph_projections::{
    check_readback_v1, project_snapshot_v1, ProjectionBackendV1, ProjectionManifestV1,
    ReadbackInventoryV1,
};
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub(crate) enum ProjectionCommands {
    /// Compile exact canonical `.axi` and emit one typed derived projection manifest.
    Emit(ProjectionEmitArgs),
    /// Compare backend readback records with one projection manifest.
    CheckReadback(ProjectionReadbackArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ProjectionBackendArg {
    Pathdb,
    Typedb,
    Terminusdb,
    RdfOwl,
    PropertyGraph,
}

impl From<ProjectionBackendArg> for ProjectionBackendV1 {
    fn from(value: ProjectionBackendArg) -> Self {
        match value {
            ProjectionBackendArg::Pathdb => Self::PathDb,
            ProjectionBackendArg::Typedb => Self::TypeDb,
            ProjectionBackendArg::Terminusdb => Self::TerminusDb,
            ProjectionBackendArg::RdfOwl => Self::RdfOwl,
            ProjectionBackendArg::PropertyGraph => Self::PropertyGraph,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct ProjectionEmitArgs {
    /// Root canonical `.axi` module. Imports are compiled from exact source bytes.
    pub input: PathBuf,

    /// Derived backend target.
    #[arg(long, value_enum)]
    pub backend: ProjectionBackendArg,

    /// Search root for imported `.axi` modules. May be repeated.
    #[arg(long = "search-root", default_value = ".")]
    pub search_roots: Vec<PathBuf>,

    /// Projection manifest JSON. Defaults to stdout.
    #[arg(short, long)]
    pub out: Option<PathBuf>,

    /// Optional native artifact body output. The manifest remains the audit contract.
    #[arg(long)]
    pub artifact_out: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct ProjectionReadbackArgs {
    /// Typed projection manifest emitted by `tools projection emit`.
    #[arg(long)]
    pub manifest: PathBuf,

    /// Backend readback inventory containing record ids and payload fingerprints.
    #[arg(long)]
    pub inventory: PathBuf,

    /// Readback report JSON. Defaults to stdout.
    #[arg(short, long)]
    pub out: Option<PathBuf>,
}

pub(crate) fn cmd_projection(command: ProjectionCommands) -> Result<()> {
    match command {
        ProjectionCommands::Emit(args) => emit_projection(args),
        ProjectionCommands::CheckReadback(args) => check_projection_readback(args),
    }
}

fn emit_projection(args: ProjectionEmitArgs) -> Result<()> {
    let package = compile_canonical_axi_path(&args.input, &args.search_roots)
        .with_context(|| format!("compile canonical package from {}", args.input.display()))?;
    let manifest = project_snapshot_v1(package.snapshot(), args.backend.into())?;
    if let Some(path) = args.artifact_out {
        crate::security::write_output_bounded(&path, &manifest.artifact.body, "CLI output")
            .with_context(|| format!("write native projection artifact {}", path.display()))?;
    }
    write_json(args.out, &manifest)
}

fn check_projection_readback(args: ProjectionReadbackArgs) -> Result<()> {
    let manifest: ProjectionManifestV1 = crate::security::parse_json_bounded(
        &crate::security::read_file_bounded(
            &args.manifest,
            crate::security::MAX_JSON_INPUT_BYTES,
            "projection manifest",
        )?,
        crate::security::MAX_JSON_INPUT_BYTES,
        "projection manifest",
    )
    .with_context(|| format!("parse projection manifest {}", args.manifest.display()))?;
    let inventory: ReadbackInventoryV1 = crate::security::parse_json_bounded(
        &crate::security::read_file_bounded(
            &args.inventory,
            crate::security::MAX_JSON_INPUT_BYTES,
            "projection readback inventory",
        )?,
        crate::security::MAX_JSON_INPUT_BYTES,
        "projection readback inventory",
    )
    .with_context(|| format!("parse readback inventory {}", args.inventory.display()))?;
    let report = check_readback_v1(&manifest, &inventory)?;
    write_json(args.out, &report)
}

fn write_json<T: serde::Serialize>(out: Option<PathBuf>, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if let Some(path) = out {
        crate::security::write_output_bounded(&path, bytes, "CLI output")
            .with_context(|| format!("write {}", path.display()))?;
    } else {
        print!("{}", String::from_utf8(bytes).expect("JSON is UTF-8"));
    }
    Ok(())
}
