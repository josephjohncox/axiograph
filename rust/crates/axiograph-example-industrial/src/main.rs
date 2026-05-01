use std::path::PathBuf;

use anyhow::{anyhow, Result};
use axiograph_example_industrial::{
    inspect_industrial_harness_run, render_industrial_harness_inspection,
    run_regulated_production_line_seed_harness_from_axi_module,
    REGULATED_PRODUCTION_LINE_CAMPAIGN_ID, REGULATED_PRODUCTION_LINE_MODULE_PATH,
};
use axiograph_pathdb::{AcceptedAxiAnchor, AcceptedSnapshotId, AxiDigest};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "axiograph-industrial-example")]
#[command(about = "Industrial engineering teaching harness over canonical Axiograph .axi")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Materialize the regulated production line teaching run from canonical `.axi`.
    RunRegulatedSeed {
        /// Canonical `.axi` module to import.
        #[arg(long, default_value = REGULATED_PRODUCTION_LINE_MODULE_PATH)]
        axi: PathBuf,
        /// Optional semantic snapshot id to use in the emitted anchor.
        #[arg(long)]
        accepted_snapshot_id: Option<String>,
        /// Root directory under which `_cache/industrial_harness/...` will be written.
        #[arg(long, default_value = ".")]
        cache_root: PathBuf,
        /// Deterministic run id for this materialization.
        #[arg(long)]
        run_id: String,
        /// Deterministic timestamp for this run.
        #[arg(long)]
        created_at_unix_secs: u64,
        /// Emit the typed `industrial_harness_run_response_v1` report as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Inspect one persisted industrial example run without mutating ontology state.
    Inspect {
        /// Root directory under which `_cache/industrial_harness/...` is stored.
        #[arg(long, default_value = ".")]
        cache_root: PathBuf,
        /// Harness campaign id.
        #[arg(long, default_value = REGULATED_PRODUCTION_LINE_CAMPAIGN_ID)]
        campaign_id: String,
        /// Harness run id.
        #[arg(long)]
        run_id: String,
        /// Optional expected snapshot id for anchor verification.
        #[arg(long)]
        expected_snapshot_id: Option<String>,
        /// Optional expected .axi digest for anchor verification.
        #[arg(long)]
        expected_axi_digest: Option<String>,
        /// Emit the typed `industrial_harness_inspection_result_v1` report as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn expected_anchor(
    expected_snapshot_id: Option<String>,
    expected_axi_digest: Option<String>,
) -> Result<Option<AcceptedAxiAnchor>> {
    match (expected_snapshot_id, expected_axi_digest) {
        (Some(snapshot), Some(digest)) => Ok(Some(AcceptedAxiAnchor::new(
            AcceptedSnapshotId::new(snapshot),
            AxiDigest::new(digest),
        ))),
        (None, None) => Ok(None),
        _ => Err(anyhow!(
            "`--expected-snapshot-id` and `--expected-axi-digest` must be provided together"
        )),
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::RunRegulatedSeed {
            axi,
            accepted_snapshot_id,
            cache_root,
            run_id,
            created_at_unix_secs,
            json,
        } => {
            let response = run_regulated_production_line_seed_harness_from_axi_module(
                &cache_root,
                &axi,
                accepted_snapshot_id.map(AcceptedSnapshotId::new),
                &run_id,
                created_at_unix_secs,
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("industrial example harness");
                println!("  campaign: {}", response.campaign_id);
                println!("  run: {}", response.run_id);
                println!(
                    "  accepted anchor: {} @ {}",
                    response.accepted_axi_anchor.accepted_snapshot_id,
                    response.accepted_axi_anchor.axi_digest
                );
                println!("  cache root: {}", response.cache_root);
                println!("  campaign manifest: {}", response.campaign_manifest_path);
                println!("  run artifact: {}", response.artifacts.run);
                println!("  cq results: {}", response.artifacts.cq_results);
                println!("  coverage: {}", response.artifacts.coverage);
                println!("  agent report: {}", response.artifacts.agent_report);
                println!("  distill: {}", response.artifacts.distill);
            }
        }
        Commands::Inspect {
            cache_root,
            campaign_id,
            run_id,
            expected_snapshot_id,
            expected_axi_digest,
            json,
        } => {
            let expected = expected_anchor(expected_snapshot_id, expected_axi_digest)?;
            let inspection = inspect_industrial_harness_run(
                &cache_root,
                &campaign_id,
                &run_id,
                expected.as_ref(),
            )?;
            if json {
                println!("{}", serde_json::to_string_pretty(&inspection)?);
            } else {
                println!(
                    "{}",
                    render_industrial_harness_inspection(&cache_root, &inspection)
                );
            }
        }
    }
    Ok(())
}
