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
