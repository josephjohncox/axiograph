use std::fs;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use axiograph_kernel::ObjectBlobIdV2;
use axiograph_store::*;
use tempfile::tempdir;

fn axiograph_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_axiograph")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/axiograph")
        })
}

fn descriptor() -> RepositoryDescriptor {
    RepositoryDescriptor::new("cli-materialization-tests", "fixed-test-genesis").unwrap()
}

fn exact_module(label: &str) -> Vec<u8> {
    format!("module {label}\n").into_bytes()
}

fn supporting_objects(label: &str) -> Vec<ImmutableBlob> {
    [
        (ImmutableObjectKind::KernelIr, "kernel"),
        (ImmutableObjectKind::CanonicalFactLog, "facts"),
        (ImmutableObjectKind::ValidationReport, "validation"),
        (ImmutableObjectKind::CompetencyQuestionReport, "competency"),
        (ImmutableObjectKind::TheoryReport, "theory"),
        (ImmutableObjectKind::VerificationReceipt, "checker"),
    ]
    .into_iter()
    .map(|(kind, suffix)| {
        ImmutableBlob::new(kind, format!("{label}:{suffix}").into_bytes()).unwrap()
    })
    .collect()
}

fn object_digest(objects: &[ImmutableBlob], kind: ImmutableObjectKind) -> ObjectBlobIdV2 {
    objects
        .iter()
        .find(|object| object.kind == kind)
        .unwrap()
        .digest
        .clone()
}

fn materialization_spec(label: &str) -> AxpdBuildSpec {
    let repository = descriptor().repository_id().unwrap();
    let module = AcceptedModule::from_bytes(&repository, label, &exact_module(label)).unwrap();
    let tree = AcceptedTree::new(repository.clone(), vec![module.clone()]).unwrap();
    let snapshot = AcceptedSnapshot::new(repository.clone(), tree.tree_id.clone(), vec![]).unwrap();
    let support = supporting_objects(label);
    AxpdBuildSpec {
        anchors: AxpdAnchors {
            repository_id: repository,
            accepted_snapshot_id: snapshot.snapshot_id,
            accepted_tree_id: tree.tree_id,
            ordered_module_closure: vec![module.revision_digest],
            kernel_ir_digest: object_digest(&support, ImmutableObjectKind::KernelIr),
            canonical_fact_log_digest: object_digest(
                &support,
                ImmutableObjectKind::CanonicalFactLog,
            ),
        },
        image: AxpdLogicalImage::default(),
        configuration: AxpdConfiguration::default(),
        materializer_version: AXPD_MATERIALIZER_VERSION.to_string(),
    }
}

fn init_store(path: &std::path::Path, label: &str) -> AxiStore {
    let store = AxiStore::init(path, &descriptor()).unwrap();
    let spec = materialization_spec(label);
    let repository = descriptor().repository_id().unwrap();
    let publication = ModulePublication::new(&repository, label, exact_module(label)).unwrap();
    let tree = AcceptedTree::new(repository.clone(), vec![publication.module.clone()]).unwrap();
    let snapshot = AcceptedSnapshot::new(repository.clone(), tree.tree_id.clone(), vec![]).unwrap();
    let objects = supporting_objects(label);
    let validation = object_digest(&objects, ImmutableObjectKind::ValidationReport);
    let competency = object_digest(&objects, ImmutableObjectKind::CompetencyQuestionReport);
    let theory = object_digest(&objects, ImmutableObjectKind::TheoryReport);
    let checker = object_digest(&objects, ImmutableObjectKind::VerificationReceipt);
    let manifest = AcceptedBuildManifest {
        format: BUILD_MANIFEST_FORMAT.to_string(),
        version: FORMAT_VERSION,
        repository_id: repository.clone(),
        accepted_tree_id: tree.tree_id.clone(),
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        ordered_module_closure: vec![publication.module.revision_digest.clone()],
        compiler_version: "canonical-cli-test".to_string(),
        ir_version: "kernel-ir-v2-cli-test".to_string(),
        kernel_ir_digest: object_digest(&objects, ImmutableObjectKind::KernelIr),
        canonical_fact_log_digest: object_digest(&objects, ImmutableObjectKind::CanonicalFactLog),
        validation_report_digest: validation.clone(),
        competency_question_report_digest: competency.clone(),
        runtime_theory_report_digest: theory.clone(),
        trusted_checker_receipt_digest: checker.clone(),
        non_claims: required_non_claims(),
    };
    assert_eq!(tree.tree_id, spec.anchors.accepted_tree_id);
    assert_eq!(snapshot.snapshot_id, spec.anchors.accepted_snapshot_id);
    let commit = SemCommitV2::new(
        repository,
        CommitKind::Normal,
        vec![],
        tree.tree_id.clone(),
        snapshot.snapshot_id.clone(),
        manifest.digest().unwrap(),
        None,
        "operator@example.test",
        1_700_000_000,
        Some(format!("accept {label}")),
        "promote",
        "protected-main",
        CommitProvenance {
            source: "db-server-e2e".to_string(),
            command: None,
            origin_trust: OriginTrust::Native,
        },
        SemanticDelta {
            changes: vec![SemanticChange {
                operation: ReindexOperation::Add,
                sources: vec![],
                targets: vec![SemanticId::Revision(
                    publication.module.revision_digest.clone(),
                )],
            }],
        },
        vec![
            PromotionGate {
                kind: GateKind::CanonicalValidation,
                decision: GateDecision::Passed,
                report_digest: validation,
            },
            PromotionGate {
                kind: GateKind::CompetencyQuestions,
                decision: GateDecision::Passed,
                report_digest: competency,
            },
            PromotionGate {
                kind: GateKind::RuntimeTheory,
                decision: GateDecision::Passed,
                report_digest: theory,
            },
            PromotionGate {
                kind: GateKind::Trust,
                decision: GateDecision::Passed,
                report_digest: checker,
            },
        ],
        vec![CommitAttachment {
            kind: ImmutableObjectKind::KernelIr,
            digest: manifest.kernel_ir_digest.clone(),
        }],
        vec![LifecycleEvent {
            artifact: SemanticId::Revision(publication.module.revision_digest.clone()),
            from: Some(LifecycleStage::Reviewed),
            to: LifecycleStage::Accepted,
            reason: "reviewed promotion".to_string(),
        }],
    )
    .unwrap();
    store
        .promote(
            0,
            &PromotionPlan {
                modules: vec![publication],
                objects,
                tree,
                snapshot,
                manifest,
                reconciliation: None,
                commit,
                ref_updates: vec![],
            },
        )
        .unwrap();
    store
}

fn wait_for_ready(path: &std::path::Path, child: &mut Child) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if path.exists() {
            return serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("server exited before readiness: {status}");
        }
        assert!(Instant::now() < deadline, "server readiness timed out");
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn bare_axpd_flag_is_not_a_cli_surface() {
    let output = Command::new(axiograph_bin())
        .args(["db", "serve", "--axpd", "bare.axpd"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument '--axpd'"), "{stderr}");
}

#[test]
fn cli_publishes_and_inspects_only_for_an_accepted_axi_store_manifest() {
    let store = tempdir().unwrap();
    init_store(store.path(), "cli-publish");
    let spec_path = store.path().join("build-spec.json");
    fs::write(
        &spec_path,
        serde_json::to_vec(&materialization_spec("cli-publish")).unwrap(),
    )
    .unwrap();

    let published = Command::new(axiograph_bin())
        .args([
            "db",
            "materialize",
            "--dir",
            store.path().to_str().unwrap(),
            "--spec",
            spec_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stderr)
    );
    let receipt: axiograph_store::AxpdReceipt = serde_json::from_slice(&published.stdout).unwrap();

    let inspected = Command::new(axiograph_bin())
        .args([
            "db",
            "materialization-show",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
        ])
        .output()
        .unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspected_receipt: axiograph_store::AxpdReceipt =
        serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(inspected_receipt, receipt);
}

#[test]
fn invalid_or_tampered_materialization_rejects_before_listener_publication() {
    let store = tempdir().unwrap();
    let ready = store.path().join("ready.json");
    let invalid = Command::new(axiograph_bin())
        .args([
            "db",
            "serve",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            "not-a-materialization-id",
            "--listen",
            "127.0.0.1:0",
            "--ready-file",
            ready.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    assert!(!ready.exists());

    let axi_store = init_store(store.path(), "tampered-server");
    let receipt = axi_store
        .publish_axpd(
            materialization_spec("tampered-server"),
            &AxpdLimits::default(),
        )
        .unwrap();
    let receipt_path = axi_store.axpd_receipt_path(&receipt.materialization_id);
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    json["logical_digest"] = serde_json::Value::String(
        ObjectBlobIdV2::from_canonical_fields(&[b"tampered"]).to_string(),
    );
    fs::write(&receipt_path, serde_json::to_vec(&json).unwrap()).unwrap();

    let tampered = Command::new(axiograph_bin())
        .args([
            "db",
            "serve",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
            "--listen",
            "127.0.0.1:0",
            "--ready-file",
            ready.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!tampered.status.success());
    assert!(!ready.exists());
}

#[test]
fn server_publishes_only_after_authenticated_open() {
    let store = tempdir().unwrap();
    let axi_store = init_store(store.path(), "server-open");
    let receipt = axi_store
        .publish_axpd(materialization_spec("server-open"), &AxpdLimits::default())
        .unwrap();
    let ready = store.path().join("ready.json");
    let mut child = Command::new(axiograph_bin())
        .args([
            "db",
            "serve",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
            "--listen",
            "127.0.0.1:0",
            "--ready-file",
            ready.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let payload = wait_for_ready(&ready, &mut child);
    assert_eq!(payload["format"], "axiograph_db_server_ready_v2");
    assert_eq!(
        payload["materialization_id"],
        receipt.materialization_id.to_string()
    );
    let listen = payload["listen"].as_str().unwrap();
    let health = reqwest::blocking::get(format!("http://{listen}/healthz")).unwrap();
    assert!(health.status().is_success());
    let status: serde_json::Value = reqwest::blocking::get(format!("http://{listen}/status"))
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(status["format"], "axiograph_authenticated_pathdb_status_v2");
    assert_eq!(
        status["receipt"]["materialization_id"],
        receipt.materialization_id.to_string()
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn mcp_requires_and_opens_an_exact_materialization_id() {
    let store = tempdir().unwrap();
    let axi_store = init_store(store.path(), "mcp-open");
    let receipt = axi_store
        .publish_axpd(materialization_spec("mcp-open"), &AxpdLimits::default())
        .unwrap();

    let invalid = Command::new(axiograph_bin())
        .args([
            "mcp",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            "invalid",
        ])
        .output()
        .unwrap();
    assert!(!invalid.status.success());

    let mut child = Command::new(axiograph_bin())
        .args([
            "mcp",
            "--dir",
            store.path().to_str().unwrap(),
            "--materialization",
            receipt.materialization_id.as_str(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    thread::sleep(Duration::from_millis(250));
    assert!(child.try_wait().unwrap().is_none());
    drop(stdin);
    child.kill().unwrap();
    child.wait().unwrap();
}
