use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, CommitIdV2, CompiledKernelSnapshot,
    KernelCompilationRequest, ObjectBlobIdV2, RepositoryIdV2, SnapshotIdV2,
};
use axiograph_store::*;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

fn descriptor(label: &str) -> RepositoryDescriptor {
    RepositoryDescriptor::new(format!("repository-{label}"), format!("nonce-{label}"))
        .expect("valid descriptor")
}

fn supporting_objects(label: &str, kernel_ir: Vec<u8>) -> Vec<ImmutableBlob> {
    let mut objects = vec![ImmutableBlob::new(ImmutableObjectKind::KernelIr, kernel_ir).unwrap()];
    objects.extend(
        [
            (ImmutableObjectKind::CanonicalFactLog, "facts"),
            (ImmutableObjectKind::ValidationReport, "validation"),
            (
                ImmutableObjectKind::CompetencyQuestionReport,
                "competency-questions",
            ),
            (ImmutableObjectKind::TheoryReport, "theory"),
            (ImmutableObjectKind::VerificationReceipt, "checker-receipt"),
        ]
        .into_iter()
        .map(|(kind, suffix)| {
            ImmutableBlob::new(kind, format!("{label}-{suffix}").into_bytes()).unwrap()
        }),
    );
    objects
}

fn object(objects: &[ImmutableBlob], kind: ImmutableObjectKind) -> ObjectBlobIdV2 {
    objects
        .iter()
        .find(|value| value.kind == kind)
        .expect("supporting object")
        .digest
        .clone()
}

fn compile_candidate(
    repository_id: &RepositoryIdV2,
    snapshot: &AcceptedSnapshot,
    tree: &AcceptedTree,
    modules: &[ModulePublication],
) -> CompiledKernelSnapshot {
    let root = tree.modules.last().expect("root module");
    CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: repository_id.clone(),
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        root_module: root.module_name.clone(),
        modules: modules
            .iter()
            .map(|module| CanonicalModuleSource::parse(module.exact_bytes.clone()).unwrap())
            .collect(),
    })
    .unwrap()
}

fn candidate_from_plan(plan: &PromotionPlan) -> TypedCandidatePayloadV2 {
    let compiled = compile_candidate(
        &plan.tree.repository_id,
        &plan.snapshot,
        &plan.tree,
        &plan.modules,
    );
    TypedCandidatePayloadV2::checked(
        plan.snapshot.snapshot_id.clone(),
        plan.tree.tree_id.clone(),
        compiled.ir().root_module_id().clone(),
        plan.manifest.kernel_ir_digest.clone(),
        compiled
            .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Merge)
            .unwrap(),
        compiled.payload_fingerprints().unwrap(),
    )
    .unwrap()
}

fn gate_reports(plan: &PromotionPlan) -> CandidateGateReportsV2 {
    CandidateGateReportsV2 {
        canonical_validation: plan.manifest.validation_report_digest.clone(),
        competency_questions: plan.manifest.competency_question_report_digest.clone(),
        trust: plan.manifest.trusted_checker_receipt_digest.clone(),
        runtime_theory: plan.manifest.runtime_theory_report_digest.clone(),
    }
}

fn reviewed_parent(plan: &PromotionPlan) -> ReviewedParentCandidateV2 {
    ReviewedParentCandidateV2 {
        commit_id: plan.commit.commit_id.clone(),
        candidate: candidate_from_plan(plan),
        gates: gate_reports(plan),
        reviewer: "reviewer@example.test".to_string(),
        reviewed_at_unix_secs: 1_700_000_001,
    }
}

fn union_decisions(
    left: &ReviewedParentCandidateV2,
    right: &ReviewedParentCandidateV2,
    merged: &ReviewedMergedCandidateV2,
) -> Vec<TypedReconciliationDecisionV2> {
    let left_set = left
        .candidate
        .payloads
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let right_set = right
        .candidate
        .payloads
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let merged_set = merged
        .candidate
        .payloads
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let mut decisions = Vec::new();
    for source in &left_set {
        let origin = if right_set.contains(source) {
            CandidateOriginV2::Both
        } else {
            CandidateOriginV2::Left
        };
        if merged_set.contains(source) {
            decisions.push(TypedReconciliationDecisionV2::Keep {
                origin,
                source: source.clone(),
                target: source.clone(),
                rationale: "exact typed payload retained".to_string(),
            });
        } else {
            decisions.push(TypedReconciliationDecisionV2::Drop {
                origin,
                source: source.clone(),
                rationale: "reviewed parent payload explicitly omitted".to_string(),
            });
        }
    }
    for source in right_set.difference(&left_set) {
        if merged_set.contains(source) {
            decisions.push(TypedReconciliationDecisionV2::Keep {
                origin: CandidateOriginV2::Right,
                source: source.clone(),
                target: source.clone(),
                rationale: "exact typed payload retained".to_string(),
            });
        } else {
            decisions.push(TypedReconciliationDecisionV2::Drop {
                origin: CandidateOriginV2::Right,
                source: source.clone(),
                rationale: "reviewed parent payload explicitly omitted".to_string(),
            });
        }
    }
    for target in merged_set.difference(&left_set.union(&right_set).cloned().collect()) {
        decisions.push(TypedReconciliationDecisionV2::Introduce {
            target: target.clone(),
            rationale: "reviewed merged payload explicitly introduced".to_string(),
        });
    }
    decisions
}

fn build_plan(
    repository_id: &RepositoryIdV2,
    label: &str,
    ordered_parents: &[(CommitIdV2, SnapshotIdV2)],
    reconciliation_parents: Option<(CommitIdV2, &PromotionPlan, &PromotionPlan)>,
) -> PromotionPlan {
    let exact = format!("module {label}\n\nschema {label}:\n  object Item\n").into_bytes();
    let publication = ModulePublication::new(repository_id, label, exact).unwrap();
    let tree = AcceptedTree::new(repository_id.clone(), vec![publication.module.clone()]).unwrap();
    let snapshot = AcceptedSnapshot::new(
        repository_id.clone(),
        tree.tree_id.clone(),
        ordered_parents
            .iter()
            .map(|(_, snapshot)| snapshot.clone())
            .collect(),
    )
    .unwrap();
    let compiled = compile_candidate(
        repository_id,
        &snapshot,
        &tree,
        std::slice::from_ref(&publication),
    );
    let kernel_ir = serde_json::to_vec(compiled.ir()).unwrap();
    let mut objects = supporting_objects(label, kernel_ir);
    let validation = object(&objects, ImmutableObjectKind::ValidationReport);
    let competency = object(&objects, ImmutableObjectKind::CompetencyQuestionReport);
    let theory = object(&objects, ImmutableObjectKind::TheoryReport);
    let receipt = object(&objects, ImmutableObjectKind::VerificationReceipt);
    let manifest = AcceptedBuildManifest {
        format: BUILD_MANIFEST_FORMAT.to_string(),
        version: FORMAT_VERSION,
        repository_id: repository_id.clone(),
        accepted_tree_id: tree.tree_id.clone(),
        accepted_snapshot_id: snapshot.snapshot_id.clone(),
        ordered_module_closure: vec![publication.module.revision_digest.clone()],
        compiler_version: "canonical-compiler-test".to_string(),
        ir_version: "kernel-ir-test".to_string(),
        kernel_ir_digest: object(&objects, ImmutableObjectKind::KernelIr),
        canonical_fact_log_digest: object(&objects, ImmutableObjectKind::CanonicalFactLog),
        validation_report_digest: validation.clone(),
        competency_question_report_digest: competency.clone(),
        runtime_theory_report_digest: theory.clone(),
        trusted_checker_receipt_digest: receipt.clone(),
        non_claims: required_non_claims(),
    };
    let current_candidate = TypedCandidatePayloadV2::checked(
        snapshot.snapshot_id.clone(),
        tree.tree_id.clone(),
        compiled.ir().root_module_id().clone(),
        manifest.kernel_ir_digest.clone(),
        compiled
            .require_finite_theory_gate(axiograph_kernel::FiniteTheoryGateConsumerIr::Merge)
            .unwrap(),
        compiled.payload_fingerprints().unwrap(),
    )
    .unwrap();
    let reconciliation = reconciliation_parents.map(|(base, left_plan, right_plan)| {
        let preview = ImmutableBlob::new(
            ImmutableObjectKind::Evidence,
            format!("{label}-reconciliation-preview").into_bytes(),
        )
        .unwrap();
        let left = reviewed_parent(left_plan);
        let right = reviewed_parent(right_plan);
        let merged = ReviewedMergedCandidateV2 {
            candidate: current_candidate.clone(),
            gates: CandidateGateReportsV2 {
                canonical_validation: validation.clone(),
                competency_questions: competency.clone(),
                trust: receipt.clone(),
                runtime_theory: theory.clone(),
            },
            reviewer: "reviewer@example.test".to_string(),
            reviewed_at_unix_secs: 1_700_000_002,
        };
        let decisions = union_decisions(&left, &right, &merged);
        let result = SemReconciliationV2::new(
            repository_id.clone(),
            base,
            left,
            right,
            merged,
            decisions,
            preview.digest.clone(),
            ReconciliationOutcomeV2::Materialized,
        )
        .unwrap();
        objects.push(preview);
        result
    });
    let kind = if reconciliation.is_some() {
        CommitKind::Merge
    } else {
        CommitKind::Normal
    };
    let commit = SemCommitV2::new(
        repository_id.clone(),
        kind,
        ordered_parents
            .iter()
            .map(|(commit, _)| commit.clone())
            .collect(),
        tree.tree_id.clone(),
        snapshot.snapshot_id.clone(),
        manifest.digest().unwrap(),
        reconciliation
            .as_ref()
            .map(|value| value.reconciliation_id.clone()),
        "operator@example.test",
        1_700_000_000,
        Some(format!("promote {label}")),
        "promote",
        "protected-main",
        CommitProvenance {
            source: "axiograph-store-tests".to_string(),
            command: Some(format!("test {label}")),
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
                report_digest: receipt,
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
    PromotionPlan {
        modules: vec![publication],
        objects,
        tree,
        snapshot,
        manifest,
        reconciliation,
        commit,
        ref_updates: vec![],
    }
}

fn rebuild_commit(
    plan: &PromotionPlan,
    reconciliation_id: Option<axiograph_kernel::ReconciliationIdV2>,
    gates: Vec<PromotionGate>,
) -> SemCommitV2 {
    SemCommitV2::new(
        plan.commit.repository_id.clone(),
        plan.commit.kind,
        plan.commit.ordered_parents.clone(),
        plan.commit.accepted_tree_id.clone(),
        plan.commit.accepted_snapshot_id.clone(),
        plan.commit.build_manifest_digest.clone(),
        reconciliation_id,
        plan.commit.author.clone(),
        plan.commit.created_at_unix_secs,
        plan.commit.message.clone(),
        plan.commit.action.clone(),
        plan.commit.policy.clone(),
        plan.commit.provenance.clone(),
        plan.commit.delta.clone(),
        gates,
        plan.commit.attachments.clone(),
        plan.commit.lifecycle_events.clone(),
    )
    .unwrap()
}

fn initialized(path: &Path, label: &str) -> (AxiStore, RepositoryIdV2) {
    let descriptor = descriptor(label);
    let repository_id = descriptor.repository_id().unwrap();
    let store = AxiStore::init(path, &descriptor).unwrap();
    (store, repository_id)
}

fn object_path(root: &Path, id: &str) -> PathBuf {
    root.join("objects/sha256")
        .join(id.rsplit(':').next().unwrap())
}

fn immutable_objects(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(root.join("objects/sha256"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn catalog_uses_wal_full_foreign_keys_and_strict_authority_tables() {
    let directory = tempdir().unwrap();
    let (_store, _repository_id) = initialized(directory.path(), "pragmas");
    let connection = Connection::open(directory.path().join("catalog.sqlite")).unwrap();
    let journal: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    let synchronous: i64 = connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .unwrap();
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal.to_lowercase(), "wal");
    assert_eq!(synchronous, 2, "SQLite FULL synchronous is numeric mode 2");
    assert_eq!(foreign_keys, 1);
    let application_id: i64 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .unwrap();
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let page_size: i64 = connection
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .unwrap();
    assert_eq!(application_id, i64::from(AXI_STORE_APPLICATION_ID));
    assert_eq!(user_version, i64::from(AXI_STORE_SCHEMA_VERSION));
    assert_eq!(page_size, i64::from(AXI_STORE_PAGE_SIZE));

    let strict_tables: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_list WHERE schema='main' AND name IN ('objects','repository','store_state','refs','audit_events','semantic_commits','accepted_snapshots') AND strict=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(strict_tables, 7);
}

#[test]
fn identical_promotions_have_identical_ids_state_digest_refs_and_immutable_bytes() {
    let left = tempdir().unwrap();
    let right = tempdir().unwrap();
    let (left_store, left_repository) = initialized(left.path(), "deterministic");
    let (right_store, right_repository) = initialized(right.path(), "deterministic");
    assert_eq!(left_repository, right_repository);
    let left_plan = build_plan(&left_repository, "Deterministic", &[], None);
    let right_plan = build_plan(&right_repository, "Deterministic", &[], None);
    let left_status = left_store.promote(0, &left_plan).unwrap();
    let right_status = right_store.promote(0, &right_plan).unwrap();

    assert_eq!(left_plan.tree.tree_id, right_plan.tree.tree_id);
    assert_eq!(
        left_plan.snapshot.snapshot_id,
        right_plan.snapshot.snapshot_id
    );
    assert_eq!(left_plan.commit.commit_id, right_plan.commit.commit_id);
    assert_eq!(left_status.state_digest, right_status.state_digest);
    assert_eq!(left_status.refs, right_status.refs);
    assert_eq!(
        immutable_objects(left.path()),
        immutable_objects(right.path())
    );
}

#[test]
fn catalog_identity_schema_and_size_limits_fail_closed_before_object_hydration() {
    for (case, mutation) in [
        ("application-id", "PRAGMA application_id=0;"),
        ("old-v1-schema", "PRAGMA user_version=1;"),
        (
            "table-substitution",
            "CREATE TABLE rogue(value TEXT NOT NULL) STRICT;",
        ),
    ] {
        let directory = tempdir().unwrap();
        let (store, _) = initialized(directory.path(), case);
        drop(store);
        let connection = Connection::open(directory.path().join("catalog.sqlite")).unwrap();
        connection.execute_batch(mutation).unwrap();
        drop(connection);
        assert!(
            AxiStore::open(directory.path()).is_err(),
            "catalog mutation `{case}` was accepted"
        );
    }

    let directory = tempdir().unwrap();
    let (store, _) = initialized(directory.path(), "catalog-size");
    drop(store);
    fs::OpenOptions::new()
        .write(true)
        .open(directory.path().join("catalog.sqlite"))
        .unwrap()
        .set_len(AXI_STORE_MAX_CATALOG_BYTES + 1)
        .unwrap();
    assert!(matches!(
        AxiStore::open(directory.path()),
        Err(AxiStoreError::LimitExceeded {
            name: "catalog_bytes",
            ..
        })
    ));
}

#[test]
fn oversized_immutable_object_rejects_before_reading_or_decoding() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "object-size");
    let plan = build_plan(&repository_id, "ObjectSize", &[], None);
    store.promote(0, &plan).unwrap();
    drop(store);

    let module_path = object_path(
        directory.path(),
        plan.modules[0].module.revision_digest.as_str(),
    );
    fs::OpenOptions::new()
        .write(true)
        .open(module_path)
        .unwrap()
        .set_len(AXI_STORE_MAX_OBJECT_BYTES + 1)
        .unwrap();
    assert!(matches!(
        AxiStore::open(directory.path()),
        Err(AxiStoreError::LimitExceeded {
            name: "immutable_object_bytes",
            ..
        })
    ));
}

#[test]
fn audit_sequence_gap_and_checksum_substitution_reject_on_restart() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "sequence-gap");
    let plan = build_plan(&repository_id, "SequenceGap", &[], None);
    store.promote(0, &plan).unwrap();
    drop(store);
    let connection = Connection::open(directory.path().join("catalog.sqlite")).unwrap();
    connection
        .execute("UPDATE audit_events SET sequence=2 WHERE sequence=1", [])
        .unwrap();
    drop(connection);
    assert!(AxiStore::open(directory.path()).is_err());

    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "checksum-substitution");
    let plan = build_plan(&repository_id, "ChecksumSubstitution", &[], None);
    store.promote(0, &plan).unwrap();
    drop(store);
    let substituted = ObjectBlobIdV2::from_canonical_fields(&[b"substituted-audit-checksum"]);
    let connection = Connection::open(directory.path().join("catalog.sqlite")).unwrap();
    connection
        .execute(
            "UPDATE audit_events SET event_id=?1 WHERE sequence=1",
            [substituted.as_str()],
        )
        .unwrap();
    drop(connection);
    assert!(AxiStore::open(directory.path()).is_err());
}

#[test]
fn objects_first_failures_restart_as_exactly_old_or_new_complete_state() {
    let failure_points = (0..11)
        .flat_map(|index| {
            [
                FailurePoint::ObjectWrite(index),
                FailurePoint::ObjectFsync(index),
                FailurePoint::ObjectPublish(index),
            ]
        })
        .chain([
            FailurePoint::CatalogBegin,
            FailurePoint::CatalogEvent,
            FailurePoint::CatalogState,
            FailurePoint::CatalogCommit,
        ])
        .collect::<Vec<_>>();

    for (case, point) in failure_points.into_iter().enumerate() {
        let directory = tempdir().unwrap();
        let (store, repository_id) = initialized(directory.path(), &format!("failure-{case}"));
        let plan = build_plan(&repository_id, &format!("Failure{case}"), &[], None);
        let result = store.promote_with_injector(0, &plan, &FailAt(point.clone()));
        assert!(matches!(result, Err(AxiStoreError::Injected(ref actual)) if actual == &point));
        drop(store);

        let reopened =
            AxiStore::open(directory.path()).expect("restart must find a complete state");
        let status = reopened.status().unwrap();
        if point == FailurePoint::CatalogCommit {
            assert_eq!(status.state.generation, 1);
            assert_eq!(
                status.state.accepted_commit_id.as_ref(),
                Some(&plan.commit.commit_id)
            );
        } else {
            assert_eq!(status.state.generation, 0);
            assert!(status.state.accepted_commit_id.is_none());
        }
    }
}

#[test]
fn concurrent_expected_generation_has_one_winner_and_one_typed_stale_conflict() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "race");
    let plan = Arc::new(build_plan(&repository_id, "Race", &[], None));
    let barrier = Arc::new(Barrier::new(3));
    let handles = (0..2)
        .map(|_| {
            let store = store.clone();
            let plan = Arc::clone(&plan);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                store.promote(0, &plan)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(AxiStoreError::StaleState {
                    expected: 0,
                    actual: 1
                })
            ))
            .count(),
        1
    );
    let status = AxiStore::open(directory.path()).unwrap().status().unwrap();
    assert_eq!(status.state.generation, 1);
    assert_eq!(
        status.state.accepted_commit_id.as_ref(),
        Some(&plan.commit.commit_id)
    );
    assert_eq!(
        status
            .refs
            .iter()
            .find(|reference| reference.name == "heads/main")
            .map(|reference| &reference.target),
        Some(&plan.commit.commit_id)
    );
}

struct DagFixture {
    store: AxiStore,
    a: PromotionPlan,
    b: PromotionPlan,
    c: PromotionPlan,
    d: PromotionPlan,
    e: PromotionPlan,
}

fn criss_cross_store(path: &Path) -> DagFixture {
    let (store, repository_id) = initialized(path, "dag");
    let a = build_plan(&repository_id, "A", &[], None);
    let status = store.promote(0, &a).unwrap();
    let a_parent = [(a.commit.commit_id.clone(), a.snapshot.snapshot_id.clone())];
    let b = build_plan(&repository_id, "B", &a_parent, None);
    let status = store
        .publish_candidate(status.state.generation, "heads/review/left", None, &b)
        .unwrap();
    let c = build_plan(&repository_id, "C", &a_parent, None);
    let status = store
        .publish_candidate(status.state.generation, "heads/review/right", None, &c)
        .unwrap();
    let d_parents = [
        (b.commit.commit_id.clone(), b.snapshot.snapshot_id.clone()),
        (c.commit.commit_id.clone(), c.snapshot.snapshot_id.clone()),
    ];
    let d = build_plan(
        &repository_id,
        "D",
        &d_parents,
        Some((a.commit.commit_id.clone(), &b, &c)),
    );
    let status = store
        .publish_candidate(
            status.state.generation,
            "heads/review/left",
            Some(&b.commit.commit_id),
            &d,
        )
        .unwrap();
    let e_parents = [
        (c.commit.commit_id.clone(), c.snapshot.snapshot_id.clone()),
        (b.commit.commit_id.clone(), b.snapshot.snapshot_id.clone()),
    ];
    let e = build_plan(
        &repository_id,
        "E",
        &e_parents,
        Some((a.commit.commit_id.clone(), &c, &b)),
    );
    store
        .publish_candidate(
            status.state.generation,
            "heads/review/right",
            Some(&c.commit.commit_id),
            &e,
        )
        .unwrap();
    DagFixture {
        store,
        a,
        b,
        c,
        d,
        e,
    }
}

#[test]
fn merge_bases_use_maximal_common_ancestor_antichain_and_fail_on_criss_cross_ambiguity() {
    let directory = tempdir().unwrap();
    let fixture = criss_cross_store(directory.path());
    assert_eq!(
        fixture
            .store
            .merge_base(&fixture.d.commit.commit_id, &fixture.b.commit.commit_id)
            .unwrap(),
        fixture.b.commit.commit_id
    );
    let candidates = fixture
        .store
        .maximal_common_ancestors(&fixture.d.commit.commit_id, &fixture.e.commit.commit_id)
        .unwrap();
    assert_eq!(
        candidates,
        vec![
            fixture.b.commit.commit_id.clone(),
            fixture.c.commit.commit_id.clone()
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
    );
    assert!(matches!(
        fixture
            .store
            .merge_base(&fixture.d.commit.commit_id, &fixture.e.commit.commit_id),
        Err(AxiStoreError::AmbiguousMergeBase { candidates }) if candidates.len() == 2
    ));
}

#[test]
fn authenticated_exact_two_parent_merge_materializes_on_protected_main() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "authenticated-merge");
    let target = build_plan(&repository_id, "Target", &[], None);
    let status = store.promote(0, &target).unwrap();
    let parent = [(
        target.commit.commit_id.clone(),
        target.snapshot.snapshot_id.clone(),
    )];
    let source = build_plan(&repository_id, "Target", &parent, None);
    let status = store
        .publish_candidate(
            status.state.generation,
            "heads/review/source",
            None,
            &source,
        )
        .unwrap();
    let merge_parents = [
        (
            target.commit.commit_id.clone(),
            target.snapshot.snapshot_id.clone(),
        ),
        (
            source.commit.commit_id.clone(),
            source.snapshot.snapshot_id.clone(),
        ),
    ];
    let merge = build_plan(
        &repository_id,
        "Target",
        &merge_parents,
        Some((target.commit.commit_id.clone(), &target, &source)),
    );
    assert!(merge
        .reconciliation
        .as_ref()
        .unwrap()
        .decisions
        .iter()
        .all(|decision| matches!(
            decision,
            TypedReconciliationDecisionV2::Keep {
                origin: CandidateOriginV2::Both,
                ..
            }
        )));

    assert!(store.promote(status.state.generation, &merge).is_err());
    let merged = store
        .materialize_merge(
            status.state.generation,
            "heads/review/source",
            &source.commit.commit_id,
            &merge,
        )
        .unwrap();
    assert_eq!(
        merged.state.accepted_commit_id,
        Some(merge.commit.commit_id.clone())
    );
    assert_eq!(merge.commit.ordered_parents.len(), 2);
    assert_eq!(merge.snapshot.ordered_parents.len(), 2);
    assert_eq!(
        merge.reconciliation.as_ref().unwrap().outcome,
        ReconciliationOutcomeV2::Materialized
    );
}

#[test]
fn reviewed_transport_decision_requires_and_persists_its_witness() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "transport-merge");
    let target = build_plan(&repository_id, "Target", &[], None);
    let status = store.promote(0, &target).unwrap();
    let parent = [(
        target.commit.commit_id.clone(),
        target.snapshot.snapshot_id.clone(),
    )];
    let source = build_plan(&repository_id, "Source", &parent, None);
    let status = store
        .publish_candidate(
            status.state.generation,
            "heads/review/source",
            None,
            &source,
        )
        .unwrap();
    let merge_parents = [
        (
            target.commit.commit_id.clone(),
            target.snapshot.snapshot_id.clone(),
        ),
        (
            source.commit.commit_id.clone(),
            source.snapshot.snapshot_id.clone(),
        ),
    ];
    let mut plan = build_plan(
        &repository_id,
        "Merged",
        &merge_parents,
        Some((target.commit.commit_id.clone(), &target, &source)),
    );
    let original = plan.reconciliation.as_ref().unwrap();
    let drop_index = original
        .decisions
        .iter()
        .position(|decision| matches!(decision, TypedReconciliationDecisionV2::Drop { .. }))
        .unwrap();
    let introduce_index = original
        .decisions
        .iter()
        .position(|decision| matches!(decision, TypedReconciliationDecisionV2::Introduce { .. }))
        .unwrap();
    let (origin, source_payload) = match &original.decisions[drop_index] {
        TypedReconciliationDecisionV2::Drop { origin, source, .. } => (*origin, source.clone()),
        _ => unreachable!(),
    };
    let target_payload = match &original.decisions[introduce_index] {
        TypedReconciliationDecisionV2::Introduce { target, .. } => target.clone(),
        _ => unreachable!(),
    };
    let witness = ImmutableBlob::new(
        ImmutableObjectKind::Certificate,
        b"checked-finite-transport-witness".to_vec(),
    )
    .unwrap();
    let mut decisions = original.decisions.clone();
    let mut removal_indexes = [drop_index, introduce_index];
    removal_indexes.sort_unstable();
    for index in removal_indexes.into_iter().rev() {
        decisions.remove(index);
    }
    decisions.push(TypedReconciliationDecisionV2::Transport {
        origin,
        source: source_payload,
        target: target_payload,
        witness_digest: witness.digest.clone(),
        rationale: "reviewed finite typed transport".to_string(),
    });
    let reconciliation = SemReconciliationV2::new(
        repository_id,
        original.base_commit_id.clone(),
        original.left.clone(),
        original.right.clone(),
        original.merged.clone(),
        decisions,
        original.preview_digest.clone(),
        ReconciliationOutcomeV2::Materialized,
    )
    .unwrap();
    plan.commit = rebuild_commit(
        &plan,
        Some(reconciliation.reconciliation_id.clone()),
        plan.commit.gates.clone(),
    );
    plan.reconciliation = Some(reconciliation);
    assert!(store
        .materialize_merge(
            status.state.generation,
            "heads/review/source",
            &source.commit.commit_id,
            &plan,
        )
        .is_err());
    plan.objects.push(witness);

    let merged = store
        .materialize_merge(
            status.state.generation,
            "heads/review/source",
            &source.commit.commit_id,
            &plan,
        )
        .unwrap();
    assert_eq!(merged.state.accepted_commit_id, Some(plan.commit.commit_id));
}

#[test]
fn merge_rejects_stale_source_ref_forged_payloads_and_wrong_trust_gate() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "adversarial-merge");
    let target = build_plan(&repository_id, "Target", &[], None);
    let status = store.promote(0, &target).unwrap();
    let parent = [(
        target.commit.commit_id.clone(),
        target.snapshot.snapshot_id.clone(),
    )];
    let source = build_plan(&repository_id, "Source", &parent, None);
    let status = store
        .publish_candidate(
            status.state.generation,
            "heads/review/source",
            None,
            &source,
        )
        .unwrap();
    let merge_parents = [
        (
            target.commit.commit_id.clone(),
            target.snapshot.snapshot_id.clone(),
        ),
        (
            source.commit.commit_id.clone(),
            source.snapshot.snapshot_id.clone(),
        ),
    ];
    let merge = build_plan(
        &repository_id,
        "Merged",
        &merge_parents,
        Some((target.commit.commit_id.clone(), &target, &source)),
    );
    assert!(store
        .materialize_merge(
            status.state.generation,
            "heads/review/missing",
            &source.commit.commit_id,
            &merge,
        )
        .is_err());

    let original = merge.reconciliation.as_ref().unwrap();
    let mut forged_left = original.left.clone();
    forged_left.candidate.payloads[0].payload_fingerprint =
        ObjectBlobIdV2::from_canonical_fields(&[b"forged-stable-address-payload"]);
    forged_left.candidate.payloads.sort();
    let forged_merged = original.merged.clone();
    let forged_decisions = union_decisions(&forged_left, &original.right, &forged_merged);
    let forged_reconciliation = SemReconciliationV2::new(
        repository_id.clone(),
        original.base_commit_id.clone(),
        forged_left,
        original.right.clone(),
        forged_merged,
        forged_decisions,
        original.preview_digest.clone(),
        ReconciliationOutcomeV2::Materialized,
    )
    .unwrap();
    let mut forged_plan = merge.clone();
    forged_plan.commit = rebuild_commit(
        &forged_plan,
        Some(forged_reconciliation.reconciliation_id.clone()),
        forged_plan.commit.gates.clone(),
    );
    forged_plan.reconciliation = Some(forged_reconciliation);
    let error = store
        .materialize_merge(
            status.state.generation,
            "heads/review/source",
            &source.commit.commit_id,
            &forged_plan,
        )
        .expect_err("review attestation cannot forge compiled payload fingerprints");
    assert!(error.to_string().contains("payload fingerprints"));

    let mut forged_gate_merged = original.merged.clone();
    forged_gate_merged
        .candidate
        .finite_theory_gate
        .coverage
        .path_explanations_replayed += 1;
    let forged_gate_reconciliation = SemReconciliationV2::new(
        repository_id.clone(),
        original.base_commit_id.clone(),
        original.left.clone(),
        original.right.clone(),
        forged_gate_merged.clone(),
        union_decisions(&original.left, &original.right, &forged_gate_merged),
        original.preview_digest.clone(),
        ReconciliationOutcomeV2::Materialized,
    )
    .unwrap();
    let mut forged_gate_plan = merge.clone();
    forged_gate_plan.commit = rebuild_commit(
        &forged_gate_plan,
        Some(forged_gate_reconciliation.reconciliation_id.clone()),
        forged_gate_plan.commit.gates.clone(),
    );
    forged_gate_plan.reconciliation = Some(forged_gate_reconciliation);
    let error = store
        .materialize_merge(
            status.state.generation,
            "heads/review/source",
            &source.commit.commit_id,
            &forged_gate_plan,
        )
        .expect_err("merge must reproduce typed finite-theory scope and coverage");
    assert!(error.to_string().contains("finite-theory scope"));

    let mut wrong_gates = merge.commit.gates.clone();
    wrong_gates
        .iter_mut()
        .find(|gate| gate.kind == GateKind::Trust)
        .unwrap()
        .report_digest = ObjectBlobIdV2::from_canonical_fields(&[b"wrong-trust-receipt"]);
    let mut wrong_gate_plan = merge.clone();
    wrong_gate_plan.commit = rebuild_commit(
        &wrong_gate_plan,
        wrong_gate_plan.commit.reconciliation_id.clone(),
        wrong_gates,
    );
    assert!(store
        .materialize_merge(
            status.state.generation,
            "heads/review/source",
            &source.commit.commit_id,
            &wrong_gate_plan,
        )
        .is_err());
}

#[test]
fn typed_union_rejects_unaccounted_and_address_only_payload_changes() {
    let directory = tempdir().unwrap();
    let (_store, repository_id) = initialized(directory.path(), "union-rejection");
    let target = build_plan(&repository_id, "Target", &[], None);
    let parent = [(
        target.commit.commit_id.clone(),
        target.snapshot.snapshot_id.clone(),
    )];
    let source = build_plan(&repository_id, "Source", &parent, None);
    let merge_parents = [
        (
            target.commit.commit_id.clone(),
            target.snapshot.snapshot_id.clone(),
        ),
        (
            source.commit.commit_id.clone(),
            source.snapshot.snapshot_id.clone(),
        ),
    ];
    let merge = build_plan(
        &repository_id,
        "Merged",
        &merge_parents,
        Some((target.commit.commit_id.clone(), &target, &source)),
    );
    let mut single_parent_merge = merge.commit.clone();
    single_parent_merge.ordered_parents.truncate(1);
    assert!(single_parent_merge.validate().is_err());

    let original = merge.reconciliation.as_ref().unwrap();
    assert!(SemReconciliationV2::new(
        repository_id.clone(),
        original.base_commit_id.clone(),
        original.left.clone(),
        original.right.clone(),
        original.merged.clone(),
        Vec::new(),
        original.preview_digest.clone(),
        ReconciliationOutcomeV2::Materialized,
    )
    .is_err());

    let mut changed_left = original.left.clone();
    changed_left.candidate.payloads[0].payload_fingerprint =
        ObjectBlobIdV2::from_canonical_fields(&[b"same-ref-different-payload"]);
    changed_left.candidate.payloads.sort();
    assert!(SemReconciliationV2::new(
        repository_id,
        original.base_commit_id.clone(),
        changed_left,
        original.right.clone(),
        original.merged.clone(),
        original.decisions.clone(),
        original.preview_digest.clone(),
        ReconciliationOutcomeV2::Materialized,
    )
    .is_err());
}

#[test]
fn ordered_merge_parents_are_identity_material_and_subject_heads_require_independent_pins() {
    let directory = tempdir().unwrap();
    let fixture = criss_cross_store(directory.path());
    assert_ne!(
        fixture.d.snapshot.snapshot_id,
        fixture.e.snapshot.snapshot_id
    );
    assert_ne!(fixture.d.commit.commit_id, fixture.e.commit.commit_id);

    let status = fixture.store.status().unwrap();
    assert!(fixture
        .store
        .lineage_proof(
            &fixture.d.commit.commit_id,
            &fixture.a.commit.commit_id,
            &LineagePin::AcceptedState(status.state_digest.clone()),
        )
        .is_err());
    let proof = fixture
        .store
        .lineage_proof(
            &fixture.d.commit.commit_id,
            &fixture.a.commit.commit_id,
            &LineagePin::SubjectCommit(fixture.d.commit.commit_id.clone()),
        )
        .unwrap();
    fixture
        .store
        .verify_lineage_proof(
            &proof,
            &LineagePin::SubjectCommit(fixture.d.commit.commit_id.clone()),
        )
        .unwrap();
    assert!(fixture
        .store
        .verify_lineage_proof(
            &proof,
            &LineagePin::SubjectCommit(fixture.e.commit.commit_id.clone()),
        )
        .is_err());
}

#[test]
fn tags_are_immutable_and_ref_updates_advance_the_single_state_transaction() {
    let directory = tempdir().unwrap();
    let fixture = criss_cross_store(directory.path());
    let before = fixture.store.status().unwrap();
    let tagged = fixture
        .store
        .create_tag(
            before.state.generation,
            "tags/release",
            &fixture.d.commit.commit_id,
        )
        .unwrap();
    assert_eq!(tagged.state.generation, before.state.generation + 1);
    assert_eq!(fixture.store.tags().unwrap().len(), 1);
    assert!(matches!(
        fixture.store.create_tag(
            tagged.state.generation,
            "tags/release",
            &fixture.e.commit.commit_id
        ),
        Err(AxiStoreError::ImmutableTag(_)) | Err(AxiStoreError::StaleRef { .. })
    ));
}

#[test]
fn review_candidate_manifest_cannot_masquerade_as_accepted_axpd_anchor() {
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "candidate-materialization");
    let accepted = build_plan(&repository_id, "Accepted", &[], None);
    let status = store.promote(0, &accepted).unwrap();
    let parents = [(
        accepted.commit.commit_id.clone(),
        accepted.snapshot.snapshot_id.clone(),
    )];
    let candidate = build_plan(&repository_id, "Candidate", &parents, None);
    store
        .publish_candidate(
            status.state.generation,
            "heads/review/candidate",
            None,
            &candidate,
        )
        .unwrap();

    let error = store
        .publish_axpd(
            AxpdBuildSpec {
                anchors: AxpdAnchors {
                    repository_id,
                    accepted_snapshot_id: candidate.snapshot.snapshot_id.clone(),
                    accepted_tree_id: candidate.tree.tree_id.clone(),
                    ordered_module_closure: candidate.manifest.ordered_module_closure.clone(),
                    kernel_ir_digest: candidate.manifest.kernel_ir_digest.clone(),
                    canonical_fact_log_digest: candidate.manifest.canonical_fact_log_digest.clone(),
                },
                image: AxpdLogicalImage::default(),
                configuration: AxpdConfiguration::default(),
                materializer_version: AXPD_MATERIALIZER_VERSION.to_string(),
            },
            &AxpdLimits::default(),
        )
        .expect_err("review-only manifest must not authenticate an accepted materialization");
    assert!(matches!(
        error,
        AxpdError::AnchorMismatch {
            field: "accepted_build_manifest",
            ..
        }
    ));
}

fn mutate_json_object(root: &Path, id: &str, mutate: impl FnOnce(&mut Value)) {
    let path = object_path(root, id);
    let original = fs::read(&path).unwrap();
    let mut value: Value = serde_json::from_slice(&original).unwrap();
    mutate(&mut value);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
}

type TamperCase = (
    &'static str,
    Box<dyn Fn(&Path, &RepositoryIdV2, &PromotionPlan)>,
);

#[test]
fn tampering_repository_tree_snapshot_commit_and_all_commit_payload_families_rejects_on_restart() {
    let mut cases: Vec<TamperCase> = vec![
        (
            "repository",
            Box::new(|root, repository, _| {
                mutate_json_object(root, repository.as_str(), |value| {
                    value["name"] = Value::String("tampered".into())
                });
            }),
        ),
        (
            "tree",
            Box::new(|root, _, plan| {
                mutate_json_object(root, plan.tree.tree_id.as_str(), |value| {
                    value["modules"][0]["module_name"] = Value::String("Tampered".into())
                })
            }),
        ),
        (
            "snapshot-parent",
            Box::new(|root, _, plan| {
                mutate_json_object(root, plan.snapshot.snapshot_id.as_str(), |value| {
                    value["tree_id"] = Value::String(plan.snapshot.snapshot_id.to_string())
                })
            }),
        ),
    ];
    for field in [
        "repository_id",
        "ordered_parents",
        "delta",
        "gates",
        "attachments",
        "lifecycle_events",
        "accepted_tree_id",
        "build_manifest_digest",
    ] {
        let field = field.to_string();
        cases.push((
            Box::leak(field.clone().into_boxed_str()),
            Box::new(move |root, _, plan| {
                mutate_json_object(root, plan.commit.commit_id.as_str(), |value| {
                    value[&field] = Value::Null;
                });
            }),
        ));
    }

    for (case, (label, tamper)) in cases.into_iter().enumerate() {
        let directory = tempdir().unwrap();
        let (store, repository_id) = initialized(directory.path(), &format!("tamper-{case}"));
        let plan = build_plan(&repository_id, &format!("Tamper{case}"), &[], None);
        store.promote(0, &plan).unwrap();
        drop(store);
        tamper(directory.path(), &repository_id, &plan);
        assert!(
            AxiStore::open(directory.path()).is_err(),
            "{label} tamper was accepted"
        );
    }
}

#[test]
fn tampering_reconciliation_audit_state_missing_parent_and_cycle_rejects() {
    // Reconciliation object bytes.
    let directory = tempdir().unwrap();
    let fixture = criss_cross_store(directory.path());
    let reconciliation = fixture.d.reconciliation.as_ref().unwrap();
    mutate_json_object(
        directory.path(),
        reconciliation.reconciliation_id.as_str(),
        |value| value["base"] = Value::String(fixture.c.commit.commit_id.to_string()),
    );
    drop(fixture);
    assert!(AxiStore::open(directory.path()).is_err());

    // Audit field.
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "audit-tamper");
    let plan = build_plan(&repository_id, "AuditTamper", &[], None);
    store.promote(0, &plan).unwrap();
    drop(store);
    Connection::open(directory.path().join("catalog.sqlite"))
        .unwrap()
        .execute(
            "UPDATE audit_events SET action='tampered' WHERE sequence=1",
            [],
        )
        .unwrap();
    assert!(AxiStore::open(directory.path()).is_err());

    // Singleton state field.
    let directory = tempdir().unwrap();
    let (store, repository_id) = initialized(directory.path(), "state-tamper");
    let plan = build_plan(&repository_id, "StateTamper", &[], None);
    store.promote(0, &plan).unwrap();
    drop(store);
    Connection::open(directory.path().join("catalog.sqlite"))
        .unwrap()
        .execute(
            "UPDATE store_state SET ref_map_digest=?1",
            [ObjectBlobIdV2::from_canonical_fields(&[b"tampered"]).as_str()],
        )
        .unwrap();
    assert!(AxiStore::open(directory.path()).is_err());

    // Missing parent and cycle are injected with foreign keys disabled. Exact
    // immutable commit objects remain unchanged, so restart must fail closed.
    for cycle in [false, true] {
        let directory = tempdir().unwrap();
        let fixture = criss_cross_store(directory.path());
        let connection = Connection::open(directory.path().join("catalog.sqlite")).unwrap();
        connection
            .pragma_update(None, "foreign_keys", "OFF")
            .unwrap();
        if cycle {
            connection
                .execute(
                    "INSERT INTO semantic_commit_parents(commit_id, position, parent_commit_id) VALUES(?1, 0, ?2)",
                    [fixture.a.commit.commit_id.as_str(), fixture.b.commit.commit_id.as_str()],
                )
                .unwrap();
        } else {
            connection
                .execute(
                    "UPDATE semantic_commit_parents SET parent_commit_id=?1 WHERE commit_id=?2 AND position=0",
                    [
                        CommitIdV2::from_canonical_fields(&[b"missing-parent"]).as_str(),
                        fixture.b.commit.commit_id.as_str(),
                    ],
                )
                .unwrap();
        }
        drop(connection);
        drop(fixture);
        assert!(AxiStore::open(directory.path()).is_err());
    }
}
