use axiograph_projections::{
    manifest_readback_fixture_v1, ExternalEvidenceAuthorityV1, ProjectionBackendV1,
    ProjectionManifestV1, ReadbackReportV1, ReadbackTransportStatusV1,
};
use std::fs;
use std::process::Command;

#[test]
fn cli_emits_projection_and_checks_readback_without_granting_authority() {
    let temp = tempfile::tempdir().expect("tempdir");
    let input = temp.path().join("ProjectionCli.axi");
    let manifest_path = temp.path().join("projection.json");
    let artifact_path = temp.path().join("projection.tql");
    let inventory_path = temp.path().join("readback.json");
    let report_path = temp.path().join("readback-report.json");
    fs::write(
        &input,
        r#"module ProjectionCli

schema S:
  object Person
  object World
  relation Knows(source: Person @data, target: Person @data, world: World @world)

instance I of S:
  Person = {Alice, Bob}
  World = {Observed}
  Knows = {
    knows_1: (source=Alice, target=Bob, world=Observed)
  }
"#,
    )
    .expect("write input");

    let emit = Command::new(env!("CARGO_BIN_EXE_axiograph"))
        .args([
            "tools",
            "projection",
            "emit",
            input.to_str().expect("input path"),
            "--backend",
            "typedb",
            "--search-root",
            temp.path().to_str().expect("search root"),
            "--out",
            manifest_path.to_str().expect("manifest path"),
            "--artifact-out",
            artifact_path.to_str().expect("artifact path"),
        ])
        .output()
        .expect("run projection emit");
    assert!(
        emit.status.success(),
        "emit failed: {}",
        String::from_utf8_lossy(&emit.stderr)
    );

    let manifest: ProjectionManifestV1 =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read manifest"))
            .expect("parse manifest");
    assert_eq!(manifest.backend.backend, ProjectionBackendV1::TypeDb);
    assert!(fs::read_to_string(&artifact_path)
        .expect("read artifact")
        .contains("define"));

    let inventory = manifest_readback_fixture_v1(&manifest);
    fs::write(
        &inventory_path,
        serde_json::to_vec_pretty(&inventory).expect("inventory JSON"),
    )
    .expect("write inventory");
    let check = Command::new(env!("CARGO_BIN_EXE_axiograph"))
        .args([
            "tools",
            "projection",
            "check-readback",
            "--manifest",
            manifest_path.to_str().expect("manifest path"),
            "--inventory",
            inventory_path.to_str().expect("inventory path"),
            "--out",
            report_path.to_str().expect("report path"),
        ])
        .output()
        .expect("run projection readback");
    assert!(
        check.status.success(),
        "readback failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );

    let report: ReadbackReportV1 =
        serde_json::from_slice(&fs::read(&report_path).expect("read report"))
            .expect("parse report");
    assert_eq!(
        report.transport_status,
        ReadbackTransportStatusV1::ExactFiniteRecordMatch
    );
    assert_eq!(
        report.evidence.authority,
        ExternalEvidenceAuthorityV1::EvidenceOnly
    );
    assert!(!report.semantic_equivalence_claim);
    assert!(!report.evidence.accepted_state_change);
}
