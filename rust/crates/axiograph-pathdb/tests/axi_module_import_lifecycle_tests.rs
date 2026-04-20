use axiograph_dsl::axi_v1::parse_axi_v1;
use axiograph_pathdb::axi_module_export::export_axi_schema_v1_module_from_pathdb;
use axiograph_pathdb::axi_module_import::{
    import_axi_schema_v1_into_pathdb, import_axi_schema_v1_module_into_pathdb,
};
use axiograph_pathdb::axi_module_typecheck::{validate_axi_v1_module, ReviewStamp};
use axiograph_pathdb::PathDB;

#[test]
fn text_import_fails_closed_before_any_db_mutation_for_invalid_module() {
    let text = r#"
module ImportLifecycleFailClosed

schema S:
  object Person
  relation Parent(child: Person, parent: Person)

instance I of S:
  Person = {Alice, Bob}
  Parent = {(kid=Alice, parent=Bob)}
"#;

    let mut db = PathDB::new();
    let err = import_axi_schema_v1_into_pathdb(&mut db, text).expect_err("invalid module");

    assert!(
        err.to_string()
            .contains("failed to validate axi_schema_v1 module"),
        "expected validation failure, got {err}"
    );
    assert!(
        err.to_string().contains("unknown field `kid`"),
        "expected typecheck detail, got {err}"
    );
    assert!(
        db.entities.is_empty(),
        "db should remain empty on failed import"
    );
    assert!(
        db.relations.is_empty(),
        "db relations should remain empty on failed import"
    );
}

#[test]
fn reviewed_module_import_matches_validated_module_import() {
    let text = r#"
module ImportLifecycleEquivalence

schema S:
  object Person
  object Context
  relation Parent(child: Person, parent: Person) @context Context

theory T on S:
  constraint key Parent(child, parent, ctx)

instance I of S:
  Person = {Alice, Bob, Carol}
  Context = {Accepted}
  Parent = {
    (child=Alice, parent=Bob, ctx=Accepted),
    (child=Carol, parent=Bob, ctx=Accepted)
  }
"#;

    let parsed = parse_axi_v1(text).expect("parse");
    let validated = validate_axi_v1_module(parsed).expect("validate");
    let reviewed = validated.clone().into_reviewed(ReviewStamp {
        reviewer: Some("agent".to_string()),
        note: Some("reviewed".to_string()),
    });

    let mut validated_db = PathDB::new();
    let validated_summary =
        import_axi_schema_v1_module_into_pathdb(&mut validated_db, &validated).expect("import");

    let mut reviewed_db = PathDB::new();
    let reviewed_summary =
        import_axi_schema_v1_module_into_pathdb(&mut reviewed_db, &reviewed).expect("import");

    assert_eq!(validated_summary, reviewed_summary);
    assert_eq!(validated_db.entities.len(), reviewed_db.entities.len());
    assert_eq!(validated_db.relations.len(), reviewed_db.relations.len());

    let validated_export =
        export_axi_schema_v1_module_from_pathdb(&validated_db, "ImportLifecycleEquivalence")
            .expect("export validated");
    let reviewed_export =
        export_axi_schema_v1_module_from_pathdb(&reviewed_db, "ImportLifecycleEquivalence")
            .expect("export reviewed");

    assert_eq!(validated_export, reviewed_export);
}
