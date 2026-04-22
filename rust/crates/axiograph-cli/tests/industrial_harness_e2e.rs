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

fn axiograph_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_axiograph"))
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

fn count_snapshot_manifests(accepted_dir: &Path) -> usize {
    fs::read_dir(accepted_dir.join("snapshots"))
        .expect("read snapshots dir")
        .filter_map(Result::ok)
        .count()
}

#[test]
fn regulated_line_cli_materializes_cache_from_accepted_snapshot_without_mutation() {
    let repo_root = repo_root();
    let test_root = unique_run_dir(&repo_root, "regulated_line_cli");
    let accepted_dir = test_root.join("accepted_plane");
    let cache_root = test_root.join("cache_root");
    let axi_path = repo_root.join("examples/industrial/RegulatedProductionLine.axi");
    let bin = axiograph_bin();

    let promote_status = Command::new(&bin)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&axi_path)
        .arg("--dir")
        .arg(&accepted_dir)
        .status()
        .expect("promote regulated line module");
    assert!(promote_status.success(), "promotion command should succeed");

    let head_before = fs::read_to_string(accepted_dir.join("HEAD")).expect("read accepted HEAD");
    let snapshot_count_before = count_snapshot_manifests(&accepted_dir);

    let output = Command::new(&bin)
        .arg("tools")
        .arg("industrial-harness")
        .arg("run-regulated-seed")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg("head")
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

    let head_after =
        fs::read_to_string(accepted_dir.join("HEAD")).expect("read accepted HEAD after run");
    let snapshot_count_after = count_snapshot_manifests(&accepted_dir);
    assert_eq!(head_before, head_after, "accepted HEAD must stay unchanged");
    assert_eq!(
        snapshot_count_before, snapshot_count_after,
        "accepted snapshot manifests must stay unchanged"
    );

    fs::remove_dir_all(&test_root).ok();
}
