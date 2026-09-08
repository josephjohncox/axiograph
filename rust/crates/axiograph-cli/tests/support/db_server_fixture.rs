//! Shared test/example fixtures, not a public runtime API. Synthetic logical
//! rows and inert support blobs are published through production AxiStore APIs;
//! the resulting image receipts are not Lean checker receipts.
use axiograph_kernel::{InstanceIdV2, ObjectBlobIdV2, SchemaIdV2, SemanticKeyV2};
use axiograph_store::*;
use std::fs;
use std::process::Child;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) fn axiograph_bin() -> std::path::PathBuf {
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
pub(crate) fn materialization_spec(label: &str) -> AxpdBuildSpec {
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
pub(crate) fn init_store(path: &std::path::Path, label: &str) -> AxiStore {
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
pub(crate) fn finite_client_spec(label: &str, oversized_ui: bool) -> AxpdBuildSpec {
    let mut spec = materialization_spec(label);
    let module =
        AcceptedModule::from_bytes(&spec.anchors.repository_id, label, &exact_module(label))
            .unwrap();
    let schema = SchemaIdV2::derive(
        &module.revision_digest,
        &SemanticKeyV2::derive(&module.module_id, "schema", "Fixture"),
    );
    let instance = InstanceIdV2::derive(
        &module.revision_digest,
        &schema,
        &SemanticKeyV2::derive(&module.module_id, "instance", "Fixture"),
    );
    for name in ["Alice", "Bob"] {
        spec.image.entities.push(EntityRow {
            entity_key: ObjectBlobIdV2::from_canonical_fields(&[b"http-fixture", name.as_bytes()]),
            instance_id: instance.clone(),
            object_ref_json: r#"{"kind":"object_type","id":"fixture"}"#.to_string(),
            value: if oversized_ui {
                "x".repeat(8193)
            } else {
                name.to_string()
            },
        });
    }
    spec
}
pub(crate) fn wait_for_ready(path: &std::path::Path, child: &mut Child) -> serde_json::Value {
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
pub(crate) struct ServerChild(pub(crate) Child);
impl Drop for ServerChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
