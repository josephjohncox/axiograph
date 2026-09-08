use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_ingest_docs::{
    EvidencePointer, ProposalMetaV1, ProposalSourceV1, ProposalV1, ProposalsFileV1,
};
use axiograph_pathdb::certificate::{CertificatePayloadV2, CertificateV2};
use serde::Deserialize;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
struct ExampleCatalogV1 {
    version: u32,
    examples: Vec<ExampleCatalogEntryV1>,
}

#[derive(Debug, Deserialize)]
struct ExampleCatalogEntryV1 {
    id: String,
    path: String,
    surface: String,
    teaching_tier: String,
    #[serde(default)]
    feature_tags: Vec<String>,
    #[serde(default)]
    commands: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SemanticVcsLeanRefSetV1 {
    #[serde(default)]
    refs: Vec<SemanticVcsLeanRefV1>,
}

#[derive(Debug, Deserialize)]
struct SemanticVcsLeanRefV1 {
    kind: String,
    id: String,
}

#[derive(Debug, Deserialize)]
struct SemanticVcsLeanMergeCaseV1 {
    version: String,
    result: SemanticVcsLeanRefSetV1,
    #[serde(default)]
    blockers: Vec<String>,
    #[serde(default)]
    resolver_steps: Vec<SemanticVcsLeanResolverStepV1>,
    #[serde(default)]
    residual_obligations: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct SemanticVcsLeanRebaseCaseV1 {
    version: String,
    #[serde(default)]
    blockers: Vec<String>,
    #[serde(default)]
    transport_items: Vec<SemanticVcsLeanTransportItemV1>,
}

#[derive(Debug, Deserialize)]
struct SemanticVcsLeanResolverStepV1 {
    required: bool,
}

#[derive(Debug, Deserialize)]
struct SemanticVcsLeanTransportItemV1 {
    required: bool,
}

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

fn read_semantic_vcs_case<T: for<'de> Deserialize<'de>>(repo_root: &Path, name: &str) -> T {
    let path = repo_root.join("examples/semantic_merge").join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("read semantic-merge case"))
        .unwrap_or_else(|err| panic!("parse semantic-merge case {}: {err}", path.display()))
}

#[test]
fn example_catalog_paths_exist_and_stay_teaching_oriented() {
    let repo_root = repo_root();
    let catalog_path = repo_root.join("examples/catalog.json");
    let text = fs::read_to_string(&catalog_path).expect("read examples/catalog.json");
    let catalog: ExampleCatalogV1 =
        serde_json::from_str(&text).expect("parse examples/catalog.json");
    assert_eq!(catalog.version, 1);

    let examples = &catalog.examples;
    assert!(
        examples.len() >= 8,
        "expected a pedagogical catalog with multiple routes"
    );

    for example in examples {
        assert!(
            repo_root.join(&example.path).exists(),
            "catalog example `{}` points to missing path `{}`",
            example.id,
            example.path
        );

        assert!(
            !example.feature_tags.is_empty(),
            "catalog example `{}` needs feature tags so agents can route it",
            example.id
        );

        for command in &example.commands {
            assert!(
                !command.contains("export_axi build/"),
                "catalog example `{}` should not foreground debug export scripts",
                example.id
            );
            assert!(
                !command.contains("db pathdb export-axi")
                    && !command.contains("db pathdb import-axi")
                    && !command.contains("PathDBExportV1"),
                "catalog example `{}` should keep storage/debug roundtrips out of teaching commands, got `{command}`",
                example.id
            );
        }
    }
}

#[test]
fn public_competency_question_fixtures_lower_to_typed_queries() {
    let repo_root = repo_root();
    let run_dir = unique_run_dir(&repo_root, "public_competency_question_fixtures");
    let cases = [
        (
            "examples/Family.axi",
            "examples/competency_questions/family_parent.cq",
            "Fam.Parent(child=Carol, parent=?p, ctx=CensusData, time=T2020)",
        ),
        (
            "examples/ontology/OntologyRewrites.axi",
            "examples/competency_questions/bob_parent.cq",
            "OrgFamily.Parent(parent=?p, child=Bob)",
        ),
        (
            "examples/manufacturing/SupplyChainHoTT.axi",
            "examples/competency_questions/supply_chain.cq",
            "SupplyChain.Flow(from=RawMetal_A, to=?to, material=Steel_Billet, qty=?qty, time=?time)",
        ),
    ];

    for (idx, (axi, cq, expected_lowering)) in cases.into_iter().enumerate() {
        let out = run_dir.join("build").join(format!("cq_fixture_{idx}.json"));
        let output = Command::new(axiograph_bin())
            .current_dir(&repo_root)
            .arg("discover")
            .arg("competency-questions")
            .arg(axi)
            .arg("--from-cq")
            .arg(cq)
            .arg("--no-schema")
            .arg("--out")
            .arg(&out)
            .output()
            .expect("run competency question fixture");
        assert!(
            output.status.success(),
            "competency question fixture {cq} should lower successfully\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let report = fs::read_to_string(&out).expect("read competency-question report");
        assert!(
            report.contains(expected_lowering),
            "competency question fixture {cq} should lower against current schema refs"
        );
    }
}

#[test]
fn examples_readme_keeps_storage_debug_roundtrips_out_of_teaching_path() {
    let repo_root = repo_root();
    let readme_path = repo_root.join("examples/README.md");
    let text = fs::read_to_string(&readme_path).expect("read examples/README.md");

    let before_rules = text
        .split("## Greenfield Example Rules")
        .next()
        .expect("examples README should have pre-rules content");
    assert!(
        !before_rules.contains("PathDBExportV1")
            && !before_rules.contains("export-axi")
            && !before_rules.contains("import-axi"),
        "examples README should not foreground storage/debug roundtrips before the greenfield rules"
    );

    let rules = text
        .split("## Greenfield Example Rules")
        .nth(1)
        .expect("examples README should have greenfield rules");
    assert!(
        rules.contains("Storage/debug roundtrips")
            && rules.contains("not public semantic inputs")
            && rules.contains("certificate/query"),
        "examples README must keep storage/debug roundtrips out of the semantic teaching path"
    );
}

#[test]
fn viz_explorer_does_not_offer_unsupported_http_certificate_or_llm_requests() {
    let repo_root = repo_root();
    let text = fs::read_to_string(repo_root.join("frontend/viz/src/tabs/llm.ts"))
        .expect("read viz explorer LLM source");
    assert!(text.contains("UNSUPPORTED") && text.contains("control.disabled = true"));
    for unsupported in [
        "fetch(",
        "query_certificate_policy",
        "body.certify_queries",
        "body.verify_queries",
        "body.require_query_certs",
        "body.require_verified_queries",
    ] {
        assert!(
            !text.contains(unsupported),
            "read-only browser must not issue unsupported `{unsupported}`"
        );
    }
    let query = fs::read_to_string(repo_root.join("frontend/viz/src/tabs/query.ts"))
        .expect("read finite query tab");
    assert!(query.contains("ReadOnlyClient") && query.contains("button.disabled = true"));
    assert!(!query.contains("fetch(") && !query.contains("highlightFromQueryResponse"));
    // Dynamic production-module and genuine HTTP/client checks live in the
    // frontend suite and the explicitly invoked read_only_client_workflow gate.
}

#[test]
fn query_certificate_docs_foreground_policy_not_boolean_aliases() {
    let repo_root = repo_root();
    let docs = [
        repo_root.join("docs/howto/DB_SERVER.md"),
        repo_root.join("docs/reference/QUERY_LANG.md"),
        repo_root.join("docs/howto/CANONICAL_SEMANTIC_SPINE.md"),
        repo_root.join("docs/tutorials/VIZ_EXPLORER.md"),
    ];

    for path in docs {
        let text = fs::read_to_string(&path).expect("read query certificate doc");
        assert!(
            text.contains("certificate_policy") || text.contains("query certificate policy"),
            "{} should document the shared certificate policy surface",
            path.display()
        );
        for removed_literal in [
            "\"certify\": true",
            "\"verify\": true",
            "\"require_query_certs\": true",
            "\"require_verified_queries\": true",
        ] {
            assert!(
                !text.contains(removed_literal),
                "{} should not show query-certificate boolean alias request literal `{removed_literal}`",
                path.display()
            );
        }
    }
}

#[test]
fn semantic_merge_example_keeps_ci_safe_contract() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let theory_output = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("check")
        .arg("theory")
        .arg("examples/semantic_merge/PlantOperationsCore.axi")
        .arg("--closure-tier")
        .arg("finite_fragment")
        .arg("--json")
        .output()
        .expect("run semantic-merge base runtime theory check");
    assert!(
        theory_output.status.success(),
        "semantic-merge base runtime theory check failed (exit={})\nstdout={}\nstderr={}",
        theory_output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&theory_output.stdout),
        String::from_utf8_lossy(&theory_output.stderr)
    );

    let theory: serde_json::Value =
        serde_json::from_slice(&theory_output.stdout).expect("parse runtime theory json");
    assert_eq!(
        theory["version"],
        serde_json::json!("runtime_theory_check_module_report_v1")
    );
    assert!(theory["summary"]["non_claims"]
        .as_array()
        .is_some_and(|claims| claims.iter().any(|claim| {
            claim["code"] == serde_json::json!("closure_engine_not_implemented")
        })));
    assert!(theory["reports"]
        .as_array()
        .expect("runtime theory reports array")
        .iter()
        .all(|report| {
            report["admissibility_scan"]["residual_obligations"]
                .as_array()
                .is_none_or(|residuals| {
                    residuals
                        .iter()
                        .all(|residual| residual != "closure_engine_not_implemented")
                })
                && report["non_claims"].as_array().is_some_and(|claims| {
                    claims.iter().any(|claim| {
                        claim["code"] == serde_json::json!("closure_engine_not_implemented")
                    })
                })
        }));
    assert_eq!(theory["summary"]["blocking_errors"], serde_json::json!(0));
    assert!(
        theory["summary"]["scope"]["fragments"]
            .as_array()
            .expect("runtime-theory scope fragments array")
            .iter()
            .any(|tier| tier == "finite_fragment"),
        "semantic-merge base should keep the finite runtime-theory check contract"
    );
    assert!(
        theory["reports"]
            .as_array()
            .expect("runtime theory reports array")
            .iter()
            .any(|report| report["schema_id"] == serde_json::json!("PlantOps")),
        "semantic-merge base should report the PlantOps theory surface"
    );

    let catalog_path = repo_root.join("examples/catalog.json");
    let catalog: ExampleCatalogV1 = serde_json::from_str(
        &fs::read_to_string(&catalog_path).expect("read examples/catalog.json"),
    )
    .expect("parse examples/catalog.json");
    let semantic_merge = catalog
        .examples
        .iter()
        .find(|example| example.id == "semantic-merge-plant-operations")
        .expect("semantic-merge example catalog entry");
    assert_eq!(semantic_merge.path, "examples/semantic_merge");
    assert_eq!(semantic_merge.surface, "semantic_vcs_flow");
    assert_eq!(semantic_merge.teaching_tier, "teaching");
    for tag in ["semantic-vcs", "merge", "rebase", "runtime-theory"] {
        assert!(
            semantic_merge.feature_tags.iter().any(|value| value == tag),
            "semantic-merge catalog entry should keep `{tag}` feature tag"
        );
    }
    assert!(
        semantic_merge
            .commands
            .iter()
            .any(|command| command == "make verify-lean-semantic-vcs"),
        "semantic-merge catalog entry should expose the focused Rust+Lean gate"
    );
    assert!(
        semantic_merge.commands.iter().any(|command| command
            .contains("check theory examples/semantic_merge/PlantOperationsCore.axi")),
        "semantic-merge catalog entry should expose the lightweight runtime-theory check"
    );

    assert!(
        !repo_root
            .join("examples/semantic_merge/run_merge_flow.sh")
            .exists(),
        "removed filesystem semantic-VCS runner must not return"
    );

    let clean_merge: SemanticVcsLeanMergeCaseV1 =
        read_semantic_vcs_case(&repo_root, "plant_clean_merge_lean.json");
    assert_eq!(clean_merge.version, "semantic_vcs_lean_merge_plan_v1");
    assert!(clean_merge.blockers.is_empty());
    assert!(clean_merge.resolver_steps.is_empty());
    assert!(clean_merge.residual_obligations.is_empty());
    assert!(
        !clean_merge.result.refs.is_empty(),
        "clean merge case should cite result refs"
    );
    assert!(
        clean_merge
            .result
            .refs
            .iter()
            .any(|reference| reference.kind == "relation_object"
                && reference.id == "PlantProcurement.ReleaseDocumentForLot"),
        "clean merge case should cite typed result refs"
    );

    let clean_rebase: SemanticVcsLeanRebaseCaseV1 =
        read_semantic_vcs_case(&repo_root, "plant_clean_rebase_lean.json");
    assert_eq!(clean_rebase.version, "semantic_vcs_lean_rebase_plan_v1");
    assert!(clean_rebase.blockers.is_empty());
    assert!(
        clean_rebase
            .transport_items
            .iter()
            .any(|item| item.required),
        "clean rebase case should keep a required transport item"
    );

    let conflict_merge: SemanticVcsLeanMergeCaseV1 =
        read_semantic_vcs_case(&repo_root, "plant_conflict_merge_lean.json");
    assert_eq!(conflict_merge.version, "semantic_vcs_lean_merge_plan_v1");
    assert!(
        !conflict_merge.blockers.is_empty(),
        "conflict case should remain a negative merge case"
    );
    assert!(
        conflict_merge
            .resolver_steps
            .iter()
            .any(|step| step.required),
        "conflict case should require a typed resolver step"
    );
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
fn behavior_case_example_runs() {
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
        .expect("run behavior-case example");

    assert!(
        status.success(),
        "behavior-case example failed (exit={})",
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
        .expect("run software-authoring behavior-case example");

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
        .arg("--term")
        .arg("shipment eligibility")
        .arg("--relation")
        .arg("OrderEligibleForShipment")
        .arg("--cq-name")
        .arg("accepted_order_is_shipment_eligible")
        .arg("--surface-hint")
        .arg("shipping")
        .arg("--max-matches")
        .arg("8")
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
        serde_json::json!("overlay_software_coverage_report_v1")
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

    let behavior_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("behavior_case_report.json"))
            .expect("read scripted behavior report"),
    )
    .expect("parse scripted behavior report");
    assert_eq!(
        behavior_json["runtime_theory_check"]["version"],
        serde_json::json!("runtime_theory_check_summary_v1")
    );
    assert!(behavior_json["runtime_theory_check"]["scope"]["fragments"]
        .as_array()
        .is_some_and(|fragments| fragments
            .iter()
            .any(|fragment| fragment == "finite_fragment")));

    let coverage_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("software_coverage.json"))
            .expect("read scripted software coverage"),
    )
    .expect("parse scripted software coverage");
    assert_eq!(
        coverage_json["version"],
        serde_json::json!("overlay_software_coverage_report_v1")
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
            "authoring_workspace_report.json",
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
fn software_authoring_cli_exposes_unified_workspace_and_editor_contracts() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "software_authoring_cli_contracts");
    let codegen_plan = run_dir.join("build/codegen_plan.json");
    let tool_specs = run_dir.join("build/tool_specs.json");
    let lsp = run_dir.join("build/lsp_capabilities.json");
    let integration_manifest = run_dir.join("build/integration_manifest.json");
    let authoring_report = run_dir.join("build/authoring_workspace_report.json");

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

    let workspace_status = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("authoring")
        .arg("workspace")
        .arg("--workspace")
        .arg(".")
        .arg("--request")
        .arg("examples/software_authoring/authoring_workspace_request.json")
        .arg("--out")
        .arg(&authoring_report)
        .status()
        .expect("run unified authoring workspace");
    assert!(workspace_status.success(), "authoring workspace failed");

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
        .arg("--workspace")
        .arg(".")
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

    let authoring_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&authoring_report).expect("read authoring workspace report"),
    )
    .expect("parse authoring workspace report");
    assert_eq!(
        authoring_json["version"],
        serde_json::json!("authoring_workspace_report_v1")
    );
    assert!(authoring_json["query_explanation"].is_object());
    assert_eq!(
        authoring_json["competency_questions"]["promotion_gate"],
        serde_json::json!("passed")
    );

    let lsp_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&lsp).expect("read lsp capabilities"))
            .expect("parse lsp capabilities");
    assert_eq!(
        lsp_json["version"],
        serde_json::json!("authoring_workspace_capabilities_v1")
    );

    let manifest_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&integration_manifest).expect("read integration manifest"),
    )
    .expect("parse integration manifest");
    assert_eq!(
        manifest_json["version"],
        serde_json::json!("authoring_workspace_integration_manifest_v1")
    );
    assert_eq!(
        manifest_json["lsp"]["args"],
        serde_json::json!(["authoring", "lsp", "--workspace", "."])
    );
    assert_eq!(
        manifest_json["mcp"]["args"],
        serde_json::json!(["authoring", "mcp", "--workspace", "."])
    );
    assert_eq!(
        manifest_json["request_contract"],
        serde_json::json!("authoring_workspace_request_v1")
    );
    assert_eq!(
        lsp_json["mcp"]["tool"],
        serde_json::json!("axiograph_authoring_workspace")
    );
}

#[test]
fn embedded_tooling_behavior_case_fields_fail_clearly() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "embedded_tooling_behavior_case_schema");
    let embedded_tooling_request = run_dir.join("embedded_tooling_behavior_case.json");
    fs::write(
        &embedded_tooling_request,
        r#"{
  "behavior_case": {
    "case_id": "software_authoring.embedded_tooling",
    "title": "Embedded tooling field",
    "context": {
      "context_id": "bounded-context:embedded-tooling",
      "label": "Embedded Tooling"
    }
  }
}
"#,
    )
    .expect("write embedded tooling request");

    let output = Command::new(&bin)
        .current_dir(&repo_root)
        .arg("discover")
        .arg("behavior-case")
        .arg("examples/software_authoring/OrderFulfillmentDomain.axi")
        .arg("--request")
        .arg(&embedded_tooling_request)
        .arg("--overlay")
        .arg("examples/software_authoring/order_fulfillment_tooling_overlay.json")
        .output()
        .expect("run embedded tooling behavior-case");
    assert!(
        !output.status.success(),
        "embedded tooling behavior-case fields should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown field `context`") || stderr.contains("context"),
        "expected embedded tooling schema error mentioning context, got: {stderr}"
    );
}

#[test]
fn software_authoring_runtime_theory_check_reports_admissibility_trace() {
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
        stdout.contains("admissibility trace:"),
        "expected human summary to expose admissibility trace, got: {stdout}"
    );
    assert!(
        stdout.contains("anchor: revision_digest=axi:revision:v2:sha256:"),
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
        stdout.contains("saturation=not_run"),
        "expected human summary to deny synthetic saturation, got: {stdout}"
    );
    assert!(
        stdout.contains("closure requires a separate replayable finite certificate"),
        "expected next action to preserve the non-closure boundary, got: {stdout}"
    );
}

#[test]
fn typecheck_cert_regression() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "typecheck_cert");
    let cert_path = run_dir.join("build/typecheck_cert.json");
    let repeated_cert_path = run_dir.join("build/typecheck_cert_repeated.json");

    let input = repo_root.join("examples/Family.axi");
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
    let repeated_status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("typecheck")
        .arg(&input)
        .arg("--out")
        .arg(&repeated_cert_path)
        .status()
        .expect("repeat axiograph cert typecheck");
    assert!(repeated_status.success(), "repeated typecheck-cert failed");

    let cert_bytes = fs::read(&cert_path).expect("read typecheck cert bytes");
    let repeated_bytes = fs::read(&repeated_cert_path).expect("read repeated typecheck cert bytes");
    assert_eq!(
        cert_bytes, repeated_bytes,
        "typecheck certificate bytes drifted"
    );
    let cert_text = String::from_utf8(cert_bytes).expect("typecheck certificate must be UTF-8");
    let cert: CertificateV2 = serde_json::from_str(&cert_text).expect("parse typecheck cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor
            .revision_digest_v2
            .as_str()
            .starts_with("axi:revision:v2:sha256:"),
        "unexpected digest format: {}",
        anchor.revision_digest_v2
    );

    match cert.payload {
        CertificatePayloadV2::AxiWellTypedV1 { proof } => {
            assert_eq!(proof.module_name, "Family");
            assert!(proof.schema_count >= 1);
        }
        other => panic!("expected axi_well_typed_v1 certificate, got {other:?}"),
    }
}

#[test]
fn constraints_cert_regression() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "constraints_cert");
    let cert_path = run_dir.join("build/constraints_cert.json");
    let repeated_cert_path = run_dir.join("build/constraints_cert_repeated.json");

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
    let repeated_status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("cert")
        .arg("constraints")
        .arg(&input)
        .arg("--out")
        .arg(&repeated_cert_path)
        .status()
        .expect("repeat axiograph cert constraints");
    assert!(
        repeated_status.success(),
        "repeated constraints-cert failed"
    );

    let cert_bytes = fs::read(&cert_path).expect("read constraints cert bytes");
    let repeated_bytes =
        fs::read(&repeated_cert_path).expect("read repeated constraints cert bytes");
    assert_eq!(
        cert_bytes, repeated_bytes,
        "constraints certificate bytes drifted"
    );
    let cert_text = String::from_utf8(cert_bytes).expect("constraints certificate must be UTF-8");
    let cert: CertificateV2 =
        serde_json::from_str(&cert_text).expect("parse constraints cert json");

    assert_eq!(cert.version, 2);
    let anchor = cert.anchor.expect("expected anchor");
    assert!(
        anchor
            .revision_digest_v2
            .as_str()
            .starts_with("axi:revision:v2:sha256:"),
        "unexpected digest format: {}",
        anchor.revision_digest_v2
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
fn analyze_network_and_quality_regression() {
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

    let mut quality_command = Command::new(&bin);
    quality_command.current_dir(&run_dir);
    #[cfg(feature = "profiling")]
    quality_command.arg("--cpu-profile").arg("off");
    let status = quality_command
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
fn removed_accepted_plane_cli_fails_closed_without_writing_state() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "removed_accepted_plane_cli");
    let obsolete_dir = run_dir.join("build/accepted_plane");
    let output = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("db")
        .arg("accept")
        .arg("promote")
        .arg(repo_root.join("examples/ontology/OntologyRewrites.axi"))
        .arg("--dir")
        .arg(&obsolete_dir)
        .output()
        .expect("run removed accepted-plane command");

    assert!(
        !output.status.success(),
        "removed command unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unrecognized subcommand 'accept'"),
        "unexpected removed-command error: {stderr}"
    );
    assert!(
        !obsolete_dir.exists(),
        "removed command must not create legacy accepted-plane state"
    );
}

#[test]
fn repl_scripts_canonical_regression() {
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
        let mut removed_debug_exports: Vec<PathBuf> = fs::read_dir(&build_dir)
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
        removed_debug_exports.sort();

        assert!(
            removed_debug_exports.is_empty(),
            "REPL script `{}` should not emit debug snapshot teaching exports: {:?}",
            script.display(),
            removed_debug_exports
        );
    }
}

#[test]
fn repl_rejects_removed_export_axi_command() {
    let repo_root = repo_root();
    let bin = axiograph_bin();
    let run_dir = unique_run_dir(&repo_root, "repl_rejects_removed_export_axi");

    let output = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("repl")
        .arg("--quiet")
        .arg("--cmd")
        .arg("export_axi build/removed_export_v1.axi")
        .output()
        .expect("run axiograph repl removed export_axi");

    assert!(
        !output.status.success(),
        "removed REPL export_axi command should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown command `export_axi`"),
        "expected removed export_axi guidance, got: {stderr}"
    );
}

#[test]
fn discover_augment_proposals_regression() {
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
            locator: "discover_augment_proposals_regression".to_string(),
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
fn discover_draft_module_regression() {
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
            locator: "discover_draft_module_regression".to_string(),
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
fn discover_transport_preview_regression() {
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
fn doc_to_proposals_to_candidate_axi_regression() {
    let repo_root = repo_root();
    let bin = axiograph_bin();

    let run_dir = unique_run_dir(&repo_root, "doc_to_candidates");
    let build_dir = run_dir.join("build");

    let input = repo_root.join("examples/ingest_sources/machining_conversation.txt");
    let proposals_path = build_dir.join("proposals.json");
    let chunks_path = build_dir.join("chunks.json");
    let facts_path = build_dir.join("facts.json");

    let status = Command::new(&bin)
        .current_dir(&run_dir)
        .arg("ingest")
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
        .expect("run axiograph ingest doc");
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
