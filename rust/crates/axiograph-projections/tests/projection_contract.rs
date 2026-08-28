use axiograph_kernel::{
    CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest, KernelRefV2,
    ObjectBlobIdV2, RefinementPredicateIr, RepositoryIdV2, SnapshotIdV2, TypeExprIr,
};
use axiograph_projections::{
    backend_capabilities_v1, check_readback_v1, manifest_readback_fixture_v1, project_snapshot_v1,
    project_type_expr_v1, CapabilityDispositionV1, ExternalEvidenceAuthorityV1,
    ProjectedRefinementPredicateV1, ProjectedTypeExprV1, ProjectionAuthorityV1,
    ProjectionBackendV1, ProjectionCapabilityV1, ProjectionError, ProjectionMutationAuthorityV1,
    ProjectionRecordPayloadV1, ReadbackRecordObservationV1, ReadbackTransportStatusV1,
    SemanticLossClassV1,
};

fn compile_fixture() -> axiograph_kernel::CompiledKernelSnapshot {
    let text = r#"module ProjectionDemo

schema Supply:
  object Part
  object Plant
  object World
  relation Transfer(part: Part @data, source: Plant @data, target: Plant @data, world: World @world)
  aspect home: Part -> Plant

theory SupplyRules on Supply:
  constraint key Transfer(part, source, target, world)
  equation home_identity:
    home = home

instance Current of Supply:
  Part = {P1}
  Plant = {A, B}
  World = {Observed}
  Transfer = {
    shipment: (part=P1, source=A, target=B, world=Observed)
  }
  home = {
    (source=P1, target=A)
  }
"#;
    let source = CanonicalModuleSource::parse(text.as_bytes().to_vec()).expect("canonical source");
    CanonicalCompiler::compile(KernelCompilationRequest {
        repository_id: RepositoryIdV2::from_descriptor_bytes(b"projection-test-repository"),
        accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[b"accepted-projection-test"]),
        root_module: "ProjectionDemo".to_string(),
        modules: vec![source],
    })
    .expect("finite compiled kernel snapshot")
}

#[test]
fn every_backend_emits_a_typed_anchored_projection_and_honest_losses() {
    let snapshot = compile_fixture();
    for backend in ProjectionBackendV1::ALL {
        let manifest = project_snapshot_v1(&snapshot, backend).expect("projection");
        assert_eq!(manifest.authority, ProjectionAuthorityV1::DerivedOnly);
        assert_eq!(
            manifest.mutation_authority,
            ProjectionMutationAuthorityV1::AxiographSemanticVcsOnly
        );
        assert_eq!(manifest.anchor.kernel_ir_digest, *snapshot.ir().ir_digest());
        assert_eq!(
            manifest.coverage.kernel_refs_total,
            manifest.coverage.kernel_refs_covered
        );
        assert!(manifest.coverage.uncovered_kernel_refs.is_empty());
        assert!(manifest.coverage.relation_objects > 0);
        assert!(manifest.coverage.role_projections >= 4);
        assert!(manifest.coverage.path_equations > 0);
        assert!(manifest.coverage.relation_facts > 0);
        assert!(!manifest.records.is_empty());
        assert!(manifest.records.iter().any(|record| matches!(
            (&record.source_ref, &record.payload),
            (
                KernelRefV2::Generator { .. },
                ProjectionRecordPayloadV1::SchemaGenerator { .. }
            )
        )));
        assert!(!manifest.artifact.body.is_empty());
        assert!(manifest.artifact.read_only);
        assert!(!manifest.semantic_loss.lossless_semantic_projection_claim);
        assert!(manifest.semantic_loss.losses.iter().any(|loss| {
            loss.capability == ProjectionCapabilityV1::PathEquations
                && loss.class == SemanticLossClassV1::NotEnforcedByBackend
        }));
        assert!(manifest
            .semantic_loss
            .losses
            .iter()
            .any(|loss| loss.capability == ProjectionCapabilityV1::FiniteInstances));
        assert!(manifest.records.iter().any(|record| matches!(
            record.payload,
            ProjectionRecordPayloadV1::RelationObject { arity: 4, .. }
        )));
        match backend {
            ProjectionBackendV1::PathDb
            | ProjectionBackendV1::TerminusDb
            | ProjectionBackendV1::PropertyGraph => {
                serde_json::from_str::<serde_json::Value>(&manifest.artifact.body)
                    .expect("JSON projection artifact");
            }
            ProjectionBackendV1::TypeDb => {
                assert!(manifest.artifact.body.contains(" relates "));
                assert!(manifest.artifact.body.contains(" plays "));
            }
            ProjectionBackendV1::RdfOwl => {
                assert!(manifest.artifact.body.contains("axp:RelationFact"));
                assert!(manifest.artifact.body.contains("axp:payloadFingerprint"));
            }
        }
    }
}

#[test]
fn capability_profiles_never_claim_native_higher_paths() {
    for backend in ProjectionBackendV1::ALL {
        let declaration = backend_capabilities_v1(backend);
        let higher = declaration
            .capabilities
            .iter()
            .find(|entry| entry.capability == ProjectionCapabilityV1::HigherPaths)
            .expect("higher-path capability is explicit");
        assert_eq!(higher.disposition, CapabilityDispositionV1::Unsupported);
        assert!(higher.caveat.contains("univalence"));
        let readback = declaration
            .capabilities
            .iter()
            .find(|entry| entry.capability == ProjectionCapabilityV1::NativeReadback)
            .expect("readback capability is explicit");
        assert_eq!(readback.disposition, CapabilityDispositionV1::Encoded);
    }
    let typedb = backend_capabilities_v1(ProjectionBackendV1::TypeDb);
    let subtype = typedb
        .capabilities
        .iter()
        .find(|entry| entry.capability == ProjectionCapabilityV1::SubtypeInclusions)
        .expect("subtype capability is explicit");
    assert_eq!(subtype.disposition, CapabilityDispositionV1::Encoded);
    let instances = typedb
        .capabilities
        .iter()
        .find(|entry| entry.capability == ProjectionCapabilityV1::FiniteInstances)
        .expect("finite-instance capability is explicit");
    assert_eq!(instances.disposition, CapabilityDispositionV1::Sidecar);
}

#[test]
fn dependent_and_refinement_type_structure_is_not_flattened() {
    let snapshot = compile_fixture();
    let object_id = snapshot.ir().schemas()[0].objects[0].object_type_id.clone();
    let refined = TypeExprIr::Refined {
        base: Box::new(TypeExprIr::Indexed {
            base: Box::new(TypeExprIr::Object {
                object_type_id: object_id,
            }),
            over_roles: vec![snapshot.ir().schemas()[0].relations[0].roles[0]
                .role_id
                .clone()],
        }),
        predicates: vec![
            RefinementPredicateIr::Cardinality { min: 1, max: 1 },
            RefinementPredicateIr::Predicate {
                name: "eligible".to_string(),
                args: vec!["current_world".to_string()],
            },
        ],
    };
    let projected = project_type_expr_v1(&refined);
    let ProjectedTypeExprV1::Refined { base, predicates } = projected else {
        panic!("refinement wrapper was flattened")
    };
    assert!(matches!(*base, ProjectedTypeExprV1::Indexed { .. }));
    assert!(matches!(
        predicates.as_slice(),
        [
            ProjectedRefinementPredicateV1::Cardinality { min: 1, max: 1 },
            ProjectedRefinementPredicateV1::Predicate { .. }
        ]
    ));
}

#[test]
fn exact_readback_is_transport_evidence_not_semantic_authority() {
    let manifest =
        project_snapshot_v1(&compile_fixture(), ProjectionBackendV1::TypeDb).expect("projection");
    let inventory = manifest_readback_fixture_v1(&manifest);
    let report = check_readback_v1(&manifest, &inventory).expect("readback report");
    assert_eq!(
        report.transport_status,
        ReadbackTransportStatusV1::ExactFiniteRecordMatch
    );
    assert_eq!(report.matched_records, manifest.records.len());
    assert_eq!(
        report.evidence.authority,
        ExternalEvidenceAuthorityV1::EvidenceOnly
    );
    assert!(!report.evidence.accepted_state_change);
    assert!(report.evidence.requires_typed_proposal_review_and_promotion);
    assert!(!report.semantic_equivalence_claim);
    assert!(!report.completeness_claim);
    assert!(!report.ontology_closure_claim);
}

#[test]
fn readback_reports_missing_drifted_and_unexpected_records() {
    let manifest =
        project_snapshot_v1(&compile_fixture(), ProjectionBackendV1::RdfOwl).expect("projection");
    let mut inventory = manifest_readback_fixture_v1(&manifest);
    let missing = inventory.records.remove(0);
    inventory.records[0].payload_fingerprint =
        ObjectBlobIdV2::from_canonical_fields(&[b"tampered-payload"]);
    inventory.records[1].native_key = "renamed_backend_record".to_string();
    inventory.records.push(ReadbackRecordObservationV1 {
        record_id: ObjectBlobIdV2::from_canonical_fields(&[b"unexpected-record"]),
        payload_fingerprint: ObjectBlobIdV2::from_canonical_fields(&[b"unexpected-payload"]),
        native_key: "backend_native_extra".to_string(),
    });

    let report = check_readback_v1(&manifest, &inventory).expect("drift report");
    assert_eq!(
        report.transport_status,
        ReadbackTransportStatusV1::DriftDetected
    );
    assert_eq!(report.missing_record_ids, vec![missing.record_id]);
    assert_eq!(report.drifted_records.len(), 2);
    assert_eq!(report.unexpected_records.len(), 1);
    assert!(!report.evidence.accepted_state_change);
}

#[test]
fn readback_rejects_wrong_anchor_backend_version_provenance_and_duplicate_ids() {
    let manifest =
        project_snapshot_v1(&compile_fixture(), ProjectionBackendV1::PathDb).expect("projection");

    let mut wrong_anchor = manifest_readback_fixture_v1(&manifest);
    wrong_anchor.projection_id = ObjectBlobIdV2::from_canonical_fields(&[b"wrong-projection"]);
    assert!(matches!(
        check_readback_v1(&manifest, &wrong_anchor),
        Err(ProjectionError::ProjectionIdMismatch)
    ));

    let mut wrong_backend = manifest_readback_fixture_v1(&manifest);
    wrong_backend.backend = ProjectionBackendV1::TypeDb;
    assert!(matches!(
        check_readback_v1(&manifest, &wrong_backend),
        Err(ProjectionError::BackendMismatch { .. })
    ));

    let mut wrong_version = manifest_readback_fixture_v1(&manifest);
    wrong_version.version = "removed_readback_v0".to_string();
    assert!(matches!(
        check_readback_v1(&manifest, &wrong_version),
        Err(ProjectionError::UnsupportedReadbackVersion(_))
    ));

    let mut missing_provenance = manifest_readback_fixture_v1(&manifest);
    missing_provenance.provenance.adapter_id.clear();
    assert!(matches!(
        check_readback_v1(&manifest, &missing_provenance),
        Err(ProjectionError::InvalidReadbackProvenance(_))
    ));

    let mut wrong_manifest_version = manifest.clone();
    wrong_manifest_version.version = "removed_projection_v0".to_string();
    let inventory = manifest_readback_fixture_v1(&manifest);
    assert!(matches!(
        check_readback_v1(&wrong_manifest_version, &inventory),
        Err(ProjectionError::UnsupportedManifestVersion(_))
    ));

    let mut tampered_manifest = manifest.clone();
    tampered_manifest.artifact.body.push_str("\n# backend edit");
    assert!(matches!(
        check_readback_v1(&tampered_manifest, &inventory),
        Err(ProjectionError::ManifestIntegrityMismatch)
    ));

    let mut duplicate = manifest_readback_fixture_v1(&manifest);
    duplicate.records.push(duplicate.records[0].clone());
    assert!(matches!(
        check_readback_v1(&manifest, &duplicate),
        Err(ProjectionError::DuplicateReadbackRecord(_))
    ));
}

#[test]
fn manifest_json_rejects_unknown_authority_fields() {
    let manifest = project_snapshot_v1(&compile_fixture(), ProjectionBackendV1::PropertyGraph)
        .expect("projection");
    let mut value = serde_json::to_value(&manifest).expect("serialize manifest");
    value.as_object_mut().expect("manifest object").insert(
        "backend_is_authority".to_string(),
        serde_json::Value::Bool(true),
    );
    assert!(serde_json::from_value::<axiograph_projections::ProjectionManifestV1>(value).is_err());
}
