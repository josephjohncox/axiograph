use std::path::PathBuf;

use anyhow::Result;
use axiograph_example_regulated_shipment::{run_workflow, RegulatedShipmentWorkflowInputs};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "axiograph-regulated-shipment")]
#[command(about = "Run the finite regulated-shipment persistence and typed-merge fixture")]
struct Args {
    #[arg(long)]
    baseline_axi: PathBuf,
    #[arg(long)]
    candidate_axi: PathBuf,
    #[arg(long)]
    baseline_authoring_report: PathBuf,
    #[arg(long)]
    candidate_authoring_report: PathBuf,
    #[arg(long)]
    baseline_theory_report: PathBuf,
    #[arg(long)]
    candidate_theory_report: PathBuf,
    #[arg(long)]
    baseline_verification_receipt: PathBuf,
    #[arg(long)]
    candidate_verification_receipt: PathBuf,
    #[arg(long)]
    store_dir: PathBuf,
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let report = run_workflow(&RegulatedShipmentWorkflowInputs {
        baseline_axi: args.baseline_axi,
        candidate_axi: args.candidate_axi,
        baseline_authoring_report: args.baseline_authoring_report,
        candidate_authoring_report: args.candidate_authoring_report,
        baseline_theory_report: args.baseline_theory_report,
        candidate_theory_report: args.candidate_theory_report,
        baseline_verification_receipt: args.baseline_verification_receipt,
        candidate_verification_receipt: args.candidate_verification_receipt,
        store_dir: args.store_dir,
    })?;
    let json = serde_json::to_string_pretty(&report)?;
    if let Some(out) = args.out {
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        axiograph_security::write_file_atomic_bounded(
            &out,
            format!("{json}\n"),
            16 * 1024 * 1024,
            "regulated-shipment report",
        )?;
        println!("wrote {}", out.display());
    } else {
        println!("{json}");
    }
    Ok(())
}
