use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::axi_module_typecheck::TypedAxiV1Module;

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
    let typed = TypedAxiV1Module::new(module).expect("typecheck");
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
    let err = TypedAxiV1Module::new(module).unwrap_err();
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
    let err = TypedAxiV1Module::new(module).unwrap_err();
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
    let err = TypedAxiV1Module::new(module).unwrap_err();
    assert!(err
        .to_string()
        .contains("mixes identifiers and tuples"));
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
    let err = TypedAxiV1Module::new(module).unwrap_err();
    assert!(err.to_string().contains("unknown field"));
}

