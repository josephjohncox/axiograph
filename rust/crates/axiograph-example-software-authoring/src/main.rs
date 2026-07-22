use std::path::PathBuf;

use anyhow::Result;
use axiograph_example_software_authoring::{
    render_continuous_semantic_coverage_example, run_continuous_semantic_coverage_example,
    ContinuousSemanticCoverageExampleOptions,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "axiograph-software-authoring-example")]
#[command(about = "Pedagogical software-authoring example over Axiograph typed coverage reports")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Check a generated behavior-case report as continuous semantic coverage.
    ContinuousCheck {
        /// Path to a behavior_case_report_v1 JSON file.
        #[arg(long)]
        behavior_report: PathBuf,
        /// Repository root used to resolve code_refs from the behavior report.
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        /// Comma-separated list of generated languages required in codegen_previews.
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "rust,typescript,python,go"
        )]
        require_codegen: Vec<String>,
        /// Fail when ontology rules are not mapped to implementation/test coverage.
        #[arg(long)]
        strict_coverage: bool,
        /// Fail when code_refs do not exist on disk.
        #[arg(long)]
        require_code_refs: bool,
        /// Fail unless the report carries a runtime theory-check summary.
        #[arg(long)]
        require_runtime_theory: bool,
        /// Output JSON path. Defaults to stdout for --json and human text otherwise.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Emit JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::ContinuousCheck {
            behavior_report,
            repo_root,
            require_codegen,
            strict_coverage,
            require_code_refs,
            require_runtime_theory,
            out,
            json,
        } => {
            let report = run_continuous_semantic_coverage_example(
                &ContinuousSemanticCoverageExampleOptions {
                    behavior_report,
                    repo_root,
                    require_codegen,
                    strict_coverage,
                    require_code_refs,
                    require_runtime_theory,
                },
            )?;
            if let Some(out) = out {
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                axiograph_security::write_file_atomic_bounded(
                    &out,
                    serde_json::to_string_pretty(&report)?,
                    8 * 1024 * 1024,
                    "software-authoring example report",
                )?;
                println!("wrote {}", out.display());
            } else if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", render_continuous_semantic_coverage_example(&report));
            }
            if !report.coverage_report.pass {
                anyhow::bail!("continuous semantic coverage example failed");
            }
        }
    }
    Ok(())
}
