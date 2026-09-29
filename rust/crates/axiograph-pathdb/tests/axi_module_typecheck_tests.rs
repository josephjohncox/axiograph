use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::axi_module_typecheck::{
    review_axi_v1_module, validate_axi_v1_module, Module, ReviewStamp,
};
use axiograph_pathdb::kernel_ir::{derive_runtime_schema_index, derive_runtime_theory_index};
use axiograph_pathdb::{
    Reviewed, RuntimeTheoryObligationFragmentStatusV1, RuntimeTheoryObligationTrustClassV1,
    TheoryObligationRefIr, Validated, RUNTIME_THEORY_FRAGMENT_SUMMARY_VERSION_V1,
};

#[test]
fn typecheck_accepts_minimal_well_typed_module() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    let proof = typed.proof();

    assert_eq!(proof.module_name, "Demo");
    assert_eq!(proof.schema_count, 1);
    assert_eq!(proof.instance_count, 1);
    assert_eq!(proof.tuple_count, 1);
}

#[test]
fn typecheck_rejects_duplicate_schema_names() {
    let axi = r#"
module Demo

schema S:
  object X

schema S:
  object Y

instance I of S:
  X = {a}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err.to_string().contains("duplicate schema `S`"));
}

#[test]
fn typecheck_rejects_object_relation_name_collisions() {
    let axi = r#"
module Demo

schema S:
  object Shared
  relation Shared(value: Shared)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let error = validate_axi_v1_module(module).expect_err("colliding declarations must fail");
    assert!(error
        .to_string()
        .contains("declares both object and relation `Shared`"));
}

#[test]
fn typecheck_rejects_repeated_subtype_edges() {
    let axi = r#"
module Demo

schema S:
  object Child
  object Parent
  subtype Child < Parent
  subtype Child < Parent
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let error = validate_axi_v1_module(module).expect_err("repeated subtype must fail");
    assert!(error
        .to_string()
        .contains("repeats subtype `Child < Parent`"));
}

#[test]
fn typecheck_rejects_self_subtype_cycles() {
    let axi = r#"
module Demo

schema S:
  object Node
  subtype Node < Node
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let error = validate_axi_v1_module(module).expect_err("self subtype must fail");
    assert!(error
        .to_string()
        .contains("has a subtype cycle involving `Node` and `Node`"));
}

#[test]
fn typecheck_rejects_relation_fields_with_undeclared_object_types() {
    let axi = r#"
module Demo

schema S:
  object Known
  relation Broken(value: Missing)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err
        .to_string()
        .contains("field `value` references unknown value type `Missing`"));
}

#[test]
fn typecheck_rejects_unknown_or_non_earlier_dependent_role_indexes() {
    let axi = r#"
module Demo

schema S:
  object Entity
  relation Broken(first: Entity, dependent: indexed(Entity; future), future: Entity)
"#;

    let module = parse_axi_v1(axi).expect("dependent-role syntax parses");
    let error = validate_axi_v1_module(module).expect_err("future role is not earlier");
    assert!(error
        .to_string()
        .contains("role `dependent` indexes unknown or non-earlier role `future`"));
}

#[test]
fn typecheck_rejects_unsupported_refinement_predicates() {
    let axi = r#"
module Demo

schema S:
  object Entity
  relation Broken(value: refined(Entity; predicate(custom_check|argument)))
"#;

    let module = parse_axi_v1(axi).expect("refinement syntax parses");
    let error = validate_axi_v1_module(module).expect_err("unsupported predicate must fail");
    assert!(error
        .to_string()
        .contains("role `value` uses unsupported predicate `custom_check`"));
}

#[test]
fn typecheck_accepts_valid_dependent_roles_and_supported_refinement_predicates() {
    let axi = r#"
module Demo

schema S:
  object Entity
  relation Valid(
    first: Entity,
    dependent: indexed(Entity; first),
    refined_value: refined(Entity; predicate(non_empty); key(first))
  )
"#;

    let module = parse_axi_v1(axi).expect("dependent/refinement syntax parses");
    validate_axi_v1_module(module).expect("valid conservative role types typecheck");
}

#[test]
fn typecheck_accepts_relation_objects_as_declared_field_types() {
    let axi = r#"
module Demo

schema S:
  object Node
  relation Flow(from: Node, to: Node)
  relation FlowCompose(first: relation(Flow), second: relation(Flow))
"#;

    let module = parse_axi_v1(axi).expect("parse");
    validate_axi_v1_module(module).expect("relation objects are valid role targets");
}

#[test]
fn typecheck_rejects_duplicate_instances() {
    let axi = r#"
module Demo

schema S:
  object X

instance I of S:
  X = {a}

instance I of S:
  X = {b}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err
        .to_string()
        .contains("duplicate instance `I` on schema `S`"));
}

#[test]
fn typecheck_rejects_repeated_assignments() {
    let axi = r#"
module Demo

schema S:
  object X

instance I of S:
  X = {a}
  X = {b}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let error = validate_axi_v1_module(module).expect_err("repeated assignments are ambiguous");
    assert!(error.to_string().contains("repeats assignment `X`"));
}

#[test]
fn typecheck_rejects_unknown_schema_reference() {
    let axi = r#"
module Demo

schema S:
  object X

instance I of NoSuchSchema:
  X = {a}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err.to_string().contains("references unknown schema"));
}

#[test]
fn typecheck_rejects_mixed_identifiers_and_tuples_in_assignment() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice}
  Parent = {(child=Alice, parent=Alice), Bob}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err.to_string().contains("mixes identifiers and tuples"));
}

#[test]
fn typecheck_rejects_tuple_with_unknown_field() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(kid=Alice, parent=Bob)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn typecheck_rejects_tuple_with_missing_declared_field() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).unwrap_err();
    assert!(err.to_string().contains("missing field `parent`"));
}

#[test]
fn review_transition_preserves_module_and_proof() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let validated: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    let validated_name = validated.module().module_name.clone();
    let validated_proof = validated.proof().clone();

    let reviewed: Module<Reviewed> = review_axi_v1_module(
        validated,
        ReviewStamp {
            reviewer: Some("agent".to_string()),
            note: Some("ready".to_string()),
        },
    );

    assert_eq!(reviewed.module().module_name, validated_name);
    assert_eq!(reviewed.proof().tuple_count, validated_proof.tuple_count);
    assert_eq!(
        reviewed.review_stamp().and_then(|s| s.reviewer.as_deref()),
        Some("agent")
    );
}

#[test]
fn typecheck_allows_supertype_name_to_upgrade_to_required_subtype() {
    let axi = r#"
module Demo

schema S:
  object Animal
  object Cat
  subtype Cat < Animal
  relation Favorite(friend: Cat, observer: Animal)

instance I of S:
  Animal = {Mittens}
  Favorite = {(friend=Mittens, observer=Mittens)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    assert_eq!(typed.proof().tuple_count, 1);
}

#[test]
fn typecheck_rejects_sibling_subtype_name_collision_at_supertype_boundary() {
    let axi = r#"
module Demo

schema S:
  object Animal
  object Cat
  object Dog
  subtype Cat < Animal
  subtype Dog < Animal
  relation Seen(subject: Animal, observer: Animal)

instance I of S:
  Cat = {Paws}
  Dog = {Paws}
  Seen = {(subject=Paws, observer=Paws)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).expect_err("ambiguous sibling reuse");
    assert!(err.to_string().contains("ambiguous element `Paws`"));
    assert!(err.to_string().contains("Animal"));
}

#[test]
fn typecheck_allows_transitive_upgrade_to_leaf_subtype() {
    let axi = r#"
module Demo

schema S:
  object Animal
  object Mammal
  object Cat
  subtype Mammal < Animal
  subtype Cat < Mammal
  relation Tracks(cat: Cat, witness: Mammal)

instance I of S:
  Animal = {Whiskers}
  Tracks = {(cat=Whiskers, witness=Whiskers)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    assert_eq!(typed.proof().tuple_count, 1);
}

#[test]
fn typecheck_accepts_same_theory_name_on_distinct_schemas() {
    let axi = r#"
module Demo

schema S1:
  object Person
  relation Parent(from: Person, to: Person)

schema S2:
  object Device
  relation DependsOn(from: Device, to: Device)

theory Rules on S1:
  constraint functional Parent.from -> Parent.to

theory Rules on S2:
  constraint functional DependsOn.from -> DependsOn.to
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    assert_eq!(typed.proof().theory_count, 2);
}

#[test]
fn typecheck_rejects_constraint_with_duplicate_param_field() {
    let axi = r#"
module Demo

schema S:
  object Person
  object Context
  relation Parent(from: Person, to: Person, ctx: Context)

theory T on S:
  constraint at_most 1 Parent.from -> Parent.to param (ctx, ctx)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).expect_err("duplicate params should fail");
    assert!(err.to_string().contains("repeats field `ctx`"));
}

#[test]
fn typecheck_rejects_theory_with_unknown_schema_reference() {
    let axi = r#"
module Demo

schema S:
  object Person

theory T on Missing:
  constraint key Parent(child)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).expect_err("unknown theory schema");
    assert!(err
        .to_string()
        .contains("theory `T` references unknown schema `Missing`"));
}

#[test]
fn typecheck_rejects_theory_constraint_with_unknown_field() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint functional Parent.guardian -> Parent.parent
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).expect_err("unknown constraint field");
    assert!(err.to_string().contains("has no field `guardian`"));
}

#[test]
fn typecheck_rejects_rewrite_rule_with_unknown_relation() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  rewrite bad:
    vars: x: Person, y: Person
    lhs: step(x, NoSuchRel, y)
    rhs: step(x, Parent, y)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let err = validate_axi_v1_module(module).expect_err("unknown rewrite relation");
    assert!(err.to_string().contains("rewrite rule `bad` lhs ill-typed"));
    assert!(err.to_string().contains("unknown relation `NoSuchRel`"));
}

#[test]
fn typecheck_rejects_invalid_checked_equations_and_duplicate_names() {
    let unknown_relation = r#"
module Demo
schema S:
  object A
  relation Edge(from: A, to: A)
theory T on S:
  equation bad:
    step(x, Missing, y) = refl(x)
"#;
    let module = parse_axi_v1(unknown_relation).expect("equation syntax parses");
    let error = validate_axi_v1_module(module).expect_err("unknown equation relation must fail");
    assert!(error.to_string().contains("unknown relation `Missing`"));

    let bad_composition = r#"
module Demo
schema S:
  object A
  relation Edge(from: A, to: A)
theory T on S:
  equation bad:
    trans(step(x, Edge, y), step(z, Edge, w)) = step(x, Edge, w)
"#;
    let module = parse_axi_v1(bad_composition).expect("equation syntax parses");
    let error = validate_axi_v1_module(module).expect_err("non-composable equation must fail");
    assert!(error.to_string().contains("cannot compose paths"));

    let mismatched_endpoints = r#"
module Demo
schema S:
  object A
  relation Edge(from: A, to: A)
theory T on S:
  equation bad:
    step(x, Edge, y) = refl(x)
"#;
    let module = parse_axi_v1(mismatched_endpoints).expect("equation syntax parses");
    let error = validate_axi_v1_module(module).expect_err("unequal equation endpoints must fail");
    assert!(error.to_string().contains("mismatched path endpoints"));

    let duplicate_name = r#"
module Demo
schema S:
  object A
theory T on S:
  equation duplicate:
    opaque(x) = opaque(x)
  equation duplicate:
    other(x) = other(x)
"#;
    let module = parse_axi_v1(duplicate_name).expect("opaque equation syntax parses");
    let error = validate_axi_v1_module(module).expect_err("duplicate equation name must fail");
    assert!(error.to_string().contains("duplicate equation `duplicate`"));
}

#[test]
fn equation_relation_object_carriers_match_initial_and_runtime_typechecking() {
    let axi = r#"
module Demo
schema S
  object Node
  relation Edge(from: Node, to: Node)
  relation Pair(left: relation(Edge), right: relation(Edge))
theory T on S
  equation relation_carrier
    step(x, Pair, y) = step(x, Pair, y)
"#;
    let module = parse_axi_v1(axi).expect("parse relation-carrier equation");
    validate_axi_v1_module(module.clone()).expect("initial typecheck accepts relation values");
    let schema = derive_runtime_schema_index(&module.schemas[0]);
    derive_runtime_theory_index(&schema, &module.theories[0])
        .expect("runtime typecheck accepts the same relation values");
}

#[test]
fn typecheck_rejects_duplicate_generator_fields_in_all_orders() {
    for tuple in [
        "source=a, source=b, target=c",
        "source=a, target=b, target=c",
        "target=c, source=a, source=b",
    ] {
        let axi = format!(
            "module Demo\nschema S\n  object A\n  function F: A -> A\ninstance I of S\n  F = {{({tuple})}}\n"
        );
        let module = parse_axi_v1(&axi).expect("duplicate tuple fields remain visible in the AST");
        let error = validate_axi_v1_module(module)
            .expect_err("duplicate generator tuple fields must reject before map insertion");
        assert!(error.to_string().contains("repeats field"));
    }
}

#[test]
fn rewrite_variable_names_are_unique_before_path_endpoint_resolution() {
    for vars in [
        "x: A, y: A, p: A, p: A",
        "x: A, y: A, p: A, p: Path(x,y)",
        "x: A, y: A, p: Path(x,y), p: A",
        "x: A, y: A, p: Path(x,y), p: Path(x,y)",
    ] {
        let axi = format!(
            "module Demo\nschema S\n  object A\ntheory T on S\n  rewrite duplicate\n    vars: {vars}\n    lhs: refl(x)\n    rhs: refl(x)\n"
        );
        let module = parse_axi_v1(&axi).expect("duplicate rewrite variables remain in the AST");
        let initial_error = validate_axi_v1_module(module.clone())
            .expect_err("initial typechecking must reject every duplicate namespace order");
        assert!(initial_error.to_string().contains("duplicate variable `p`"));

        let schema = derive_runtime_schema_index(&module.schemas[0]);
        let runtime_error = derive_runtime_theory_index(&schema, &module.theories[0])
            .expect_err("runtime IR typechecking must reject every duplicate namespace order");
        assert!(runtime_error.contains("reuses variable name `p`"));
    }
}

#[test]
fn typecheck_accepts_runtime_checked_theory_slice() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint key Parent(child, parent)
  equation parent_refl:
    refl(Alice) = refl(Alice)
  rewrite parent_refl:
    vars: x: Person
    lhs: trans(refl(x), refl(x))
    rhs: refl(x)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(child=Alice, parent=Bob)}
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    assert_eq!(typed.proof().theory_count, 1);
}

#[test]
fn validated_module_exposes_compiled_theory_ir() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint key Parent(child, parent)
  rewrite normalize_parent:
    vars: x: Person, y: Person
    lhs: step(x, Parent, y)
    rhs: step(x, Parent, y)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    let theory = typed
        .compiled_theory_ir("S", "T")
        .expect("compiled theory")
        .expect("theory present");
    assert_eq!(theory.theory_id.as_str(), "theory:S:T");
    assert_eq!(theory.constraints.len(), 1);
    assert_eq!(theory.rewrite_rules.len(), 1);
    assert_eq!(
        theory.rewrite_rules[0].relation_refs,
        vec!["Parent".to_string()]
    );
}

#[test]
fn validated_module_exposes_runtime_theory_fragment_summary() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint key Parent(child, parent)
  equation opaque_business_rule:
    ParentCompose(a,b,c) = c
  rewrite normalize_parent:
    vars: x: Person, y: Person
    lhs: step(x, Parent, y)
    rhs: step(x, Parent, y)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    let summary = typed
        .runtime_theory_fragment_summary("S", "T")
        .expect("runtime fragment summary")
        .expect("theory present");

    assert_eq!(summary.version, RUNTIME_THEORY_FRAGMENT_SUMMARY_VERSION_V1);
    assert_eq!(summary.total_obligations, 3);
    assert_eq!(summary.runtime_checked_obligations, 2);
    assert_eq!(summary.opaque_or_out_of_fragment_obligations, 1);
    assert!(summary.obligation_statuses.iter().any(|status| {
        matches!(
            status.obligation_ref,
            TheoryObligationRefIr::OpaqueEquation { ref name, .. }
                if name == "opaque_business_rule"
        ) && status.fragment_status
            == RuntimeTheoryObligationFragmentStatusV1::OpaqueOrOutOfFragment
            && status.trust_class == RuntimeTheoryObligationTrustClassV1::ReviewOnly
    }));
    assert!(summary
        .notes
        .iter()
        .any(|note| note.contains("Rust runtime artifact")));
}

#[test]
fn reviewed_module_exposes_same_compiled_theory_ir() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint key Parent(child, parent)
  rewrite normalize_parent:
    vars: x: Person, y: Person
    lhs: step(x, Parent, y)
    rhs: step(x, Parent, y)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let validated: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    let validated_theory = validated
        .compiled_theory_ir("S", "T")
        .expect("compiled theory")
        .expect("theory present");
    let reviewed: Module<Reviewed> = review_axi_v1_module(
        validated,
        ReviewStamp {
            reviewer: Some("agent".to_string()),
            note: Some("reviewed".to_string()),
        },
    );
    let reviewed_theory = reviewed
        .compiled_theory_ir("S", "T")
        .expect("compiled theory")
        .expect("theory present");
    assert_eq!(reviewed_theory.theory_id, validated_theory.theory_id);
    assert_eq!(
        reviewed_theory.constraints.len(),
        validated_theory.constraints.len()
    );
    assert_eq!(
        reviewed_theory.rewrite_rules[0].rule_id,
        validated_theory.rewrite_rules[0].rule_id
    );
}

#[test]
fn compiled_schema_ir_and_compiled_theories_work_pre_import() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

theory T on S:
  constraint key Parent(child, parent)
"#;

    let module = parse_axi_v1(axi).expect("parse");
    let typed: Module<Validated> = validate_axi_v1_module(module).expect("typecheck");
    let compiled_schema = typed
        .compiled_schema_ir("S")
        .expect("compiled schema")
        .expect("schema present");
    let theories = typed
        .compiled_theories_for_schema("S")
        .expect("compiled theories");
    assert!(compiled_schema.relations.contains_key("Parent"));
    assert_eq!(theories.len(), 1);
    assert_eq!(theories[0].constraints.len(), 1);
}
