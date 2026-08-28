use axiograph_kernel::{
    FactIdV2, FactRoleValueV2, InstanceIdV2, MaterializationIdV2, ObjectBlobIdV2, RelationIdV2,
    RepositoryIdV2, RevisionDigestV2, RoleIdV2, SchemaIdV2, SemanticKeyV2, SnapshotIdV2, TreeIdV2,
};
use axiograph_store::*;
use proptest::prelude::*;
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

fn test_descriptor() -> RepositoryDescriptor {
    RepositoryDescriptor::new("materialization-tests", "fixed-test-genesis").unwrap()
}

fn exact_module(seed: &str) -> Vec<u8> {
    format!("module Fixture // {seed}\n").into_bytes()
}

fn supporting_objects(seed: &str) -> Vec<ImmutableBlob> {
    [
        (ImmutableObjectKind::KernelIr, "kernel_ir"),
        (ImmutableObjectKind::CanonicalFactLog, "fact_log"),
        (ImmutableObjectKind::ValidationReport, "validation"),
        (
            ImmutableObjectKind::CompetencyQuestionReport,
            "competency_questions",
        ),
        (ImmutableObjectKind::TheoryReport, "runtime_theory"),
        (ImmutableObjectKind::VerificationReceipt, "lean_receipt"),
    ]
    .into_iter()
    .map(|(kind, label)| ImmutableBlob::new(kind, format!("{label}:{seed}").into_bytes()).unwrap())
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

fn store_at(root: &std::path::Path) -> AxiStore {
    let catalog = root.join("catalog.sqlite");
    if catalog.exists() {
        AxiStore::open(root).unwrap()
    } else {
        AxiStore::init(root, &test_descriptor()).unwrap()
    }
}

fn accept_materialization_anchors(store: &AxiStore, spec: &AxpdBuildSpec) {
    if store.status().unwrap().state.accepted_commit_id.is_some() {
        return;
    }
    let seed = spec
        .image
        .kernel_refs
        .iter()
        .find_map(|row| {
            serde_json::from_str::<serde_json::Value>(&row.wire_json)
                .ok()?
                .get("seed")?
                .as_str()
                .map(str::to_string)
        })
        .expect("fixture kernel ref seed");
    let repository_id = test_descriptor().repository_id().unwrap();
    let publication =
        ModulePublication::new(&repository_id, "Fixture", exact_module(&seed)).unwrap();
    let tree = AcceptedTree::new(repository_id.clone(), vec![publication.module.clone()]).unwrap();
    let snapshot =
        AcceptedSnapshot::new(repository_id.clone(), tree.tree_id.clone(), vec![]).unwrap();
    assert_eq!(tree.tree_id, spec.anchors.accepted_tree_id);
    assert_eq!(snapshot.snapshot_id, spec.anchors.accepted_snapshot_id);

    let objects = supporting_objects(&seed);
    let validation = object_digest(&objects, ImmutableObjectKind::ValidationReport);
    let competency = object_digest(&objects, ImmutableObjectKind::CompetencyQuestionReport);
    let theory = object_digest(&objects, ImmutableObjectKind::TheoryReport);
    let checker = object_digest(&objects, ImmutableObjectKind::VerificationReceipt);
    let manifest = AcceptedBuildManifest {
        format: BUILD_MANIFEST_FORMAT.to_string(),
        version: FORMAT_VERSION,
        repository_id: repository_id.clone(),
        accepted_tree_id: tree.tree_id.clone(),
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        ordered_module_closure: vec![publication.module.revision_digest.clone()],
        compiler_version: "canonical-compiler-test".to_string(),
        ir_version: "kernel-ir-v2-test".to_string(),
        kernel_ir_digest: object_digest(&objects, ImmutableObjectKind::KernelIr),
        canonical_fact_log_digest: object_digest(&objects, ImmutableObjectKind::CanonicalFactLog),
        validation_report_digest: validation.clone(),
        competency_question_report_digest: competency.clone(),
        runtime_theory_report_digest: theory.clone(),
        trusted_checker_receipt_digest: checker.clone(),
        non_claims: required_non_claims(),
    };
    assert_eq!(manifest.kernel_ir_digest, spec.anchors.kernel_ir_digest);
    assert_eq!(
        manifest.canonical_fact_log_digest,
        spec.anchors.canonical_fact_log_digest
    );
    let commit = SemCommitV2::new(
        repository_id,
        CommitKind::Normal,
        vec![],
        tree.tree_id.clone(),
        snapshot.snapshot_id.clone(),
        manifest.digest().unwrap(),
        None,
        "operator@example.test",
        1_700_000_000,
        Some(format!("accept {seed}")),
        "promote",
        "protected-main",
        CommitProvenance {
            source: "materialization-tests".to_string(),
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
    let plan = PromotionPlan {
        modules: vec![publication],
        objects,
        tree,
        snapshot,
        manifest,
        reconciliation: None,
        commit,
        ref_updates: vec![],
    };
    store.promote(0, &plan).unwrap();
}

fn publish_in_store(
    root: &std::path::Path,
    spec: AxpdBuildSpec,
    limits: &AxpdLimits,
) -> AxpdResult<AxpdReceipt> {
    let store = store_at(root);
    accept_materialization_anchors(&store, &spec);
    store.publish_axpd(spec, limits)
}

fn publish_in_store_with_injector(
    root: &std::path::Path,
    spec: AxpdBuildSpec,
    limits: &AxpdLimits,
    injector: &dyn AxpdFailureInjector,
) -> AxpdResult<AxpdReceipt> {
    let store = store_at(root);
    accept_materialization_anchors(&store, &spec);
    store.publish_axpd_with_injector(spec, limits, injector)
}

fn image_path_in_store(
    root: &std::path::Path,
    materialization_id: &MaterializationIdV2,
) -> std::path::PathBuf {
    store_at(root).axpd_image_path(materialization_id)
}

fn receipt_path_in_store(
    root: &std::path::Path,
    materialization_id: &MaterializationIdV2,
) -> std::path::PathBuf {
    store_at(root).axpd_receipt_path(materialization_id)
}

fn recover_in_store(
    root: &std::path::Path,
    spec: AxpdBuildSpec,
    limits: &AxpdLimits,
) -> AxpdResult<AxpdRecoveryReport> {
    let store = store_at(root);
    accept_materialization_anchors(&store, &spec);
    store.recover_axpd(spec, limits)
}

fn open_from_store(
    root: &std::path::Path,
    materialization_id: &MaterializationIdV2,
    limits: &AxpdLimits,
) -> AxpdResult<VerifiedAxpd> {
    store_at(root).open_axpd(materialization_id, limits)
}

#[derive(Clone)]
struct FixtureIds {
    repository: RepositoryIdV2,
    revision: RevisionDigestV2,
    snapshot: SnapshotIdV2,
    tree: TreeIdV2,
    instance: InstanceIdV2,
    relation: RelationIdV2,
    role: RoleIdV2,
    fact: FactIdV2,
    entity: ObjectBlobIdV2,
}

fn fixture_ids(seed: &str) -> FixtureIds {
    let repository = test_descriptor().repository_id().unwrap();
    let accepted_module = AcceptedModule::from_bytes(&repository, "Fixture", &exact_module(seed))
        .expect("fixture module");
    let module = accepted_module.module_id.clone();
    let revision = accepted_module.revision_digest.clone();
    let schema_key = SemanticKeyV2::derive(&module, "schema", "FixtureSchema");
    let schema = SchemaIdV2::derive(&revision, &schema_key);
    let relation_key = SemanticKeyV2::derive(&module, "relation", "ObservedAt");
    let relation = RelationIdV2::derive(&revision, &schema, &relation_key);
    let role_key = SemanticKeyV2::derive(&module, "role", "subject");
    let role = RoleIdV2::derive(&revision, &schema, &relation, &role_key, 0);
    let instance_key = SemanticKeyV2::derive(&module, "instance", "FixtureData");
    let instance = InstanceIdV2::derive(&revision, &schema, &instance_key);
    let fact = FactIdV2::derive(
        &revision,
        &schema,
        &instance,
        &relation,
        &[FactRoleValueV2 {
            role_id: &role,
            value: b"entity-1",
        }],
    );
    let entity = ObjectBlobIdV2::from_canonical_fields(&[
        b"fixture_entity",
        instance.as_str().as_bytes(),
        seed.as_bytes(),
    ]);
    let tree = AcceptedTree::new(repository.clone(), vec![accepted_module])
        .unwrap()
        .tree_id;
    let snapshot = AcceptedSnapshot::new(repository.clone(), tree.clone(), vec![])
        .unwrap()
        .snapshot_id;
    FixtureIds {
        repository,
        revision,
        snapshot,
        tree,
        instance,
        relation,
        role,
        fact,
        entity,
    }
}

fn fixture_spec(seed: &str) -> AxpdBuildSpec {
    let ids = fixture_ids(seed);
    let support = supporting_objects(seed);
    let overlay = ObjectBlobIdV2::from_canonical_fields(&[b"overlay", seed.as_bytes()]);
    AxpdBuildSpec {
        anchors: AxpdAnchors {
            repository_id: ids.repository,
            accepted_snapshot_id: ids.snapshot,
            accepted_tree_id: ids.tree,
            ordered_module_closure: vec![ids.revision],
            kernel_ir_digest: object_digest(&support, ImmutableObjectKind::KernelIr),
            canonical_fact_log_digest: object_digest(
                &support,
                ImmutableObjectKind::CanonicalFactLog,
            ),
        },
        image: AxpdLogicalImage {
            kernel_refs: vec![KernelRefRow {
                wire_json: format!(r#"{{"kind":"module","seed":"{seed}"}}"#),
            }],
            entities: vec![EntityRow {
                entity_key: ids.entity.clone(),
                instance_id: ids.instance.clone(),
                object_ref_json: r#"{"kind":"object_type","id":"fixture"}"#.to_string(),
                value: "entity-1".to_string(),
            }],
            relation_facts: vec![RelationFactRow {
                fact_id: ids.fact.clone(),
                instance_id: ids.instance.clone(),
                relation_id: ids.relation,
                local_label: Some("fact-1".to_string()),
            }],
            projections: vec![ProjectionRow {
                fact_id: ids.fact.clone(),
                ordinal: 0,
                role_id: ids.role.clone(),
                value_kind: "object_element".to_string(),
                value: "entity-1".to_string(),
                target_entity_key: Some(ids.entity.clone()),
                target_fact_id: None,
            }],
            contexts: vec![ContextRow {
                fact_id: ids.fact,
                ordinal: 0,
                role_id: ids.role,
                axis_kind: "context".to_string(),
                value: "entity-1".to_string(),
            }],
            equivalences: vec![EquivalenceRow {
                instance_id: ids.instance,
                generator_ref_json: r#"{"kind":"explicit","id":"fixture"}"#.to_string(),
                source_entity_key: ids.entity.clone(),
                target_entity_key: ids.entity,
            }],
            overlays: vec![OverlayRow {
                ordinal: 0,
                kind: "reviewed_evidence".to_string(),
                digest: overlay,
            }],
        },
        configuration: AxpdConfiguration::default(),
        materializer_version: AXPD_MATERIALIZER_VERSION.to_string(),
    }
}

fn assert_limit(error: AxpdError, name: &str, limit: u64, actual: u64) {
    match error {
        AxpdError::LimitExceeded {
            limit_name,
            limit: found_limit,
            actual: found_actual,
        } => {
            assert_eq!(limit_name, name);
            assert_eq!(found_limit, limit);
            assert_eq!(found_actual, actual);
        }
        other => panic!("expected {name} limit error, got {other}"),
    }
}

#[test]
fn canonical_image_is_invariant_under_input_order_and_cache_absence() {
    let dir = tempdir().unwrap();
    let mut first = fixture_spec("order");
    let ids = fixture_ids("order");
    first.image.kernel_refs.push(KernelRefRow {
        wire_json: r#"{"kind":"z"}"#.to_string(),
    });
    first.image.entities.push(EntityRow {
        entity_key: ObjectBlobIdV2::from_canonical_fields(&[b"second-entity"]),
        instance_id: ids.instance,
        object_ref_json: r#"{"kind":"object_type","id":"fixture"}"#.to_string(),
        value: "entity-2".to_string(),
    });
    let mut second = first.clone();
    second.image.kernel_refs.reverse();
    second.image.entities.reverse();

    let store_a = dir.path().join("store-a");
    let store_b = dir.path().join("store-b");
    let receipt_a = publish_in_store(&store_a, first, &AxpdLimits::default()).unwrap();
    let receipt_b = publish_in_store(&store_b, second, &AxpdLimits::default()).unwrap();

    assert_eq!(receipt_a.logical_digest, receipt_b.logical_digest);
    assert_eq!(receipt_a.exact_image_digest, receipt_b.exact_image_digest);
    assert_eq!(receipt_a.materialization_id, receipt_b.materialization_id);
    assert_eq!(
        fs::read(image_path_in_store(&store_a, &receipt_a.materialization_id)).unwrap(),
        fs::read(image_path_in_store(&store_b, &receipt_b.materialization_id)).unwrap()
    );
}

#[test]
fn changing_any_semantic_row_changes_the_logical_digest() {
    let limits = AxpdLimits::default();
    let mut first = fixture_spec("row-change");
    let mut second = first.clone();
    first.validate_and_canonicalize(&limits).unwrap();
    second.image.entities[0].value = "changed".to_string();
    second.validate_and_canonicalize(&limits).unwrap();
    assert_ne!(
        first.logical_digest().unwrap(),
        second.logical_digest().unwrap()
    );
}

#[test]
fn every_count_limit_passes_at_n_and_fails_at_n_plus_one() {
    type BoundaryCase = (
        &'static str,
        Box<dyn Fn(&mut AxpdLimits)>,
        Box<dyn Fn(&mut AxpdBuildSpec)>,
    );

    let base = fixture_spec("counts");
    let cases: Vec<BoundaryCase> = vec![
        (
            "module_count",
            Box::new(|l| l.max_module_count = 1),
            Box::new(|s| {
                s.anchors
                    .ordered_module_closure
                    .push(RevisionDigestV2::from_accepted_text("module Extra\n"))
            }),
        ),
        (
            "kernel_ref_count",
            Box::new(|l| l.max_kernel_ref_count = 1),
            Box::new(|s| {
                s.image.kernel_refs.push(KernelRefRow {
                    wire_json: "z".into(),
                })
            }),
        ),
        (
            "entity_count",
            Box::new(|l| l.max_entity_count = 1),
            Box::new(|s| s.image.entities.push(s.image.entities[0].clone())),
        ),
        (
            "relation_fact_count",
            Box::new(|l| l.max_relation_fact_count = 1),
            Box::new(|s| {
                s.image
                    .relation_facts
                    .push(s.image.relation_facts[0].clone())
            }),
        ),
        (
            "projection_count",
            Box::new(|l| l.max_projection_count = 1),
            Box::new(|s| s.image.projections.push(s.image.projections[0].clone())),
        ),
        (
            "context_count",
            Box::new(|l| l.max_context_count = 1),
            Box::new(|s| s.image.contexts.push(s.image.contexts[0].clone())),
        ),
        (
            "equivalence_count",
            Box::new(|l| l.max_equivalence_count = 1),
            Box::new(|s| s.image.equivalences.push(s.image.equivalences[0].clone())),
        ),
        (
            "overlay_count",
            Box::new(|l| l.max_overlay_count = 1),
            Box::new(|s| {
                s.image.overlays.push(OverlayRow {
                    ordinal: 1,
                    kind: "second".into(),
                    digest: ObjectBlobIdV2::from_canonical_fields(&[b"second-overlay"]),
                })
            }),
        ),
    ];

    for (name, configure, add_one) in cases {
        let mut limits = AxpdLimits::default();
        configure(&mut limits);
        let mut at_n = base.clone();
        at_n.validate_and_canonicalize(&limits)
            .unwrap_or_else(|error| panic!("{name} must pass at N: {error}"));
        let mut at_n_plus_one = base.clone();
        add_one(&mut at_n_plus_one);
        let error = at_n_plus_one
            .validate_and_canonicalize(&limits)
            .expect_err("N+1 must reject");
        assert_limit(error, name, 1, 2);
    }
}

#[test]
fn string_and_projection_fanout_limits_are_exact() {
    let limits = AxpdLimits {
        max_string_bytes: 256,
        ..AxpdLimits::default()
    };
    let mut at_n = fixture_spec("string-n");
    at_n.image.entities[0].value = "x".repeat(256);
    at_n.validate_and_canonicalize(&limits).unwrap();
    let mut at_n_plus_one = fixture_spec("string-n-plus-one");
    at_n_plus_one.image.entities[0].value = "x".repeat(257);
    assert_limit(
        at_n_plus_one
            .validate_and_canonicalize(&limits)
            .expect_err("257-byte string must reject"),
        "string_bytes",
        256,
        257,
    );

    let fanout_limits = AxpdLimits {
        max_projection_fanout: 2,
        ..AxpdLimits::default()
    };
    let mut fanout_n = fixture_spec("fanout");
    let mut second = fanout_n.image.projections[0].clone();
    second.ordinal = 1;
    fanout_n.image.projections.push(second);
    fanout_n.validate_and_canonicalize(&fanout_limits).unwrap();
    let mut third = fanout_n.image.projections[0].clone();
    third.ordinal = 2;
    fanout_n.image.projections.push(third);
    assert_limit(
        fanout_n
            .validate_and_canonicalize(&fanout_limits)
            .expect_err("fanout N+1 must reject"),
        "projection_fanout",
        2,
        3,
    );
}

#[test]
fn file_and_page_limits_reject_before_hydration() {
    let dir = tempdir().unwrap();
    let receipt = publish_in_store(
        dir.path(),
        fixture_spec("file-limits"),
        &AxpdLimits::default(),
    )
    .unwrap();
    let pages = receipt.byte_len / u64::from(AXPD_PAGE_SIZE);
    assert_eq!(receipt.byte_len % u64::from(AXPD_PAGE_SIZE), 0);

    let exact = AxpdLimits {
        max_file_bytes: receipt.byte_len,
        max_page_count: pages,
        ..AxpdLimits::default()
    };
    open_from_store(dir.path(), &receipt.materialization_id, &exact).unwrap();

    let mut byte_small = exact.clone();
    byte_small.max_file_bytes -= 1;
    assert_limit(
        open_from_store(dir.path(), &receipt.materialization_id, &byte_small)
            .expect_err("file N+1 must reject"),
        "file_bytes",
        receipt.byte_len - 1,
        receipt.byte_len,
    );

    let mut page_small = exact;
    page_small.max_page_count -= 1;
    assert_limit(
        open_from_store(dir.path(), &receipt.materialization_id, &page_small)
            .expect_err("page N+1 must reject"),
        "page_count",
        pages - 1,
        pages,
    );
}

#[test]
fn header_anchor_table_digest_truncation_substitution_and_old_inputs_reject() {
    let dir = tempdir().unwrap();
    let base = fixture_spec("mutations");

    let cases = [
        ("header", "PRAGMA application_id = 1"),
        ("old-v1-schema", "PRAGMA user_version = 1"),
        (
            "anchor",
            "UPDATE materialization_meta SET accepted_tree_id = 'axi:tree:v2:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
        ),
        ("table", "DROP TABLE contexts"),
        ("row", "UPDATE entities SET value = 'tampered'"),
        (
            "digest",
            "UPDATE materialization_meta SET logical_digest = 'axi:object_blob:v2:sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
        ),
        ("overlay-order", "UPDATE overlays SET ordinal = 9"),
    ];
    for (name, sql) in cases {
        let store_root = dir.path().join(name);
        let receipt = publish_in_store(&store_root, base.clone(), &AxpdLimits::default()).unwrap();
        let image_path = image_path_in_store(&store_root, &receipt.materialization_id);
        let connection = Connection::open(&image_path).unwrap();
        connection.execute_batch(sql).unwrap();
        drop(connection);
        open_from_store(
            &store_root,
            &receipt.materialization_id,
            &AxpdLimits::default(),
        )
        .expect_err(name);
    }

    let truncated_root = dir.path().join("truncated");
    let truncated_receipt =
        publish_in_store(&truncated_root, base.clone(), &AxpdLimits::default()).unwrap();
    let truncated_path =
        image_path_in_store(&truncated_root, &truncated_receipt.materialization_id);
    let mut bytes = fs::read(&truncated_path).unwrap();
    bytes.truncate(bytes.len() / 2);
    fs::write(&truncated_path, bytes).unwrap();
    open_from_store(
        &truncated_root,
        &truncated_receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .expect_err("truncation");

    let target_root = dir.path().join("substitution-target");
    let target_receipt =
        publish_in_store(&target_root, base.clone(), &AxpdLimits::default()).unwrap();
    let target_path = image_path_in_store(&target_root, &target_receipt.materialization_id);
    let other_root = dir.path().join("substitution-source");
    let other_receipt = publish_in_store(
        &other_root,
        fixture_spec("substitution"),
        &AxpdLimits::default(),
    )
    .unwrap();
    let other_path = image_path_in_store(&other_root, &other_receipt.materialization_id);
    fs::copy(other_path, target_path).unwrap();
    open_from_store(
        &target_root,
        &target_receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .expect_err("substitution");

    for (name, bytes) in [
        ("obsolete-binary", vec![0_u8, 1, 2, 3, 4, 5]),
        ("old-sectioned-axpd", b"AXPD\x02\0\0\0obsolete".to_vec()),
    ] {
        let store_root = dir.path().join(name);
        let receipt = publish_in_store(&store_root, base.clone(), &AxpdLimits::default()).unwrap();
        let image_path = image_path_in_store(&store_root, &receipt.materialization_id);
        fs::write(image_path, bytes).unwrap();
        open_from_store(
            &store_root,
            &receipt.materialization_id,
            &AxpdLimits::default(),
        )
        .expect_err(name);
    }
}

#[test]
fn truncated_image_and_wrong_overlay_order_are_quarantined_or_rejected() {
    let dir = tempdir().unwrap();
    let spec = fixture_spec("recovery");
    let old = publish_in_store(dir.path(), spec.clone(), &AxpdLimits::default()).unwrap();
    let path = image_path_in_store(dir.path(), &old.materialization_id);
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(bytes.len() / 2);
    fs::write(&path, bytes).unwrap();
    let report = recover_in_store(dir.path(), spec, &AxpdLimits::default()).unwrap();
    assert_eq!(report.action, AxpdRecoveryAction::QuarantinedAndRebuilt);
    assert!(report.quarantined_path.as_ref().unwrap().exists());
    assert_eq!(old.logical_digest, report.receipt.logical_digest);
    open_from_store(
        dir.path(),
        &report.receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .unwrap();

    let mut wrong_order = fixture_spec("wrong-overlay");
    wrong_order.image.overlays[0].ordinal = 1;
    assert!(wrong_order
        .validate_and_canonicalize(&AxpdLimits::default())
        .is_err());
}

#[test]
fn prepublication_faults_leave_no_visible_materialization() {
    let prepublication = [
        AxpdFailurePoint::AfterPopulate,
        AxpdFailurePoint::AfterBackup,
        AxpdFailurePoint::BeforeValidation,
        AxpdFailurePoint::AfterValidation,
        AxpdFailurePoint::BeforePublish,
        AxpdFailurePoint::AfterPublish,
    ];
    for point in prepublication {
        let dir = tempdir().unwrap();
        let error = publish_in_store_with_injector(
            dir.path(),
            fixture_spec(&format!("{point:?}")),
            &AxpdLimits::default(),
            &FailAxpdAt(point),
        )
        .expect_err("fault must fire");
        assert!(matches!(error, AxpdError::Injected(found) if found == point));
        assert!(
            !dir.path().join(AXPD_MATERIALIZATIONS_DIR).exists()
                || fs::read_dir(dir.path().join(AXPD_MATERIALIZATIONS_DIR))
                    .unwrap()
                    .next()
                    .is_none()
        );
    }
}

#[test]
fn image_receipt_torn_writes_restart_as_unopenable_or_complete_and_recover() {
    for point in [
        AxpdFailurePoint::AfterImagePublish,
        AxpdFailurePoint::AfterReceiptWrite,
        AxpdFailurePoint::AfterReceiptFsync,
        AxpdFailurePoint::AfterReceiptPublish,
    ] {
        let dir = tempdir().unwrap();
        let spec = fixture_spec(&format!("receipt-fault-{point:?}"));
        let error = publish_in_store_with_injector(
            dir.path(),
            spec.clone(),
            &AxpdLimits::default(),
            &FailAxpdAt(point),
        )
        .expect_err("fault must fire");
        assert!(matches!(error, AxpdError::Injected(found) if found == point));

        let recovered = recover_in_store(dir.path(), spec, &AxpdLimits::default()).unwrap();
        if point == AxpdFailurePoint::AfterReceiptPublish {
            assert_eq!(recovered.action, AxpdRecoveryAction::Reused);
        } else {
            assert_eq!(recovered.action, AxpdRecoveryAction::QuarantinedAndRebuilt);
        }
        open_from_store(
            dir.path(),
            &recovered.receipt.materialization_id,
            &AxpdLimits::default(),
        )
        .unwrap();
    }
}

#[test]
fn store_family_receipt_is_required_and_revalidated() {
    let dir = tempdir().unwrap();
    let receipt = publish_in_store(
        dir.path(),
        fixture_spec("store-family"),
        &AxpdLimits::default(),
    )
    .unwrap();
    let image_path = image_path_in_store(dir.path(), &receipt.materialization_id);
    let receipt_path = receipt_path_in_store(dir.path(), &receipt.materialization_id);
    assert!(image_path.starts_with(dir.path().join(AXPD_MATERIALIZATIONS_DIR)));
    assert!(image_path.exists());
    assert!(receipt_path.exists());
    open_from_store(
        dir.path(),
        &receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .unwrap();

    let mut tampered: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    tampered["materialization_id"] = serde_json::Value::String(
        MaterializationIdV2::from_canonical_fields(&[b"tampered-receipt"]).to_string(),
    );
    fs::write(&receipt_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    open_from_store(
        dir.path(),
        &receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .expect_err("tampered receipt must reject");
}

#[test]
fn axi_store_rejects_materializations_bound_to_another_repository() {
    let dir = tempdir().unwrap();
    let store = store_at(dir.path());
    let mut spec = fixture_spec("wrong-repository");
    spec.anchors.repository_id =
        RepositoryIdV2::from_descriptor_bytes(b"different-repository-descriptor");
    let error = store
        .publish_axpd(spec, &AxpdLimits::default())
        .expect_err("cross-repository materialization must reject");
    assert!(matches!(
        error,
        AxpdError::AnchorMismatch {
            field: "repository_id",
            ..
        }
    ));
}

#[test]
fn same_repository_unaccepted_manifest_anchors_cannot_be_materialized() {
    let dir = tempdir().unwrap();
    publish_in_store(
        dir.path(),
        fixture_spec("accepted-manifest"),
        &AxpdLimits::default(),
    )
    .unwrap();
    let store = store_at(dir.path());
    let error = store
        .publish_axpd(fixture_spec("unaccepted-manifest"), &AxpdLimits::default())
        .expect_err("same-repository unaccepted anchors must reject");
    assert!(matches!(
        error,
        AxpdError::AnchorMismatch {
            field: "accepted_build_manifest",
            ..
        }
    ));
}

#[test]
fn cache_binding_requires_the_exact_materialization() {
    let dir = tempdir().unwrap();
    let receipt =
        publish_in_store(dir.path(), fixture_spec("cache"), &AxpdLimits::default()).unwrap();
    let valid = axiograph_store::AxpdCacheBinding {
        materialization_id: receipt.materialization_id.clone(),
        exact_image_digest: receipt.exact_image_digest.clone(),
        cache_digest: ObjectBlobIdV2::from_canonical_fields(&[b"cache-bytes"]),
    };
    valid.validate(&receipt).unwrap();
    let wrong = axiograph_store::AxpdCacheBinding {
        materialization_id: MaterializationIdV2::from_canonical_fields(&[b"wrong"]),
        ..valid
    };
    wrong.validate(&receipt).expect_err("wrong image binding");
}

#[test]
fn deletion_and_rebuild_preserve_logical_digest_and_finite_answers() {
    let dir = tempdir().unwrap();
    let spec = fixture_spec("rebuild");
    let relation = spec.image.relation_facts[0].relation_id.clone();
    let first_receipt = publish_in_store(dir.path(), spec.clone(), &AxpdLimits::default()).unwrap();
    let path = image_path_in_store(dir.path(), &first_receipt.materialization_id);
    let first = open_from_store(
        dir.path(),
        &first_receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .unwrap();
    let first_answers = first.finite_facts_by_relation(&relation, usize::MAX);
    fs::remove_file(&path).unwrap();

    let second_receipt = publish_in_store(dir.path(), spec, &AxpdLimits::default()).unwrap();
    let second = open_from_store(
        dir.path(),
        &second_receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .unwrap();
    assert_eq!(first_receipt.logical_digest, second_receipt.logical_digest);
    assert_eq!(
        first_receipt.exact_image_digest,
        second_receipt.exact_image_digest
    );
    assert_eq!(
        first_answers,
        second.finite_facts_by_relation(&relation, usize::MAX)
    );
    assert!(second.finite_facts_by_relation(&relation, 0).1);
}

#[test]
fn application_id_is_the_axpd_magic() {
    assert_eq!(AXPD_APPLICATION_ID, i32::from_be_bytes(*b"AXPD"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn arbitrary_bounded_bytes_never_panic_or_open_as_verified(bytes in prop::collection::vec(any::<u8>(), 0..8192)) {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join("valid-store");
        let receipt = publish_in_store(
            &store_root,
            fixture_spec("fuzz-expectation"),
            &AxpdLimits::default(),
        )
        .unwrap();
        let path = image_path_in_store(&store_root, &receipt.materialization_id);
        fs::write(path, bytes).unwrap();
        let result = std::panic::catch_unwind(|| {
            open_from_store(
                &store_root,
                &receipt.materialization_id,
                &AxpdLimits::default(),
            )
        });
        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap().is_err());
    }
}
