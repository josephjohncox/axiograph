use axiograph_kernel::{
    FactIdV2, FactRoleValueV2, InstanceIdV2, MaterializationIdV2, ObjectBlobIdV2, RelationIdV2,
    RoleIdV2, SchemaIdV2, SemanticKeyV2,
};
use axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT;
use axiograph_pathdb::materialization::load_verified_pathdb;
use axiograph_store::*;
use tempfile::tempdir;

fn descriptor() -> RepositoryDescriptor {
    RepositoryDescriptor::new("pathdb-tests", "fixed-test-genesis").unwrap()
}

fn exact_module() -> Vec<u8> {
    b"module Demo\n".to_vec()
}

fn supporting_objects() -> Vec<ImmutableBlob> {
    [
        (ImmutableObjectKind::KernelIr, b"kernel".as_slice()),
        (ImmutableObjectKind::CanonicalFactLog, b"facts".as_slice()),
        (
            ImmutableObjectKind::ValidationReport,
            b"validation".as_slice(),
        ),
        (
            ImmutableObjectKind::CompetencyQuestionReport,
            b"competency".as_slice(),
        ),
        (ImmutableObjectKind::TheoryReport, b"theory".as_slice()),
        (
            ImmutableObjectKind::VerificationReceipt,
            b"checker".as_slice(),
        ),
    ]
    .into_iter()
    .map(|(kind, bytes)| ImmutableBlob::new(kind, bytes.to_vec()).unwrap())
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

fn init_store(path: &std::path::Path, spec: &AxpdBuildSpec) -> AxiStore {
    let store = AxiStore::init(path, &descriptor()).unwrap();
    let repository = descriptor().repository_id().unwrap();
    let publication = ModulePublication::new(&repository, "Demo", exact_module()).unwrap();
    let tree = AcceptedTree::new(repository.clone(), vec![publication.module.clone()]).unwrap();
    let snapshot = AcceptedSnapshot::new(repository.clone(), tree.tree_id.clone(), vec![]).unwrap();
    let objects = supporting_objects();
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
        compiler_version: "canonical-test".to_string(),
        ir_version: "kernel-ir-v2-test".to_string(),
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
    assert_eq!(manifest.kernel_ir_digest, spec.anchors.kernel_ir_digest);
    assert_eq!(
        manifest.canonical_fact_log_digest,
        spec.anchors.canonical_fact_log_digest
    );
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
        Some("accept Demo".to_string()),
        "promote",
        "protected-main",
        CommitProvenance {
            source: "pathdb-materialization-tests".to_string(),
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

fn spec() -> AxpdBuildSpec {
    let repository = descriptor().repository_id().unwrap();
    let accepted_module = AcceptedModule::from_bytes(&repository, "Demo", &exact_module()).unwrap();
    let tree = AcceptedTree::new(repository.clone(), vec![accepted_module.clone()]).unwrap();
    let snapshot = AcceptedSnapshot::new(repository.clone(), tree.tree_id.clone(), vec![]).unwrap();
    let support = supporting_objects();
    let revision = accepted_module.revision_digest.clone();
    let module = accepted_module.module_id;
    let schema_key = SemanticKeyV2::derive(&module, "schema", "Demo");
    let schema = SchemaIdV2::derive(&revision, &schema_key);
    let instance = InstanceIdV2::derive(
        &revision,
        &schema,
        &SemanticKeyV2::derive(&module, "instance", "DemoInstance"),
    );
    let relation = RelationIdV2::derive(
        &revision,
        &schema,
        &SemanticKeyV2::derive(&module, "relation", "PersonFact"),
    );
    let role = RoleIdV2::derive(
        &revision,
        &schema,
        &relation,
        &SemanticKeyV2::derive(&module, "role", "person"),
        0,
    );
    let fact = FactIdV2::derive(
        &revision,
        &schema,
        &instance,
        &relation,
        &[FactRoleValueV2 {
            role_id: &role,
            value: b"Alice",
        }],
    );
    let entity_key = ObjectBlobIdV2::from_canonical_fields(&[b"entity"]);

    AxpdBuildSpec {
        anchors: AxpdAnchors {
            repository_id: repository,
            accepted_snapshot_id: snapshot.snapshot_id,
            accepted_tree_id: tree.tree_id,
            ordered_module_closure: vec![revision],
            kernel_ir_digest: object_digest(&support, ImmutableObjectKind::KernelIr),
            canonical_fact_log_digest: object_digest(
                &support,
                ImmutableObjectKind::CanonicalFactLog,
            ),
        },
        image: AxpdLogicalImage {
            entities: vec![EntityRow {
                entity_key: entity_key.clone(),
                instance_id: instance.clone(),
                object_ref_json: r#"{"kind":"object_type","id":"Person"}"#.to_string(),
                value: "Alice".to_string(),
            }],
            relation_facts: vec![RelationFactRow {
                fact_id: fact.clone(),
                instance_id: instance,
                relation_id: relation,
                local_label: Some("alice-fact".to_string()),
            }],
            projections: vec![ProjectionRow {
                fact_id: fact,
                ordinal: 0,
                role_id: role,
                value_kind: "object_element".to_string(),
                value: "Alice".to_string(),
                target_entity_key: Some(entity_key),
                target_fact_id: None,
            }],
            ..AxpdLogicalImage::default()
        },
        configuration: AxpdConfiguration::default(),
        materializer_version: AXPD_MATERIALIZER_VERSION.to_string(),
    }
}

#[test]
fn pathdb_hydrates_only_from_store_verified_materialization() {
    let store = tempdir().unwrap();
    let materialization = spec();
    let axi_store = init_store(store.path(), &materialization);
    let receipt = axi_store
        .publish_axpd(materialization, &AxpdLimits::default())
        .unwrap();
    let mut loaded = load_verified_pathdb(
        store.path(),
        &receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .unwrap();

    assert_eq!(loaded.receipt(), &receipt);
    loaded.configure_path_index_cache(7, None);
    assert_eq!(loaded.db().path_index_lru_capacity(), 7);
    assert_eq!(loaded.receipt(), &receipt);
    assert_eq!(loaded.db().entities.len(), 2);
    assert_eq!(loaded.db().relations.len(), 1);
    let alice = loaded
        .db()
        .find_by_type(r#"{"kind":"object_type","id":"Person"}"#)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    assert_eq!(
        loaded.db().get_entity(alice).unwrap().attrs["axiograph.value"],
        "Alice"
    );
}

#[test]
fn verified_hydration_preserves_finite_context_axis_and_reversible_generator() {
    let store = tempdir().unwrap();
    let mut materialization = spec();
    let axi_store = init_store(store.path(), &materialization);
    let fact_id = materialization.image.relation_facts[0].fact_id.clone();
    let relation_id = materialization.image.relation_facts[0].relation_id.clone();
    let instance_id = materialization.image.relation_facts[0].instance_id.clone();
    let role_id = materialization.image.projections[0].role_id.clone();
    let alice_key = materialization.image.entities[0].entity_key.clone();
    let context_key = ObjectBlobIdV2::from_canonical_fields(&[b"accepted-context"]);
    materialization.image.entities.push(EntityRow {
        entity_key: context_key.clone(),
        instance_id: instance_id.clone(),
        object_ref_json: r#"{"kind":"object_type","id":"Context"}"#.to_string(),
        value: "Accepted".to_string(),
    });
    materialization.image.projections.push(ProjectionRow {
        fact_id: fact_id.clone(),
        ordinal: 1,
        role_id: role_id.clone(),
        value_kind: "object_element".to_string(),
        value: "Accepted".to_string(),
        target_entity_key: Some(context_key.clone()),
        target_fact_id: None,
    });
    materialization.image.contexts.push(ContextRow {
        fact_id,
        ordinal: 1,
        role_id,
        axis_kind: "context".to_string(),
        value: "Accepted".to_string(),
    });
    materialization.image.equivalences.push(EquivalenceRow {
        instance_id,
        generator_ref_json: r#"{"kind":"reversible_generator","id":"context-transport"}"#
            .to_string(),
        source_entity_key: alice_key,
        target_entity_key: context_key,
    });

    let receipt = axi_store
        .publish_axpd(materialization, &AxpdLimits::default())
        .unwrap();
    let loaded = load_verified_pathdb(
        store.path(),
        &receipt.materialization_id,
        &AxpdLimits::default(),
    )
    .unwrap();
    let context = loaded
        .db()
        .find_by_type(r#"{"kind":"object_type","id":"Context"}"#)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    let alice = loaded
        .db()
        .find_by_type(r#"{"kind":"object_type","id":"Person"}"#)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    let fact_type = format!("axiograph.relation_fact:{relation_id}");
    let fact = loaded
        .db()
        .find_by_type(&fact_type)
        .unwrap()
        .iter()
        .next()
        .unwrap();
    let scope_relation = loaded.db().interner.id_of(REL_AXI_FACT_IN_CONTEXT).unwrap();
    assert!(loaded
        .db()
        .relations
        .has_edge(fact, scope_relation, context));
    assert!(loaded.db().fact_nodes_by_context(context).contains(fact));
    assert!(loaded
        .db()
        .find_equivalent(alice)
        .iter()
        .any(|(target, _generator)| *target == context));
}

#[test]
fn unknown_materialization_id_never_falls_back_to_a_bare_image() {
    let store = tempdir().unwrap();
    let materialization = spec();
    let axi_store = init_store(store.path(), &materialization);
    let receipt = axi_store
        .publish_axpd(materialization, &AxpdLimits::default())
        .unwrap();
    let unknown = MaterializationIdV2::from_canonical_fields(&[b"unknown"]);
    assert_ne!(unknown, receipt.materialization_id);
    assert!(load_verified_pathdb(store.path(), &unknown, &AxpdLimits::default()).is_err());
}
