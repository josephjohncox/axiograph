use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn industrial_example_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_axiograph-industrial-example"))
}

fn unique_run_dir(repo_root: &Path, label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let dir = repo_root
        .join("rust/target/tmp/axiograph_industrial_harness_e2e")
        .join(format!("{label}_{pid}_{nanos}"));
    fs::create_dir_all(&dir).expect("create e2e run dir");
    dir
}

#[test]
fn regulated_line_example_cli_materializes_cache_from_canonical_axi() {
    let repo_root = repo_root();
    let test_root = unique_run_dir(&repo_root, "regulated_line_cli");
    let cache_root = test_root.join("cache_root");
    let axi_path = repo_root.join("examples/industrial/RegulatedProductionLine.axi");
    let bin = industrial_example_bin();

    let output = Command::new(&bin)
        .arg("run-regulated-seed")
        .arg("--axi")
        .arg(&axi_path)
        .arg("--cache-root")
        .arg(&cache_root)
        .arg("--run-id")
        .arg("regulated-seed-e2e")
        .arg("--created-at-unix-secs")
        .arg("1713810000")
        .arg("--json")
        .output()
        .expect("run industrial harness CLI");
    assert!(
        output.status.success(),
        "industrial harness command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let response: Value =
        serde_json::from_slice(&output.stdout).expect("deserialize CLI JSON response");
    assert_eq!(response["campaign_id"], "regulated_production_line_seed");
    assert_eq!(response["run_id"], "regulated-seed-e2e");
    assert_eq!(response["trust"]["trust_class"], "runtime_guarded");
    let expected_snapshot_id = response["accepted_axi_anchor"]["accepted_snapshot_id"]
        .as_str()
        .expect("accepted snapshot id")
        .to_string();
    let expected_axi_digest = response["accepted_axi_anchor"]["axi_digest"]
        .as_str()
        .expect("axi digest")
        .to_string();

    let harness_run_dir = cache_root
        .join("_cache/industrial_harness/regulated_production_line_seed/runs/regulated-seed-e2e");
    let run_path = harness_run_dir.join("run.json");
    let cq_path = harness_run_dir.join("cq_results.json");
    let coverage_path = harness_run_dir.join("coverage.json");
    let agent_report_path = harness_run_dir.join("agent_report.json");
    let distill_path = harness_run_dir.join("distill.json");
    assert!(run_path.exists(), "run artifact should exist");
    assert!(cq_path.exists(), "cq artifact should exist");
    assert!(coverage_path.exists(), "coverage artifact should exist");
    assert!(
        agent_report_path.exists(),
        "agent report artifact should exist"
    );
    assert!(distill_path.exists(), "distill artifact should exist");

    let run_json: Value =
        serde_json::from_str(&fs::read_to_string(&run_path).expect("read run artifact"))
            .expect("deserialize run artifact");
    assert_eq!(run_json["run_id"], "regulated-seed-e2e");
    assert_eq!(run_json["created_at_unix_secs"], 1713810000u64);
    assert_eq!(
        run_json["trust"]["soundness"],
        "accepted_anchor_scoped_shadow_harness_run"
    );

    let cq_json: Value =
        serde_json::from_str(&fs::read_to_string(&cq_path).expect("read cq artifact"))
            .expect("deserialize cq artifact");
    assert!(cq_json["summary"]["total"].as_u64().unwrap_or(0) >= 2);
    assert!(cq_json["summary"]["passed"].as_u64().unwrap_or(0) >= 1);

    let coverage_json: Value =
        serde_json::from_str(&fs::read_to_string(&coverage_path).expect("read coverage artifact"))
            .expect("deserialize coverage artifact");
    assert!(
        coverage_json["summary"]["total_surfaces"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    let coverage_surfaces = coverage_json["surfaces"]
        .as_array()
        .expect("coverage surfaces array");
    assert!(coverage_surfaces
        .iter()
        .any(|surface| surface["surface"] == "Release review SOP"));

    let inspect_text_output = Command::new(&bin)
        .arg("inspect")
        .arg("--cache-root")
        .arg(&cache_root)
        .arg("--campaign-id")
        .arg("regulated_production_line_seed")
        .arg("--run-id")
        .arg("regulated-seed-e2e")
        .output()
        .expect("inspect industrial harness CLI text output");
    assert!(
        inspect_text_output.status.success(),
        "industrial harness inspect text command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&inspect_text_output.stdout),
        String::from_utf8_lossy(&inspect_text_output.stderr)
    );
    let inspect_text = String::from_utf8_lossy(&inspect_text_output.stdout);
    assert!(inspect_text.contains("status\n"));
    assert!(inspect_text.contains("  run: regulated-seed-e2e"));
    assert!(inspect_text.contains("report\n"));
    assert!(inspect_text.contains("readback\n"));
    assert!(inspect_text.contains(&run_path.display().to_string()));

    let inspect_json_output = Command::new(&bin)
        .arg("inspect")
        .arg("--cache-root")
        .arg(&cache_root)
        .arg("--campaign-id")
        .arg("regulated_production_line_seed")
        .arg("--run-id")
        .arg("regulated-seed-e2e")
        .arg("--json")
        .output()
        .expect("inspect industrial harness CLI json output");
    assert!(
        inspect_json_output.status.success(),
        "industrial harness inspect json command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&inspect_json_output.stdout),
        String::from_utf8_lossy(&inspect_json_output.stderr)
    );
    let inspect_json: Value = serde_json::from_slice(&inspect_json_output.stdout)
        .expect("deserialize inspect CLI JSON response");
    assert_eq!(
        inspect_json["bundle"]["run"]["run_id"].as_str(),
        Some("regulated-seed-e2e")
    );
    assert_eq!(inspect_json["verification"]["failed"].as_u64(), Some(0));

    let inspect_verified_output = Command::new(&bin)
        .arg("inspect")
        .arg("--cache-root")
        .arg(&cache_root)
        .arg("--campaign-id")
        .arg("regulated_production_line_seed")
        .arg("--run-id")
        .arg("regulated-seed-e2e")
        .arg("--expected-snapshot-id")
        .arg(expected_snapshot_id)
        .arg("--expected-axi-digest")
        .arg(expected_axi_digest)
        .arg("--json")
        .output()
        .expect("inspect industrial harness CLI json output with accepted anchor");
    assert!(
        inspect_verified_output.status.success(),
        "industrial harness inspect verification command failed: stdout={} stderr={}",
        String::from_utf8_lossy(&inspect_verified_output.stdout),
        String::from_utf8_lossy(&inspect_verified_output.stderr)
    );
    let inspect_verified_json: Value = serde_json::from_slice(&inspect_verified_output.stdout)
        .expect("deserialize inspect verification CLI JSON response");
    assert_eq!(
        inspect_verified_json["verification"]["failed"].as_u64(),
        Some(0)
    );

    fs::remove_dir_all(&test_root).ok();
}
