use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::kernel_ir::{
    RoleKind, RuntimeIrRef, RuntimeSchemaIndex, TheoryIr, TheoryObligationRefIr, TheoryPathSideIr,
    TheorySubjectRefIr, TheoryTransportStatusIr, TheoryVariableKindIr,
    THEORY_ADDRESS_INDEX_VERSION_V1,
};
use axiograph_pathdb::runtime_theory_checker::{
    RuntimeTheoryAssumptionEffectV1, RuntimeTheoryCheckStatusV1, RuntimeTheoryClosureStepKindV1,
    RuntimeTheoryClosureTierV1,
};
use axiograph_pathdb::{
    check_runtime_theory_v1, check_runtime_theory_with_options_v1, default_evidence_policy_v1,
    default_world_assumption_v1, derive_runtime_module_index, ArrowMappingV1,
    MigrationFunctorKindV1, ObjectMappingV1, SchemaMorphismV1,
};

fn compiled_fixture(axi: &str) -> (RuntimeSchemaIndex, TheoryIr) {
    let module = parse_axi_v1(axi).expect("fixture parses");
    let kernel = derive_runtime_module_index(&module, axi).expect("fixture compiles to kernel IR");
    (kernel.schemas[0].clone(), kernel.theories[0].clone())
}

fn compile_error(axi: &str) -> String {
    parse_axi_v1(axi)
        .map_err(|err| err.to_string())
        .and_then(|module| derive_runtime_module_index(&module, axi).map(|_| ()))
        .expect_err("fixture should be rejected")
}

#[test]
fn path_endpoint_diagnostic_names_mismatched_endpoint_types() {
    let err = compile_error(
        r#"
module Family

schema Family:
  object Person
  object Org
  relation parent(child: Person, parent: Person)
  relation member(person: Person, org: Org)

theory FamilyTheory on Family:
  equation bad_endpoint:
    step(x,parent,y) =
    step(x,member,o)
"#,
    );

    assert!(err.contains("mismatched path endpoints"));
    assert!(err.contains("lhs=Path(x,y)"));
    assert!(err.contains("rhs=Path(x,o)"));
}

#[test]
fn undeclared_rewrite_variable_diagnostic_names_variable() {
    let err = compile_error(
        r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  rewrite bad_var:
    vars: a: Person
    lhs: step(a, parent, b)
    rhs: step(a, parent, b)
"#,
    );

    assert!(err.contains("rewrite rule `bad_var`"));
    assert!(err.contains("unbound object variable `b`"));
}

#[test]
fn context_world_and_time_roles_share_one_runtime_axis_semantics() {
    let (schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  object Context
  object World
  object Time
  relation ScopedParent(child: Person, parent: Person, ctx: Context @context, world: World @world, time: Time @temporal)

theory FamilyTheory on Family:
  rewrite keep_scope:
    vars: a: Person, b: Person
    lhs: step(a, ScopedParent, b)
    rhs: step(a, ScopedParent, b)
"#,
    );

    let obligation = theory.obligation_refs()[0].clone();
    let subjects = theory.subject_refs_for_obligation(&obligation);
    assert!(subjects.iter().any(|subject| matches!(
        subject,
        TheorySubjectRefIr::Role { role_name, .. } if role_name == "ctx"
    )));
    assert!(subjects.iter().any(|subject| matches!(
        subject,
        TheorySubjectRefIr::Role { role_name, .. } if role_name == "world"
    )));
    assert!(subjects.iter().any(|subject| matches!(
        subject,
        TheorySubjectRefIr::Role { role_name, .. } if role_name == "time"
    )));

    let report = check_runtime_theory_v1(&schema, &theory);
    let judgment = &report.judgments[0];

    assert_eq!(judgment.status, RuntimeTheoryCheckStatusV1::Checked);
    assert!(judgment.admissibility_diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "rewrite_axis_roles_preserved" && diagnostic.admissible
    }));
    let endpoint = judgment
        .typed_endpoint
        .as_ref()
        .expect("rewrite endpoint payload");
    assert!(endpoint
        .axis_roles
        .iter()
        .any(|role| role.role_kind == RoleKind::Context && role.role_name == "ctx"));
    assert!(endpoint
        .axis_roles
        .iter()
        .any(|role| role.role_kind == RoleKind::World && role.role_name == "world"));
    assert!(endpoint
        .axis_roles
        .iter()
        .any(|role| role.role_kind == RoleKind::Temporal && role.role_name == "time"));
    assert!(report.assumption_diagnostics.iter().any(|diagnostic| {
        diagnostic.assumption_id == "context_scope_named"
            && diagnostic.effect == RuntimeTheoryAssumptionEffectV1::NarrowsClaim
    }));
    assert!(!report
        .admissibility_scan
        .residual_obligations
        .iter()
        .any(|id| id == "closure_engine_not_implemented"));
}

#[test]
fn dropping_a_world_axis_blocks_rewrite_admissibility() {
    let (schema, theory) = compiled_fixture(
        r#"
module WorldAxis

schema S:
  object Person
  object World
  relation Scoped(child: Person, parent: Person, world: World @world)
  relation Bare(child: Person, parent: Person)

theory T on S:
  rewrite drop_world:
    vars: a: Person, b: Person
    lhs: step(a, Scoped, b)
    rhs: step(a, Bare, b)
"#,
    );

    let report = check_runtime_theory_v1(&schema, &theory);
    assert_eq!(report.blocked_obligations, 1);
    assert_eq!(
        report.judgments[0].status,
        RuntimeTheoryCheckStatusV1::Blocked
    );
    assert!(report.judgments[0]
        .admissibility_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "rewrite_axis_roles_dropped"));
}

#[test]
fn theory_address_index_exposes_deterministic_path_context_and_transport_refs() {
    let (_schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  object Context
  object Time
  relation ScopedParent(child: Person, parent: Person, ctx: Context @context, time: Time @temporal)

theory FamilyTheory on Family:
  rewrite keep_scope:
    vars: a: Person, b: Person
    lhs: step(a, ScopedParent, b)
    rhs: step(a, ScopedParent, b)
"#,
    );

    let obligation = theory
        .obligation_refs()
        .into_iter()
        .find(|obligation| matches!(obligation, TheoryObligationRefIr::RewriteRule { .. }))
        .expect("rewrite obligation");
    let stable_id = obligation.stable_id();

    let step_refs = theory.path_step_refs_for_obligation(&obligation);
    assert_eq!(step_refs.len(), 2);
    assert_eq!(
        step_refs
            .iter()
            .map(|step| step.step_id.clone())
            .collect::<Vec<_>>(),
        vec![
            format!("path_step:{stable_id}:lhs:0"),
            format!("path_step:{stable_id}:rhs:0"),
        ]
    );
    assert_eq!(step_refs[0].side, TheoryPathSideIr::Lhs);
    assert_eq!(step_refs[1].side, TheoryPathSideIr::Rhs);
    assert_eq!(step_refs[0].relation_name, "ScopedParent");
    assert_eq!(
        step_refs[0].relation_id.as_ref().map(|id| id.as_str()),
        Some("relation:Family:ScopedParent")
    );

    let context_refs = theory.context_refs_for_obligation(&obligation);
    assert_eq!(
        context_refs
            .iter()
            .map(|context| context.context_axis_id.clone())
            .collect::<Vec<_>>(),
        vec![
            format!("context_axis:{stable_id}:role:Family:ScopedParent:ctx"),
            format!("context_axis:{stable_id}:role:Family:ScopedParent:time"),
        ]
    );

    let transport_refs = theory.transport_item_refs_for_obligation(&obligation);
    assert_eq!(transport_refs.len(), 1);
    assert_eq!(
        transport_refs[0].transport_item_id,
        format!("transport_item:{stable_id}")
    );

    let dependency = theory.obligation_dependencies(&obligation);
    assert_eq!(dependency.path_step_refs, step_refs);
    assert_eq!(dependency.context_refs, context_refs);
    assert_eq!(dependency.transport_item_refs, transport_refs);
    assert!(dependency.variable_refs.iter().any(|variable| {
        variable.variable_name == "a"
            && variable.variable_kind == TheoryVariableKindIr::Object
            && variable.object_type.as_deref() == Some("Person")
    }));
    assert_eq!(
        dependency
            .endpoint_refs
            .iter()
            .map(|endpoint| endpoint.endpoint_id.clone())
            .collect::<Vec<_>>(),
        vec![
            format!("theory_endpoint:{stable_id}:lhs"),
            format!("theory_endpoint:{stable_id}:rhs"),
        ]
    );

    let index = theory.address_index();
    assert_eq!(index.version, THEORY_ADDRESS_INDEX_VERSION_V1);
    assert_eq!(index.dependencies.len(), 1);
    assert_eq!(index.path_step_refs, dependency.path_step_refs);
    assert_eq!(index.context_refs, dependency.context_refs);
    assert_eq!(index.transport_item_refs, dependency.transport_item_refs);
}

#[test]
fn runtime_admissibility_checks_carry_dependency_refs_into_report() {
    let (schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  object Context
  object Time
  relation ScopedParent(child: Person, parent: Person, ctx: Context @context, time: Time @temporal)

theory FamilyTheory on Family:
  rewrite keep_scope:
    vars: a: Person, b: Person
    lhs: step(a, ScopedParent, b)
    rhs: step(a, ScopedParent, b)
"#,
    );

    let report = check_runtime_theory_v1(&schema, &theory);
    let check = report
        .admissibility_checks
        .iter()
        .find(|check| check.code == "rewrite_axis_roles_preserved")
        .expect("axis preservation check");

    assert!(check.admissible);
    assert_eq!(check.path_step_refs.len(), 2);
    assert_eq!(check.endpoint_refs.len(), 2);
    assert_eq!(check.transport_item_refs.len(), 1);
    assert!(check
        .context_refs
        .iter()
        .any(|context| context.role_kind == RoleKind::Context && context.role_name == "ctx"));
    assert!(check
        .context_refs
        .iter()
        .any(|context| context.role_kind == RoleKind::Temporal && context.role_name == "time"));
    assert!(report.judgments[0]
        .admissibility_checks
        .iter()
        .any(|judgment_check| judgment_check.check_id == check.check_id));
    assert!(report.admissibility_scan.steps[0]
        .admissibility_checks
        .iter()
        .any(|step_check| step_check.check_id == check.check_id));
    assert!(report
        .kernel_refs
        .iter()
        .any(|reference| matches!(reference, RuntimeIrRef::Theory { .. })));
    assert!(report.judgments[0]
        .kernel_refs
        .iter()
        .any(|reference| matches!(reference, RuntimeIrRef::TheoryObligation { .. })));
    assert!(report.admissibility_scan.steps[0]
        .kernel_refs
        .iter()
        .any(|reference| matches!(reference, RuntimeIrRef::TheorySubject { .. })));
}

#[test]
fn opaque_equation_keeps_addressable_handle_as_scan_residual() {
    let (schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  relation parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  equation business_axiom:
    parent is social =
    meaningful business rule
"#,
    );

    let report = check_runtime_theory_v1(&schema, &theory);
    let judgment = &report.judgments[0];

    assert!(matches!(
        judgment.obligation_ref,
        TheoryObligationRefIr::OpaqueEquation { ref name, .. } if name == "business_axiom"
    ));
    assert!(judgment
        .obligation_ref
        .matches_artifact_id("business_axiom"));
    assert_eq!(judgment.status, RuntimeTheoryCheckStatusV1::ReviewOnly);
    assert!(judgment
        .admissibility_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "opaque_equation_addressable_review_only"));
    assert!(report
        .admissibility_scan
        .residual_obligations
        .contains(&judgment.obligation_ref.stable_id()));
}

#[test]
fn transport_statuses_carry_subjects_into_report_steps() {
    let (schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  object Context
  object Time
  relation ScopedParent(child: Person, parent: Person, ctx: Context @context, time: Time @temporal)

theory FamilyTheory on Family:
  rewrite keep_scope:
    vars: a: Person, b: Person
    lhs: step(a, ScopedParent, b)
    rhs: step(a, ScopedParent, b)
"#,
    );

    let preserved_morphism = SchemaMorphismV1 {
        source_schema: schema.schema_id.to_string(),
        target_schema: schema.schema_id.to_string(),
        objects: vec![
            ObjectMappingV1 {
                source_object: "Person".to_string(),
                target_object: "Person".to_string(),
            },
            ObjectMappingV1 {
                source_object: "Context".to_string(),
                target_object: "Context".to_string(),
            },
            ObjectMappingV1 {
                source_object: "Time".to_string(),
                target_object: "Time".to_string(),
            },
        ],
        arrows: vec![ArrowMappingV1 {
            source_arrow: "ScopedParent".to_string(),
            target_path: vec!["ScopedParent".to_string()],
        }],
    };
    let preserved_plan =
        theory.theory_transport_plan(&schema, &preserved_morphism, MigrationFunctorKindV1::DeltaF);
    assert_eq!(
        preserved_plan.items[0].status,
        TheoryTransportStatusIr::Preserved
    );

    let blocked_morphism = SchemaMorphismV1 {
        source_schema: schema.schema_id.to_string(),
        target_schema: "FamilyV2".to_string(),
        objects: vec![ObjectMappingV1 {
            source_object: "Person".to_string(),
            target_object: "Human".to_string(),
        }],
        arrows: vec![ArrowMappingV1 {
            source_arrow: "ScopedParent".to_string(),
            target_path: vec!["ScopedParent".to_string()],
        }],
    };
    let blocked_plan =
        theory.theory_transport_plan(&schema, &blocked_morphism, MigrationFunctorKindV1::DeltaF);
    let item = &blocked_plan.items[0];

    assert_eq!(item.status, TheoryTransportStatusIr::MissingObjectImage);
    assert_eq!(
        item.missing_object_images,
        vec!["Context".to_string(), "Time".to_string()]
    );
    assert!(item.subject_refs.iter().any(|subject| matches!(
        subject,
        TheorySubjectRefIr::Role { role_name, .. } if role_name == "ctx"
    )));
    assert!(item.subject_refs.iter().any(|subject| matches!(
        subject,
        TheorySubjectRefIr::Role { role_name, .. } if role_name == "time"
    )));

    let report = check_runtime_theory_with_options_v1(
        &schema,
        &theory,
        RuntimeTheoryClosureTierV1::FiniteFragment,
        default_world_assumption_v1(),
        default_evidence_policy_v1(),
        Some(&blocked_plan),
    );

    assert_eq!(
        report.judgments[0].transport_status,
        Some(TheoryTransportStatusIr::MissingObjectImage)
    );
    assert_eq!(
        report.judgments[0].status,
        RuntimeTheoryCheckStatusV1::Blocked
    );
    assert_eq!(
        report.admissibility_scan.steps[0].transport_status,
        Some(TheoryTransportStatusIr::MissingObjectImage)
    );
}

#[test]
fn transitive_constraint_remains_review_only_without_runtime_enforcement() {
    let (schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  relation Parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  constraint transitive Parent on (child, parent)
"#,
    );

    let report = check_runtime_theory_v1(&schema, &theory);

    assert_eq!(report.review_only_obligations, 1);
    assert_eq!(
        report.judgments[0].status,
        RuntimeTheoryCheckStatusV1::ReviewOnly
    );
    assert!(report.judgments[0]
        .non_claims
        .iter()
        .any(|claim| claim.code == "constraint_review_only"));
    assert!(report.fragment.classifies_structured_constraints);
    assert!(report.fragment.checks_path_equation_endpoints);
    assert!(report.fragment.checks_rewrite_endpoints_and_axes);
    assert!(report.fragment.classifies_transports);
    assert!(!report
        .admissibility_scan
        .residual_obligations
        .iter()
        .any(|id| id == "closure_engine_not_implemented"));
}

#[test]
fn evidence_filter_cannot_erase_an_opaque_semantic_residual() {
    let (schema, theory) = compiled_fixture(
        r#"
module Family

schema Family:
  object Person
  relation Parent(child: Person, parent: Person)

theory FamilyTheory on Family:
  equation policy_statement:
    parent is socially meaningful =
    reviewed by domain owner
"#,
    );
    let obligation_id = theory.obligation_refs()[0].stable_id();
    let mut evidence = default_evidence_policy_v1();
    evidence.threshold_ppm = 900_000;
    evidence
        .obligation_weights_ppm
        .insert(obligation_id.clone(), 100_000);

    let report = check_runtime_theory_with_options_v1(
        &schema,
        &theory,
        RuntimeTheoryClosureTierV1::EvidenceWeighted,
        default_world_assumption_v1(),
        evidence,
        None,
    );

    assert_eq!(report.review_only_obligations, 1);
    assert!(report
        .admissibility_scan
        .residual_obligations
        .contains(&obligation_id));
    assert_eq!(
        report.admissibility_scan.steps.last().map(|step| step.kind),
        Some(RuntimeTheoryClosureStepKindV1::AdmissibilityScanComplete)
    );
}
