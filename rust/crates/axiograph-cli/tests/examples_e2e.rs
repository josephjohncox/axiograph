use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_ingest_docs::{
    Chunk, EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1,
};
use axiograph_pathdb::certificate::{CertificatePayloadV2, CertificateV2};
use walkdir::WalkDir;

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
        .join("rust/target/tmp/axiograph_examples_e2e")
        .join(format!("{label}_{pid}_{nanos}"));
    fs::create_dir_all(dir.join("build")).expect("create run dir build/");
    dir
}

#[test]
fn example_catalog_paths_exist_and_stay_teaching_oriented() {
    let repo_root = repo_root();
    let catalog_path = repo_root.join("examples/catalog.json");
    let text = fs::read_to_string(&catalog_path).expect("read examples/catalog.json");
    let catalog: serde_json::Value =
        serde_json::from_str(&text).expect("parse examples/catalog.json");
    assert_eq!(catalog["version"], serde_json::json!(1));

    let examples = catalog["examples"]
        .as_array()
        .expect("catalog examples must be an array");
    assert!(
        examples.len() >= 8,
        "expected a pedagogical catalog with multiple routes"
    );

    for example in examples {
        let id = example["id"].as_str().expect("example id string");
        let path = example["path"].as_str().expect("example path string");
        assert!(
            repo_root.join(path).exists(),
            "catalog example `{id}` points to missing path `{path}`"
        );

        let tags = example["feature_tags"]
            .as_array()
            .expect("feature_tags must be an array");
        assert!(
            !tags.is_empty(),
            "catalog example `{id}` needs feature tags so agents can route it"
        );

        let commands = example["commands"].as_array().expect("commands array");
        for command in commands {
            let command = command.as_str().expect("command string");
            assert!(
                !command.contains("export_axi build/"),
                "catalog example `{id}` should not foreground PathDBExportV1 export-era scripts"
            );
        }
    }
}

#[test]
fn validate_all_examples_axi() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let mut axi_files: Vec<PathBuf> = WalkDir::new(repo_root.join("examples"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().map(|s| s == "axi").unwrap_or(false))
        .filter(|p| {
            p.file_name()
                .map(|name| name != "pathdb_export_anchor_v1.axi")
                .unwrap_or(true)
        })
        .collect();
    axi_files.sort();

    assert!(
        !axi_files.is_empty(),
        "expected at least one `.axi` under examples/"
    );

    for path in axi_files {
        let status = Command::new(&bin)
            .current_dir(&repo_root)
            .arg("check")
            .arg("validate")
            .arg(&path)
            .status()
            .expect("run axiograph check validate");

        assert!(
            status.success(),
            "validate failed for `{}` (exit={})",
            path.display(),
            status.code().unwrap_or(-1)
        );
    }
}

#[test]
fn behavior_case_example_fixture_runs() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "behavior_case_example");
    let out_path = run_dir.join("build/regulated_ship_release_behavior_case_report.json");

    let status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("behavior-case")
        .arg("examples/industrial/RegulatedProductionLine.axi")
        .arg("--request")
        .arg("examples/behavior_cases/regulated_ship_release.json")
        .arg("--overlay")
        .arg("examples/behavior_cases/regulated_ship_release_overlay.json")
        .arg("--out")
        .arg(&out_path)
        .status()
        .expect("run behavior-case example fixture");

    assert!(
        status.success(),
        "behavior-case example fixture failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let report_text = fs::read_to_string(&out_path).expect("read behavior-case report");
    let report: serde_json::Value =
        serde_json::from_str(&report_text).expect("parse behavior-case report");
    assert_eq!(
        report["version"],
        serde_json::json!("behavior_case_report_v1")
    );
    assert_eq!(
        report["behavior_case"]["case_id"],
        serde_json::json!("industrial.ship_released_order")
    );
    assert!(
        report["codegen_previews"]
            .as_array()
            .expect("codegen_previews array")
            .iter()
            .any(|preview| preview["language"] == serde_json::json!("rust")),
        "expected Rust test skeleton preview"
    );
}

#[test]
fn software_authoring_behavior_case_emits_multi_language_skeletons() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "software_authoring_behavior_case");
    let out_path = run_dir.join("build/order_fulfillment_behavior_case_report.json");

    let status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("behavior-case")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--request")
        .arg("examples/software_authoring/order_fulfillment_behavior_case.json")
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .arg("--out")
        .arg(&out_path)
        .status()
        .expect("run software-authoring behavior-case example fixture");

    assert!(
        status.success(),
        "software-authoring behavior-case example failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let report_text = fs::read_to_string(&out_path).expect("read behavior-case report");
    let report: serde_json::Value =
        serde_json::from_str(&report_text).expect("parse behavior-case report");
    assert_eq!(
        report["behavior_case"]["case_id"],
        serde_json::json!("software_authoring.reserve_credit")
    );

    let languages = report["codegen_previews"]
        .as_array()
        .expect("codegen_previews array")
        .iter()
        .map(|preview| preview["language"].as_str().unwrap_or_default().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        languages,
        ["go", "python", "rust", "typescript"]
            .into_iter()
            .map(str::to_string)
            .collect()
    );
    assert!(
        report["codegen_previews"]
            .as_array()
            .expect("codegen_previews array")
            .iter()
            .any(|preview| preview["content"]
                .as_str()
                .unwrap_or_default()
                .contains("Bind this receipt to the real application service")),
        "expected implementation-facing skeleton guidance"
    );
}

#[test]
fn software_authoring_example_crate_runs_continuous_semantic_coverage() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "software_authoring_example_crate");
    let behavior_report = run_dir.join("build/order_fulfillment_behavior_case_report.json");
    let continuous_report = run_dir.join("build/example_crate_continuous_coverage.json");

    let behavior_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("behavior-case")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--request")
        .arg("examples/software_authoring/order_fulfillment_behavior_case.json")
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .arg("--out")
        .arg(&behavior_report)
        .status()
        .expect("run behavior-case before software-authoring example crate");
    assert!(
        behavior_status.success(),
        "behavior-case setup failed for software-authoring example crate"
    );

    let example_status = Command::new("cargo")
        .current_dir(&repo_root)
        .arg("run")
        .arg("--manifest-path")
        .arg("rust/Cargo.toml")
        .arg("-p")
        .arg("axiograph-example-software-authoring")
        .arg("--bin")
        .arg("axiograph-software-authoring-example")
        .arg("--")
        .arg("continuous-check")
        .arg("--behavior-report")
        .arg(&behavior_report)
        .arg("--repo-root")
        .arg(&repo_root)
        .arg("--out")
        .arg(&continuous_report)
        .status()
        .expect("run software-authoring example crate");
    assert!(
        example_status.success(),
        "software-authoring example crate failed"
    );

    let report: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&continuous_report).expect("read continuous coverage report"),
    )
    .expect("parse continuous coverage report");
    assert_eq!(
        report["version"],
        serde_json::json!("software_authoring_example_continuous_check_v1")
    );
    assert_eq!(report["coverage_report"]["pass"], serde_json::json!(true));
    assert!(report["coverage_report"]["present_codegen_languages"]
        .as_array()
        .expect("present codegen languages")
        .iter()
        .any(|language| language == "rust"));
}

#[test]
fn software_authoring_overlay_tools_support_weak_and_enforced_modes() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "software_authoring_overlay_tools");
    let overlay_report = run_dir.join("build/order_fulfillment_overlay_report.json");
    let define_report = run_dir.join("build/order_fulfillment_definition_report.json");
    let coverage_query_report = run_dir.join("build/order_fulfillment_coverage_query_report.json");
    let coverage_report = run_dir.join("build/order_fulfillment_software_coverage.json");

    let overlay_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("overlay-check")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .arg("--out")
        .arg(&overlay_report)
        .status()
        .expect("run overlay-check");
    assert!(overlay_status.success(), "overlay-check failed");

    let overlay_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&overlay_report).expect("read overlay report"))
            .expect("parse overlay report");
    assert_eq!(
        overlay_json["version"],
        serde_json::json!("overlay_validation_report_v1")
    );
    assert_eq!(overlay_json["valid"], serde_json::json!(true));

    let define_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("define")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .arg("--prompt")
        .arg("define the shipment eligibility business rule")
        .arg("--kind-hint")
        .arg("business_rule")
        .arg("--include-queries")
        .arg("--out")
        .arg(&define_report)
        .status()
        .expect("run definition query");
    assert!(define_status.success(), "definition query failed");
    let define_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&define_report).expect("read define report"))
            .expect("parse define report");
    assert_eq!(
        define_json["coverage_mode"],
        serde_json::json!("definition_query")
    );
    assert!(
        define_json["candidates"]
            .as_array()
            .expect("candidates array")
            .iter()
            .any(|candidate| candidate["ref_id"] == serde_json::json!("rule:shipment-eligibility")),
        "definition query should surface the overlay business-rule binding"
    );

    let query_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("coverage-query")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .arg("--query")
        .arg("examples/software_authoring/order_fulfillment_coverage_query.json")
        .arg("--out")
        .arg(&coverage_query_report)
        .status()
        .expect("run coverage query");
    assert!(query_status.success(), "coverage query failed");
    let query_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&coverage_query_report).expect("read coverage query report"),
    )
    .expect("parse coverage query report");
    assert_eq!(
        query_json["coverage_mode"],
        serde_json::json!("exploratory")
    );
    assert!(query_json["caveats"]
        .as_array()
        .expect("caveats array")
        .iter()
        .any(|caveat| caveat
            .as_str()
            .unwrap_or_default()
            .contains("cannot satisfy promotion gates")));

    let coverage_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("check")
        .arg("software-coverage")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--behavior-case")
        .arg("examples/software_authoring/order_fulfillment_behavior_case.json")
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .arg("--out")
        .arg(&coverage_report)
        .status()
        .expect("run software coverage");
    assert!(coverage_status.success(), "software coverage failed");
    let coverage_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&coverage_report).expect("read coverage report"))
            .expect("parse coverage report");
    assert_eq!(
        coverage_json["version"],
        serde_json::json!("continuous_software_coverage_report_v1")
    );
    assert_eq!(
        coverage_json["coverage_mode"],
        serde_json::json!("advisory")
    );
}

#[test]
fn software_authoring_script_runs_authoring_flow() {
    let repo_root = repo_root();
    let run_dir = unique_run_dir(&repo_root, "software_authoring_script");
    let out_dir = run_dir.join("build/authoring_flow");
    let script = repo_root.join("examples/software_authoring/run_authoring_flow.sh");

    let output = Command::new(&script)
        .current_dir(&repo_root)
        .env("AXIOGRAPH_BIN", axiograph_bin())
        .arg(&out_dir)
        .output()
        .expect("run software-authoring script");

    assert!(
        output.status.success(),
        "software-authoring script failed (exit={})\nstdout={}\nstderr={}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    for file in [
        "theory_check.json",
        "overlay_validation.json",
        "coverage_query.json",
        "behavior_case_report.json",
        "software_coverage.json",
        "materialize_skeletons.json",
        "definitions/define_reserve_credit_process.json",
        "definitions/define_shipment_eligibility_business_rule.json",
        "definitions/define_checkout_function.json",
    ] {
        assert!(
            out_dir.join(file).exists(),
            "script should produce {}",
            out_dir.join(file).display()
        );
    }

    let coverage_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("software_coverage.json"))
            .expect("read scripted software coverage"),
    )
    .expect("parse scripted software coverage");
    assert_eq!(
        coverage_json["version"],
        serde_json::json!("continuous_software_coverage_report_v1")
    );

    let materialized_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("materialize_skeletons.json"))
            .expect("read materialization report"),
    )
    .expect("parse materialization report");
    assert_eq!(
        materialized_json["version"],
        serde_json::json!("codegen_materialization_report_v1")
    );
    assert!(
        materialized_json["written_files"]
            .as_array()
            .expect("written_files array")
            .len()
            >= 4,
        "expected generated skeletons for the multi-language authoring example"
    );
}

#[test]
fn software_authoring_codegen_suite_runs_new_examples() {
    let repo_root = repo_root();
    let script = repo_root.join("examples/software_authoring/run_codegen_examples.sh");
    let suite = repo_root.join("examples/software_authoring/software_authoring_examples.json");

    for example_id in ["subscription_billing", "process_control"] {
        let run_dir = unique_run_dir(
            &repo_root,
            &format!("software_authoring_codegen_suite_{example_id}"),
        );
        let out_root = run_dir.join("build/codegen_examples");

        let output = Command::new(&script)
            .current_dir(&repo_root)
            .env("AXIOGRAPH_BIN", axiograph_bin())
            .env("EXAMPLE_ID", example_id)
            .arg(&suite)
            .arg(&out_root)
            .output()
            .expect("run software-authoring codegen suite");

        assert!(
            output.status.success(),
            "software-authoring codegen suite failed for {example_id} (exit={})\nstdout={}\nstderr={}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let example_out = out_root.join(example_id);
        for file in [
            "theory_check.json",
            "overlay_validation.json",
            "coverage_query.json",
            "behavior_case_report.json",
            "software_coverage.json",
            "materialize_skeletons.json",
        ] {
            assert!(
                example_out.join(file).exists(),
                "suite should produce {}",
                example_out.join(file).display()
            );
        }

        let materialized_json: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(example_out.join("materialize_skeletons.json"))
                .expect("read materialization report"),
        )
        .expect("parse materialization report");
        assert_eq!(
            materialized_json["version"],
            serde_json::json!("codegen_materialization_report_v1")
        );
        assert!(
            materialized_json["written_files"]
                .as_array()
                .expect("written_files array")
                .len()
                >= 4,
            "expected multi-language skeletons for {example_id}"
        );
    }
}

#[test]
fn software_authoring_cli_exposes_codegen_and_editor_contracts() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "software_authoring_cli_contracts");
    let codegen_plan = run_dir.join("build/codegen_plan.json");
    let tool_specs = run_dir.join("build/tool_specs.json");
    let lsp = run_dir.join("build/lsp_capabilities.json");
    let integration_manifest = run_dir.join("build/integration_manifest.json");

    let codegen = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("authoring")
        .arg("codegen-plan")
        .arg("--overlay")
        .arg("examples/software_authoring/process_control_tooling_overlay.json")
        .arg("--out")
        .arg(&codegen_plan)
        .status()
        .expect("run authoring codegen-plan");
    assert!(codegen.success(), "authoring codegen-plan failed");

    let specs = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("authoring")
        .arg("tool-specs")
        .arg("--out")
        .arg(&tool_specs)
        .status()
        .expect("run authoring tool-specs");
    assert!(specs.success(), "authoring tool-specs failed");

    let lsp_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("authoring")
        .arg("lsp-capabilities")
        .arg("--out")
        .arg(&lsp)
        .status()
        .expect("run authoring lsp-capabilities");
    assert!(lsp_status.success(), "authoring lsp-capabilities failed");

    let manifest_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("authoring")
        .arg("integration-manifest")
        .arg("--out")
        .arg(&integration_manifest)
        .status()
        .expect("run authoring integration-manifest");
    assert!(
        manifest_status.success(),
        "authoring integration-manifest failed"
    );

    let codegen_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&codegen_plan).expect("read codegen plan"))
            .expect("parse codegen plan");
    assert_eq!(
        codegen_json["version"],
        serde_json::json!("codegen_plan_report_v1")
    );
    assert!(codegen_json["file_hints"]
        .as_array()
        .expect("file_hints array")
        .iter()
        .any(|hint| hint.as_str().unwrap_or_default().ends_with(".rs")));

    let specs_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&tool_specs).expect("read tool specs"))
            .expect("parse tool specs");
    assert_eq!(
        specs_json["version"],
        serde_json::json!("axiograph_software_authoring_tool_specs_v1")
    );

    let lsp_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lsp).expect("read lsp capabilities"))
            .expect("parse lsp capabilities");
    assert_eq!(
        lsp_json["version"],
        serde_json::json!("axiograph_software_authoring_lsp_capabilities_v1")
    );

    let manifest_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&integration_manifest).expect("read integration manifest"),
    )
    .expect("parse integration manifest");
    assert_eq!(
        manifest_json["version"],
        serde_json::json!("axiograph_software_authoring_integration_manifest_v1")
    );
    assert_eq!(
        manifest_json["lsp"]["args"],
        serde_json::json!(["authoring", "lsp"])
    );
    assert_eq!(
        manifest_json["mcp"]["args"],
        serde_json::json!(["authoring", "mcp"])
    );
    assert!(manifest_json["mcp"]["tools"]
        .as_array()
        .expect("mcp tools")
        .iter()
        .any(|tool| tool["name"] == serde_json::json!("axiograph_authoring_codegen_plan")));
}

#[test]
fn stale_embedded_tooling_behavior_case_fields_fail_clearly() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "stale_behavior_case_schema");
    let stale_request = run_dir.join("stale_behavior_case.json");
    fs::write(
        &stale_request,
        r#"{
  "behavior_case": {
    "case_id": "software_authoring.stale",
    "title": "Stale embedded tooling field",
    "context": {
      "context_id": "bounded-context:stale",
      "label": "Stale"
    }
  }
}
"#,
    )
    .expect("write stale request");

    let output = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("behavior-case")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--request")
        .arg(&stale_request)
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .output()
        .expect("run stale behavior-case");
    assert!(
        !output.status.success(),
        "stale embedded behavior-case fields should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown field `context`") || stderr.contains("context"),
        "expected stale schema error mentioning context, got: {stderr}"
    );
}

#[test]
fn software_authoring_runtime_theory_check_reports_closure_trace() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let output = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("check")
        .arg("theory")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--closure-tier")
        .arg("evidence_weighted")
        .arg("--world-id")
        .arg("review:order-fulfillment")
        .arg("--evidence-threshold-ppm")
        .arg("700000")
        .arg("--weighted-evidence")
        .output()
        .expect("run software-authoring runtime theory check");

    assert!(
        output.status.success(),
        "software-authoring runtime theory check failed (exit={}) stderr={}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("closure trace:"),
        "expected human summary to expose closure trace, got: {stdout}"
    );
    assert!(
        stdout.contains("anchor: module_digest=fnv1a64:"),
        "expected human summary to expose canonical module digest, got: {stdout}"
    );
    assert!(
        stdout.contains("worlds=review:order-fulfillment (finite)"),
        "expected human summary to expose declared world, got: {stdout}"
    );
    assert!(
        stdout.contains("evidence_policies=thresholded_world_default:700000ppm"),
        "expected human summary to expose evidence threshold, got: {stdout}"
    );
    assert!(
        stdout.contains("ontology_closed=true"),
        "expected ontology closure claim under declared assumptions, got: {stdout}"
    );
}

#[test]
fn typecheck_cert_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "typecheck_cert");
    let cert_path = run_dir.join("build/typecheck_cert.json");

    let input = repo_root.join("examples/economics/EconomicFlows.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("typecheck")
        .arg(&input)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert typecheck");

    assert!(
        status.success(),
        "typecheck-cert failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read typecheck cert json");
    let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse typecheck cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor.axi_digest_v1.as_str().starts_with("fnv1a64:"),
        "unexpected digest format: {}",
        anchor.axi_digest_v1
    );

    match cert.payload {
        CertificatePayloadV2::AxiWellTypedV1 { proof } => {
            assert_eq!(proof.module_name, "EconomicFlows");
            assert!(proof.schema_count >= 1);
        }
        other => panic!("expected axi_well_typed_v1 certificate, got {other:?}"),
    }
}

#[test]
fn constraints_cert_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "constraints_cert");
    let cert_path = run_dir.join("build/constraints_cert.json");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("constraints")
        .arg(&input)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert constraints");

    assert!(
        status.success(),
        "constraints-cert failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read constraints cert json");
    let cert: CertificateV2 =
        serde_json::from_str(&cert_text).expect("parse constraints cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor.axi_digest_v1.as_str().starts_with("fnv1a64:"),
        "unexpected digest format: {}",
        anchor.axi_digest_v1
    );

    match cert.payload {
        CertificatePayloadV2::AxiConstraintsOkV1 { proof } => {
            assert_eq!(proof.module_name, "OntologyRewrites");
            assert!(proof.constraint_count >= 1);
            assert!(proof.instance_count >= 1);
            assert!(proof.check_count >= 1);
        }
        other => panic!("expected axi_constraints_ok_v1 certificate, got {other:?}"),
    }
}

#[test]
fn canonical_only_cert_commands_reject_pathdb_export_snapshots() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "canonical_only_certs_reject_snapshot");
    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let axpd = run_dir.join("build/snapshot.axpd");
    let export_axi = run_dir.join("build/snapshot_export.axi");

    let import_status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("import-axi")
        .arg(&input)
        .arg("--out")
        .arg(&axpd)
        .status()
        .expect("run axiograph db pathdb import-axi");
    assert!(
        import_status.success(),
        "db pathdb import-axi failed (exit={})",
        import_status.code().unwrap_or(-1)
    );

    let export_status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("export-axi")
        .arg(&axpd)
        .arg("--out")
        .arg(&export_axi)
        .status()
        .expect("run axiograph db pathdb export-axi");
    assert!(
        export_status.success(),
        "db pathdb export-axi failed (exit={})",
        export_status.code().unwrap_or(-1)
    );

    let expected = "expected a canonical .axi module, but input is a PathDBExportV1 snapshot";

    let typecheck = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("typecheck")
        .arg(&export_axi)
        .output()
        .expect("run axiograph cert typecheck on snapshot export");
    assert!(
        !typecheck.status.success(),
        "expected cert typecheck to reject PathDBExportV1 snapshot"
    );
    let typecheck_stderr = String::from_utf8_lossy(&typecheck.stderr);
    assert!(
        typecheck_stderr.contains(expected),
        "expected typecheck stderr to mention canonical-only rejection, got: {typecheck_stderr}"
    );

    let constraints = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("constraints")
        .arg(&export_axi)
        .output()
        .expect("run axiograph cert constraints on snapshot export");
    assert!(
        !constraints.status.success(),
        "expected cert constraints to reject PathDBExportV1 snapshot"
    );
    let constraints_stderr = String::from_utf8_lossy(&constraints.stderr);
    assert!(
        constraints_stderr.contains(expected),
        "expected constraints stderr to mention canonical-only rejection, got: {constraints_stderr}"
    );

    let query = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("query")
        .arg(&export_axi)
        .arg("--lang")
        .arg("axql")
        .arg("select ?x where ?x : Node limit 1")
        .output()
        .expect("run axiograph cert query on snapshot export");
    assert!(
        !query.status.success(),
        "expected cert query to reject PathDBExportV1 snapshot"
    );
    let query_stderr = String::from_utf8_lossy(&query.stderr);
    assert!(
        query_stderr.contains(expected),
        "expected query stderr to mention canonical-only rejection, got: {query_stderr}"
    );

    let viz = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("tools")
        .arg("viz")
        .arg(&export_axi)
        .arg("--out")
        .arg(run_dir.join("build/snapshot.dot"))
        .output()
        .expect("run axiograph tools viz on snapshot export");
    assert!(
        !viz.status.success(),
        "expected generic tools viz loading to reject PathDBExportV1 snapshot"
    );
    let viz_stderr = String::from_utf8_lossy(&viz.stderr);
    assert!(
        viz_stderr.contains("generic semantic/query/cert commands only accept canonical .axi modules"),
        "expected viz stderr to mention generic canonical-only rejection, got: {viz_stderr}"
    );
}

#[test]
fn accept_promote_rejects_pathdb_export_snapshot_without_mutating_store() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "accept_promote_rejects_snapshot");
    let accepted_dir = run_dir.join("build/accepted_plane");
    fs::create_dir_all(&accepted_dir).expect("create accepted dir");

    let base_axi = run_dir.join("build/Base.axi");
    fs::write(
        &base_axi,
        r#"module Base

schema Base:
  object Seed

instance BaseInst of Base:
  Seed = {seed0}
"#,
    )
    .expect("write base module");

    let base_promote = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&base_axi)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("test: base snapshot")
        .status()
        .expect("run base accept promote");
    assert!(
        base_promote.success(),
        "base accept promote failed (exit={})",
        base_promote.code().unwrap_or(-1)
    );

    let head_before = fs::read_to_string(accepted_dir.join("HEAD")).expect("read HEAD before");
    let log_before = fs::read_to_string(accepted_dir.join("accepted_plane.log.jsonl"))
        .expect("read accepted plane log before");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let axpd = run_dir.join("build/snapshot.axpd");
    let export_axi = run_dir.join("build/snapshot_export.axi");

    let import_status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("import-axi")
        .arg(&input)
        .arg("--out")
        .arg(&axpd)
        .status()
        .expect("run axiograph db pathdb import-axi");
    assert!(
        import_status.success(),
        "db pathdb import-axi failed (exit={})",
        import_status.code().unwrap_or(-1)
    );

    let export_status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("pathdb")
        .arg("export-axi")
        .arg(&axpd)
        .arg("--out")
        .arg(&export_axi)
        .status()
        .expect("run axiograph db pathdb export-axi");
    assert!(
        export_status.success(),
        "db pathdb export-axi failed (exit={})",
        export_status.code().unwrap_or(-1)
    );

    let promote = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&export_axi)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("test: should reject snapshot export")
        .output()
        .expect("run accept promote on snapshot export");
    assert!(
        !promote.status.success(),
        "expected accept promote to reject PathDBExportV1 snapshot"
    );
    let stderr = String::from_utf8_lossy(&promote.stderr);
    assert!(
        stderr.contains("snapshot")
            || stderr.contains("PathDBExportV1")
            || stderr.contains("unsupported"),
        "expected stderr to mention snapshot/canonical rejection, got: {stderr}"
    );

    let head_after = fs::read_to_string(accepted_dir.join("HEAD")).expect("read HEAD after");
    assert_eq!(
        head_after, head_before,
        "HEAD should not advance when promote rejects a snapshot export"
    );

    let log_after = fs::read_to_string(accepted_dir.join("accepted_plane.log.jsonl"))
        .expect("read accepted plane log after");
    assert_eq!(
        log_after, log_before,
        "accepted_plane.log.jsonl should not gain a new event on rejected promote"
    );
}

#[test]
fn pathdb_wal_import_proposals_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "pathdb_wal_proposals");
    let out_dir = run_dir.join("build");
    let accepted_dir = out_dir.join("accepted_plane");
    fs::create_dir_all(&accepted_dir).expect("create accepted dir");

    // ---------------------------------------------------------------------
    // A) Create a tiny accepted-plane base snapshot (canonical meaning plane).
    // ---------------------------------------------------------------------
    let base_axi = out_dir.join("WalBase.axi");
    fs::write(
        &base_axi,
        r#"module WalBase

schema WalBase:
  object Dummy

instance WalBaseInst of WalBase:
  Dummy = {dummy0}
"#,
    )
    .expect("write base module");

    let promote = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&base_axi)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("test: base snapshot")
        .output()
        .expect("run promote");
    assert!(
        promote.status.success(),
        "promote failed: {}",
        String::from_utf8_lossy(&promote.stderr)
    );
    let accepted_snapshot_id = String::from_utf8_lossy(&promote.stdout).trim().to_string();
    assert!(
        !accepted_snapshot_id.is_empty(),
        "expected promote to print snapshot id"
    );

    // ---------------------------------------------------------------------
    // B) Ingest RDF TriG fixture → proposals.json.
    // ---------------------------------------------------------------------
    let fixture_dir = repo_root.join("examples/rdfowl/named_graphs_minimal");
    let ingest = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("ingest")
        .arg("dir")
        .arg(&fixture_dir)
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--domain")
        .arg("rdfowl")
        .output()
        .expect("run ingest dir");
    assert!(
        ingest.status.success(),
        "ingest failed: {}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let proposals_path = out_dir.join("proposals.json");
    assert!(proposals_path.exists(), "expected proposals.json");

    // ---------------------------------------------------------------------
    // C) Commit proposals.json into the PathDB WAL and checkout .axpd.
    // ---------------------------------------------------------------------
    let commit = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-commit")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--accepted-snapshot")
        .arg(&accepted_snapshot_id)
        .arg("--proposals")
        .arg(&proposals_path)
        .arg("--message")
        .arg("test: preserve proposals")
        .output()
        .expect("run pathdb-commit");
    assert!(
        commit.status.success(),
        "pathdb-commit failed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );
    let wal_snapshot_id = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    assert!(!wal_snapshot_id.is_empty(), "expected WAL snapshot id");

    let axpd = out_dir.join("evidence_plane.axpd");
    let build = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-build")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg(&wal_snapshot_id)
        .arg("--out")
        .arg(&axpd)
        .output()
        .expect("run pathdb-build");
    assert!(
        build.status.success(),
        "pathdb-build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // ---------------------------------------------------------------------
    // D) Validate that evidence-plane data was preserved in PathDB.
    // ---------------------------------------------------------------------
    let bytes = fs::read(&axpd).expect("read axpd");
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse axpd");

    // Resolve resource entities by `name`.
    let name_key = db.interner.id_of("name").expect("interned name key");
    let a_name = db.interner.id_of("a").expect("interned a");
    let b_name = db.interner.id_of("b").expect("interned b");
    let c_name = db.interner.id_of("c").expect("interned c");
    let g_plan_name = db.interner.id_of("g_plan").expect("interned g_plan");
    let g_observed_name = db
        .interner
        .id_of("g_observed")
        .expect("interned g_observed");

    let a_id = db
        .entities
        .entities_with_attr_value(name_key, a_name)
        .iter()
        .next()
        .expect("entity named a");
    let b_id = db
        .entities
        .entities_with_attr_value(name_key, b_name)
        .iter()
        .next()
        .expect("entity named b");
    let c_id = db
        .entities
        .entities_with_attr_value(name_key, c_name)
        .iter()
        .next()
        .expect("entity named c");

    let g_plan_id = db
        .entities
        .entities_with_attr_value(name_key, g_plan_name)
        .iter()
        .next()
        .expect("context named g_plan");
    let g_observed_id = db
        .entities
        .entities_with_attr_value(name_key, g_observed_name)
        .iter()
        .next()
        .expect("context named g_observed");

    // Ensure `iri` attribute is preserved for `a`.
    let iri_key = db.interner.id_of("iri").expect("interned iri key");
    let iri_val = db
        .interner
        .id_of("http://example.org/a")
        .expect("interned a iri");
    assert_eq!(
        db.entities.get_attr(a_id, iri_key),
        Some(iri_val),
        "expected entity `a` to preserve iri attribute"
    );

    // Find `knows` fact nodes and confirm they are correctly scoped per context.
    let axi_relation_key = db
        .interner
        .id_of(axiograph_pathdb::axi_meta::ATTR_AXI_RELATION)
        .expect("interned axi_relation key");
    let knows_val = db.interner.id_of("knows").expect("interned knows");
    let knows_facts = db
        .entities
        .entities_with_attr_value(axi_relation_key, knows_val);
    assert!(!knows_facts.is_empty(), "expected knows fact nodes");

    let from_rel = db.interner.id_of("from").expect("interned from");
    let to_rel = db.interner.id_of("to").expect("interned to");
    let in_ctx_rel = db
        .interner
        .id_of(axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT)
        .expect("interned axi_fact_in_context");

    let mut saw_plan = false;
    let mut saw_observed = false;
    for f in knows_facts.iter() {
        let has_from_a = db.relations.has_edge(f, from_rel, a_id);
        if !has_from_a {
            continue;
        }
        let has_ctx_plan = db.relations.has_edge(f, in_ctx_rel, g_plan_id);
        let has_ctx_observed = db.relations.has_edge(f, in_ctx_rel, g_observed_id);

        if has_ctx_plan && db.relations.has_edge(f, to_rel, b_id) {
            saw_plan = true;
        }
        if has_ctx_observed && db.relations.has_edge(f, to_rel, c_id) {
            saw_observed = true;
        }
    }

    assert!(saw_plan, "expected g_plan to assert knows(a,b)");
    assert!(saw_observed, "expected g_observed to assert knows(a,c)");
}

#[test]
fn analyze_network_and_quality_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "analyze_quality");
    let net_path = run_dir.join("build/network.json");
    let quality_path = run_dir.join("build/quality.json");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("tools")
        .arg("analyze")
        .arg("network")
        .arg(&input)
        .arg("--plane")
        .arg("both")
        .arg("--skip-facts")
        .arg("--communities")
        .arg("--format")
        .arg("json")
        .arg("--out")
        .arg(&net_path)
        .status()
        .expect("run axiograph tools analyze network");
    assert!(
        status.success(),
        "analyze network failed (exit={})",
        status.code().unwrap_or(-1)
    );
    let net_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&net_path).expect("read network report"))
            .expect("parse network report json");
    assert_eq!(net_json["version"], "network_analysis_v1");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("check")
        .arg("quality")
        .arg(&input)
        .arg("--plane")
        .arg("both")
        .arg("--profile")
        .arg("strict")
        .arg("--format")
        .arg("json")
        .arg("--no-fail")
        .arg("--out")
        .arg(&quality_path)
        .status()
        .expect("run axiograph check quality");
    assert!(
        status.success(),
        "quality failed (exit={})",
        status.code().unwrap_or(-1)
    );
    let q_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&quality_path).expect("read quality report"))
            .expect("parse quality report json");
    assert_eq!(q_json["version"], "quality_report_v1");
}

#[test]
fn accepted_plane_promote_and_build_pathdb_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "accepted_plane");
    let accepted_dir = run_dir.join("build/accepted_plane");
    let out_axpd = run_dir.join("build/accepted_plane.axpd");

    let input = repo_root.join("examples/economics/EconomicFlows.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e smoke: accept promote")
        .status()
        .expect("run axiograph db accept promote");

    assert!(
        status.success(),
        "accept promote failed for `{}` (exit={})",
        input.display(),
        status.code().unwrap_or(-1)
    );

    let snapshot_id = fs::read_to_string(accepted_dir.join("HEAD"))
        .expect("read accepted plane HEAD")
        .trim()
        .to_string();
    assert!(
        !snapshot_id.is_empty(),
        "expected accepted plane HEAD snapshot id"
    );

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("build-pathdb")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg("latest")
        .arg("--out")
        .arg(&out_axpd)
        .status()
        .expect("run axiograph db accept build-pathdb");

    assert!(
        status.success(),
        "accept build-pathdb failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let bytes = fs::read(&out_axpd).expect("read rebuilt axpd");
    assert!(bytes.len() > 64, "expected non-empty axpd output");

    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse rebuilt axpd");
    assert!(
        !db.entities.is_empty(),
        "expected non-empty PathDB entities after rebuild"
    );

    // Grounding always has evidence: accepted-plane builds embed `.axi` module
    // source as DocChunks so LLM/UI flows can cite and open it.
    let has_chunks = db
        .find_by_type("DocChunk")
        .map(|bm| !bm.is_empty())
        .unwrap_or(false);
    assert!(
        has_chunks,
        "expected at least one DocChunk in accepted build"
    );
}

#[test]
fn accepted_plane_pathdb_wal_commit_and_build_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "pathdb_wal");
    let accepted_dir = run_dir.join("build/accepted_plane");
    let out_axpd = run_dir.join("build/pathdb_wal.axpd");

    // 1) Create an accepted-plane snapshot (canonical `.axi` is the anchor).
    let input = repo_root.join("examples/economics/EconomicFlows.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--message")
        .arg("e2e smoke: accept promote (pathdb wal)")
        .status()
        .expect("run axiograph db accept promote");
    assert!(
        status.success(),
        "accept promote failed for `{}` (exit={})",
        input.display(),
        status.code().unwrap_or(-1)
    );

    // 2) Commit an extension-layer overlay (chunks.json) into the PathDB WAL.
    let chunks_path = run_dir.join("build/chunks.json");
    let chunks: Vec<Chunk> = vec![Chunk {
        chunk_id: "chunk0".to_string(),
        document_id: "doc0.txt".to_string(),
        page: None,
        span_id: "span0".to_string(),
        text: "EconomicFlows mentions Household_A and Firm_A".to_string(),
        bbox: None,
        metadata: HashMap::new(),
    }];
    fs::write(
        &chunks_path,
        serde_json::to_string_pretty(&chunks).expect("serialize chunks"),
    )
    .expect("write chunks.json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-commit")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--accepted-snapshot")
        .arg("latest")
        .arg("--chunks")
        .arg(&chunks_path)
        .arg("--message")
        .arg("e2e smoke: pathdb wal commit")
        .status()
        .expect("run axiograph db accept pathdb-commit");
    assert!(
        status.success(),
        "accept pathdb-commit failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let pathdb_head = fs::read_to_string(accepted_dir.join("pathdb").join("HEAD"))
        .expect("read pathdb wal HEAD")
        .trim()
        .to_string();
    assert!(
        !pathdb_head.is_empty(),
        "expected non-empty pathdb wal HEAD"
    );

    // 3) Check out the `.axpd` from the WAL snapshot.
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("pathdb-build")
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--snapshot")
        .arg("latest")
        .arg("--out")
        .arg(&out_axpd)
        .status()
        .expect("run axiograph db accept pathdb-build");
    assert!(
        status.success(),
        "accept pathdb-build failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let bytes = fs::read(&out_axpd).expect("read pathdb wal axpd");
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse pathdb wal axpd");

    let chunk_id_key = db.interner.id_of("chunk_id").expect("chunk_id attr key id");
    let want = db.interner.id_of("chunk0").expect("chunk0 value id");
    let mut found = false;
    if let Some(chunks) = db.find_by_type("DocChunk") {
        for id in chunks.iter() {
            if db.entities.get_attr(id, chunk_id_key) == Some(want) {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "expected committed DocChunk chunk_id=chunk0 after wal commit"
    );
}

#[test]
fn accepted_plane_promote_with_quality_report_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "accepted_plane_quality");
    let accepted_dir = run_dir.join("build/accepted_plane");

    let input = repo_root.join("examples/ontology/OntologyRewrites.axi");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(&input)
        .arg("--dir")
        .arg(&accepted_dir)
        .arg("--quality")
        .arg("strict")
        .arg("--message")
        .arg("e2e smoke: accept promote (quality)")
        .status()
        .expect("run axiograph db accept promote --quality strict");

    assert!(
        status.success(),
        "accept promote --quality strict failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let log_path = accepted_dir.join("accepted_plane.log.jsonl");
    let log = fs::read_to_string(&log_path).expect("read accepted plane log");
    let last_line = log
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .expect("expected at least one log line");
    let event: serde_json::Value = serde_json::from_str(last_line).expect("parse event json");

    let rel = event["quality_report_path"]
        .as_str()
        .expect("expected quality_report_path on event");
    let report_path = accepted_dir.join(rel);
    assert!(
        report_path.exists(),
        "expected stored quality report at `{}`",
        report_path.display()
    );
}

#[test]
fn repl_scripts_canonical_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let scripts_root = repo_root.join("examples/repl_scripts");
    let mut scripts: Vec<PathBuf> = WalkDir::new(&scripts_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().map(|s| s == "repl").unwrap_or(false))
        .collect();
    scripts.sort();

    assert!(
        !scripts.is_empty(),
        "expected `.repl` scripts under examples/repl_scripts/"
    );

    for script in scripts {
        let label = script
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "script".to_string());
        let run_dir = unique_run_dir(&repo_root, &label);

        let status = Command::new(&bin)
            .current_dir(&run_dir)
            .arg("repl")
            .arg("--script")
            .arg(&script)
            .arg("--quiet")
            .status()
            .expect("run axiograph repl --script");
        assert!(
            status.success(),
            "repl script `{}` failed (exit={})",
            script.display(),
            status.code().unwrap_or(-1)
        );

        let build_dir = run_dir.join("build");
        let mut stale_exports: Vec<PathBuf> = fs::read_dir(&build_dir)
            .expect("read build dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|s| s == "axi").unwrap_or(false)
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.ends_with("_export_v1.axi"))
                        .unwrap_or(false)
            })
            .collect();
        stale_exports.sort();

        assert!(
            stale_exports.is_empty(),
            "REPL script `{}` should not emit PathDBExportV1 teaching snapshots: {:?}",
            script.display(),
            stale_exports
        );

        // If this REPL script imported a canonical `.axi` module (meta-plane),
        // we should be able to export it back as a canonical module from the `.axpd`.
        let mut module_exports: Vec<PathBuf> = fs::read_dir(&build_dir)
            .expect("read build dir for module exports")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|s| s == "axi").unwrap_or(false)
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains("_module"))
                        .unwrap_or(false)
            })
            .collect();
        module_exports.sort();

        for module_out in module_exports {
            let text = fs::read_to_string(&module_out).expect("read exported module .axi");
            let m = parse_axi_v1(&text).expect("parse exported module via axi_v1");
            assert_eq!(
                m.module_name.is_empty(),
                false,
                "expected non-empty module name in exported module"
            );
        }
    }
}

#[test]
fn repl_rejects_stale_export_axi_command() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "repl_rejects_stale_export_axi");

    let output = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("repl")
        .arg("--quiet")
        .arg("--cmd")
        .arg("export_axi build/stale_export_v1.axi")
        .output()
        .expect("run axiograph repl stale export_axi");

    assert!(
        !output.status.success(),
        "stale REPL export_axi command should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown command `export_axi`"),
        "expected stale export_axi guidance, got: {stderr}"
    );
}

#[test]
fn discover_augment_proposals_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "augment_proposals");
    let in_path = run_dir.join("build/in_proposals.json");
    let out_path = run_dir.join("build/out_proposals.json");
    let trace_path = run_dir.join("build/augment_trace.json");

    let mut mention_attrs = HashMap::new();
    mention_attrs.insert("role".to_string(), "material".to_string());
    mention_attrs.insert("domain".to_string(), "machining".to_string());
    mention_attrs.insert("value".to_string(), "Titanium".to_string());

    let input = ProposalsFileV1 {
        version: 1,
        generated_at: "0".to_string(),
        source: ProposalSourceV1 {
            source_type: "test".to_string(),
            locator: "discover_augment_proposals_smoke".to_string(),
        },
        schema_hint: None,
        proposals: vec![ProposalV1::Entity {
            meta: ProposalMetaV1 {
                proposal_id: "mention::fact0::material".to_string(),
                confidence: 0.9,
                evidence: vec![EvidencePointer {
                    chunk_id: "chunk0".to_string(),
                    locator: None,
                    span_id: None,
                }],
                public_rationale: "test mention".to_string(),
                metadata: HashMap::new(),
                schema_hint: None,
            },
            entity_id: "mention::fact0::material".to_string(),
            entity_type: "Mention".to_string(),
            name: "Titanium".to_string(),
            attributes: mention_attrs,
            description: None,
        }],
    };

    fs::write(
        &in_path,
        serde_json::to_string_pretty(&input).expect("serialize proposals"),
    )
    .expect("write proposals json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("augment-proposals")
        .arg(&in_path)
        .arg("--out")
        .arg(&out_path)
        .arg("--trace")
        .arg(&trace_path)
        .status()
        .expect("run axiograph discover augment-proposals");

    assert!(
        status.success(),
        "discover augment-proposals failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let out_text = fs::read_to_string(&out_path).expect("read out proposals json");
    let out: ProposalsFileV1 = serde_json::from_str(&out_text).expect("parse out proposals json");

    assert!(
        out.proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Entity { entity_id, entity_type, .. }
                if entity_type == "Role" && entity_id == "role::material"
        )),
        "expected derived Role entity"
    );

    assert!(
        out.proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Relation { rel_type, .. } if rel_type == "HasRole"
        )),
        "expected derived HasRole relation"
    );

    assert!(
        out.proposals.iter().any(|p| matches!(
            p,
            ProposalV1::Entity { meta, entity_id, .. }
                if entity_id == "mention::fact0::material"
                    && meta.schema_hint.as_deref() == Some("machinist_learning")
        )),
        "expected inferred schema_hint on Mention"
    );
}

#[test]
fn discover_draft_module_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "draft_module");
    let proposals_path = run_dir.join("build/proposals.json");
    let out_axi = run_dir.join("build/discovered.proposals.axi");

    let file = ProposalsFileV1 {
        version: 1,
        generated_at: "0".to_string(),
        source: ProposalSourceV1 {
            source_type: "test".to_string(),
            locator: "discover_draft_module_smoke".to_string(),
        },
        schema_hint: Some("sql".to_string()),
        proposals: vec![
            ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_table::Users".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test table".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: "sql_table::Users".to_string(),
                entity_type: "SqlTable".to_string(),
                name: "Users".to_string(),
                attributes: HashMap::new(),
                description: None,
            },
            ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_column::Users::id".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: "sql_column::Users::id".to_string(),
                entity_type: "SqlColumn".to_string(),
                name: "Users.id".to_string(),
                attributes: HashMap::new(),
                description: None,
            },
            ProposalV1::Entity {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_column::Users::name".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                entity_id: "sql_column::Users::name".to_string(),
                entity_type: "SqlColumn".to_string(),
                name: "Users.name".to_string(),
                attributes: HashMap::new(),
                description: None,
            },
            // One table has multiple columns: not functional `from -> to`, but functional `to -> from`.
            ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_rel::has_column::Users::id".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test has_column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                relation_id: "sql_rel::has_column::Users::id".to_string(),
                rel_type: "SqlHasColumn".to_string(),
                source: "sql_table::Users".to_string(),
                target: "sql_column::Users::id".to_string(),
                attributes: HashMap::new(),
            },
            ProposalV1::Relation {
                meta: ProposalMetaV1 {
                    proposal_id: "sql_rel::has_column::Users::name".to_string(),
                    confidence: 1.0,
                    evidence: vec![],
                    public_rationale: "test has_column".to_string(),
                    metadata: HashMap::new(),
                    schema_hint: Some("sql".to_string()),
                },
                relation_id: "sql_rel::has_column::Users::name".to_string(),
                rel_type: "SqlHasColumn".to_string(),
                source: "sql_table::Users".to_string(),
                target: "sql_column::Users::name".to_string(),
                attributes: HashMap::new(),
            },
        ],
    };

    fs::write(
        &proposals_path,
        serde_json::to_string_pretty(&file).expect("serialize proposals"),
    )
    .expect("write proposals file");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("draft-module")
        .arg(&proposals_path)
        .arg("--out")
        .arg(&out_axi)
        .arg("--infer-constraints")
        .status()
        .expect("run axiograph discover draft-module");

    assert!(
        status.success(),
        "draft-module failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let text = fs::read_to_string(&out_axi).expect("read drafted .axi");
    let module = parse_axi_v1(&text).expect("parse drafted module");
    assert_eq!(module.module_name, "Discovered");

    // Extensional inference should have included a key and the functional `to -> from`.
    let theory = module
        .theories
        .iter()
        .find(|t| t.name == "DiscoveredExtensional")
        .expect("expected extensional theory");

    use axiograph_dsl::schema_v1::ConstraintV1;
    assert!(
        theory.constraints.iter().any(|c| matches!(
            c,
            ConstraintV1::Key { relation, fields } if relation == "SqlHasColumn" && fields == &vec!["from".to_string(), "to".to_string()]
        )),
        "expected key(SqlHasColumn(from,to))"
    );
    assert!(
        theory.constraints.iter().any(|c| matches!(
            c,
            ConstraintV1::Functional { relation, src_field, dst_field }
                if relation == "SqlHasColumn" && src_field == "to" && dst_field == "from"
        )),
        "expected functional SqlHasColumn.to -> SqlHasColumn.from"
    );
}

#[test]
fn discover_transport_preview_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "transport_preview");
    let input_axi = run_dir.join("build/plant.axi");
    let morphism_path = run_dir.join("build/morphism.json");
    let preview_path = run_dir.join("build/transport_preview.json");
    let applied_preview_path = run_dir.join("build/transport_preview_applied.json");

    fs::write(
        &input_axi,
        r#"
module Plant

schema Plant:
  object PlantAsset
  object Pump
  object Compressor
  object Context
  relation installed_at(asset: PlantAsset, site: PlantAsset, ctx: Context)
  subtype Pump < PlantAsset
  subtype Compressor < PlantAsset

theory PlantTransport on Plant:
  constraint key installed_at(asset, site, ctx)
"#,
    )
    .expect("write input axi");
    fs::write(
        &morphism_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "source_schema": "Plant",
            "target_schema": "Ops",
            "objects": [
                {"source_object": "PlantAsset", "target_object": "Equipment"},
                {"source_object": "Pump", "target_object": "Equipment"},
                {"source_object": "Compressor", "target_object": "Equipment"}
            ],
            "arrows": [
                {"source_arrow": "installed_at", "target_path": ["owned_by", "located_at"]}
            ]
        }))
        .expect("serialize morphism json"),
    )
    .expect("write morphism json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("transport-preview")
        .arg(&input_axi)
        .arg("--morphism")
        .arg(&morphism_path)
        .arg("--schema")
        .arg("Plant")
        .arg("--out")
        .arg(&preview_path)
        .status()
        .expect("run axiograph discover transport-preview");

    assert!(
        status.success(),
        "transport-preview failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let preview: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&preview_path).expect("read transport preview"))
            .expect("parse transport preview json");
    assert_eq!(preview["kind"], "migration_preview");
    let handle_id = preview["refinement_candidates"]
        .as_array()
        .and_then(|candidates| candidates.first())
        .and_then(|candidate| candidate.get("handle"))
        .and_then(|handle| handle.get("id"))
        .and_then(|id| id.as_str())
        .map(str::to_string)
        .expect("expected migration refinement handle id");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("transport-preview")
        .arg(&input_axi)
        .arg("--morphism")
        .arg(&morphism_path)
        .arg("--schema")
        .arg("Plant")
        .arg("--apply-refinement-handle-id")
        .arg(&handle_id)
        .arg("--out")
        .arg(&applied_preview_path)
        .status()
        .expect("run axiograph discover transport-preview with refinement");

    assert!(
        status.success(),
        "transport-preview with refinement failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let applied_preview: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&applied_preview_path).expect("read applied transport preview"),
    )
    .expect("parse applied transport preview json");
    assert_eq!(applied_preview["kind"], "migration_preview");
    assert_eq!(applied_preview["ok"], true);
    assert!(
        applied_preview.get("residual_obligations").is_none()
            || applied_preview["residual_obligations"]
                .as_array()
                .is_some_and(|obligations| obligations.is_empty())
    );
    assert!(
        applied_preview.get("refinement_candidates").is_none()
            || applied_preview["refinement_candidates"]
                .as_array()
                .is_some_and(|candidates| candidates.is_empty())
    );
}

#[test]
fn discover_route_preview_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "route_preview");
    let axpd_path = run_dir.join("build/route_preview.axpd");
    let request_path = run_dir.join("build/route_request.json");
    let preview_path = run_dir.join("build/route_preview.json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("repl")
        .arg("--cmd")
        .arg("gen scenario social_network 3 3 1")
        .arg("--cmd")
        .arg(format!("save {}", axpd_path.display()))
        .arg("--quiet")
        .status()
        .expect("run axiograph repl route setup");

    assert!(
        status.success(),
        "route preview setup failed (exit={})",
        status.code().unwrap_or(-1)
    );

    fs::write(
        &request_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "route": {
                "start_entity": 0,
                "segments": []
            },
            "equivalent_to": {
                "start_entity": 0,
                "segments": []
            }
        }))
        .expect("serialize route preview request"),
    )
    .expect("write route preview request");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("route-preview")
        .arg(&axpd_path)
        .arg("--request")
        .arg(&request_path)
        .arg("--out")
        .arg(&preview_path)
        .status()
        .expect("run axiograph discover route-preview");

    assert!(
        status.success(),
        "route-preview failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let preview: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&preview_path).expect("read route preview"))
            .expect("parse route preview json");
    assert_eq!(preview["version"], "axiograph_discover_route_preview_v1");
    assert_eq!(preview["trust"]["trust_class"], "runtime_guarded");
    assert_eq!(preview["equivalence"]["equivalent"], true);
    assert!(
        preview["route"]["normalized"].get("hops").is_none()
            || preview["route"]["normalized"]["hops"]
                .as_array()
                .is_some_and(|hops| hops.is_empty())
    );
    assert!(preview.get("certificate_preview").is_none());
}

#[test]
fn viz_dot_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "viz_dot");
    let axpd_path = run_dir.join("build/viz.axpd");
    let dot_path = run_dir.join("build/viz.dot");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("repl")
        .arg("--cmd")
        .arg("gen scenario social_network 3 3 1")
        .arg("--cmd")
        .arg(format!("save {}", axpd_path.display()))
        .arg("--quiet")
        .status()
        .expect("run axiograph repl --cmd ...");

    assert!(
        status.success(),
        "repl gen/save failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("tools")
        .arg("viz")
        .arg(&axpd_path)
        .arg("--out")
        .arg(&dot_path)
        .arg("--focus-name")
        .arg("Alice_0")
        .arg("--hops")
        .arg("2")
        .arg("--max-nodes")
        .arg("120")
        .status()
        .expect("run axiograph tools viz");

    assert!(
        status.success(),
        "viz failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let dot = fs::read_to_string(&dot_path).expect("read dot");
    assert!(
        dot.contains("digraph axiograph"),
        "expected dot output to contain graph header"
    );
    assert!(
        dot.contains("Alice_0") || dot.contains("Person"),
        "expected dot output to contain some node labels"
    );
}

#[test]
fn querycert_rejects_pathdb_export_snapshot_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "anchor_snapshot_export");

    let input = repo_root.join("examples/anchors/pathdb_export_anchor_v1.axi");
    let cert_path = run_dir.join("build/anchor_query_cert.json");

    let query = "select ?y where name(\"a\") -r1-> ?y limit 10";

    let output = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("query")
        .arg(&input)
        .arg("--lang")
        .arg("axql")
        .arg(query)
        .arg("--out")
        .arg(&cert_path)
        .output()
        .expect("run axiograph cert query (anchor snapshot)");
    assert!(
        !output.status.success(),
        "expected querycert to reject anchor snapshot export (exit={})",
        output.status.code().unwrap_or(-1)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("expected a canonical .axi module"),
        "expected canonical-only rejection, got: {stderr}"
    );
    assert!(
        !cert_path.exists(),
        "querycert should not write output on canonical-only rejection"
    );
}

#[test]
fn querycert_canonical_axi_v3_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "querycert_canonical_v3");

    let input = repo_root.join("examples/manufacturing/SupplyChainHoTT.axi");
    let cert_path = run_dir.join("build/canonical_query_cert_v3.json");

    let query = "select ?to where name(\"RawMetal_A\") -Flow-> ?to limit 10";

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("query")
        .arg(&input)
        .arg("--lang")
        .arg("axql")
        .arg(query)
        .arg("--out")
        .arg(&cert_path)
        .status()
        .expect("run axiograph cert query (canonical .axi)");
    assert!(
        status.success(),
        "querycert on canonical module failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let cert_text = fs::read_to_string(&cert_path).expect("read query cert json");
    let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse query cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor.axi_digest_v1.as_str().starts_with("fnv1a64:"),
        "unexpected digest format: {}",
        anchor.axi_digest_v1
    );

    match cert.payload {
        CertificatePayloadV2::QueryResultV3 { proof } => {
            assert!(
                !proof.rows.is_empty(),
                "expected non-empty rows for canonical module query"
            );
        }
        other => panic!("expected query_result_v3 certificate, got {other:?}"),
    }
}

#[test]
fn doc_to_proposals_to_candidate_axi_smoke() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "doc_to_candidates");
    let build_dir = run_dir.join("build");

    let input = repo_root.join("examples/docs/sample_conversation.txt");
    let proposals_path = build_dir.join("proposals.json");
    let chunks_path = build_dir.join("chunks.json");
    let facts_path = build_dir.join("facts.json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("doc")
        .arg(&input)
        .arg("--out")
        .arg(&proposals_path)
        .arg("--chunks")
        .arg(&chunks_path)
        .arg("--facts")
        .arg(&facts_path)
        .arg("--machining")
        .status()
        .expect("run axiograph doc");
    assert!(
        status.success(),
        "doc ingestion failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let proposals_text = fs::read_to_string(&proposals_path).expect("read proposals.json");
    let proposals: ProposalsFileV1 =
        serde_json::from_str(&proposals_text).expect("parse proposals.json");
    assert!(
        !proposals.proposals.is_empty(),
        "expected non-empty proposals from sample conversation"
    );

    let candidates_dir = build_dir.join("candidates");
    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("discover")
        .arg("promote-proposals")
        .arg(&proposals_path)
        .arg("-o")
        .arg(&candidates_dir)
        .arg("--domains")
        .arg("machinist_learning")
        .status()
        .expect("run axiograph discover promote-proposals");
    assert!(
        status.success(),
        "promotion failed (exit={})",
        status.code().unwrap_or(-1)
    );

    let candidate_axi = candidates_dir.join("MachinistLearning.proposals.axi");
    let candidate_text = fs::read_to_string(&candidate_axi).expect("read candidate .axi");
    let parsed = parse_axi_v1(&candidate_text).expect("parse candidate .axi via axi_v1");
    assert!(
        !parsed.instances.is_empty(),
        "expected at least one instance in candidate output"
    );
    let inst = &parsed.instances[0];
    assert!(
        inst.assignments
            .iter()
            .any(|a| a.name == "TacitKnowledge" || a.name == "tacitRule"),
        "expected TacitKnowledge content in candidate output"
    );

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("check")
        .arg("validate")
        .arg(&candidate_axi)
        .status()
        .expect("run axiograph check validate on candidate .axi");
    assert!(
        !status.success(),
        "candidate module should remain non-canonical and fail standalone validation (exit={})",
        status.code().unwrap_or(-1)
    );
}
