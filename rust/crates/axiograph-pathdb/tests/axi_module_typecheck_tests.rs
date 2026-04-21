use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::axi_module_typecheck::{
    review_axi_v1_module, validate_axi_v1_module, Module, ReviewStamp,
};
use axiograph_pathdb::{Reviewed, Validated};

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
