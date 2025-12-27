use axiograph_pathdb::axi_semantics::{AxiTypeCheckError, MetaPlaneIndex};
use axiograph_pathdb::PathDB;

#[test]
fn meta_plane_index_builds_and_typechecks_valid_instance() {
    let text = r#"
module TestSemantics

schema S:
  object A
  object B
  relation R(from: A, to: B)

instance I of S:
  A = {a0}
  B = {b0}
  R = {(from=a0, to=b0)}
"#;

    let mut db = PathDB::new();
    let m = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(&mut db, &m)
        .expect("import module");
    db.build_indexes();

    let meta = MetaPlaneIndex::from_db(&db).expect("build meta index");
    let report = meta.typecheck_axi_facts(&db);
    assert!(
        report.ok(),
        "expected typecheck to pass, errors={:?}",
        report.errors
    );
    assert_eq!(report.checked_facts, 1);
}

#[test]
fn typechecker_reports_field_type_mismatch() {
    let text = r#"
module TestSemantics

schema S:
  object A
  object B
  relation R(from: A, to: B)

instance I of S:
  A = {a0}
  B = {b0}
  R = {(from=a0, to=b0)}
"#;

    let mut db = PathDB::new();
    let m = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(&mut db, &m)
        .expect("import module");

    // Add a deliberately ill-typed fact node:
    // `R(from: A, to: B)` but we set `to = A`.
    let a1 = db.add_entity("A", vec![("name", "a1"), ("axi_schema", "S")]);
    let a2 = db.add_entity("A", vec![("name", "a2"), ("axi_schema", "S")]);
    let bad_fact = db.add_entity(
        "R",
        vec![
            ("name", "bad_fact"),
            ("axi_schema", "S"),
            ("axi_relation", "R"),
        ],
    );
    db.add_relation("from", bad_fact, a1, 1.0, vec![]);
    db.add_relation("to", bad_fact, a2, 1.0, vec![]);
    db.build_indexes();

    let meta = MetaPlaneIndex::from_db(&db).expect("build meta index");
    let report = meta.typecheck_axi_facts(&db);
    assert!(!report.ok(), "expected typecheck failure");

    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            AxiTypeCheckError::FieldTypeMismatch {
                relation,
                field,
                expected_type,
                actual_type,
                ..
            } if relation == "R" && field == "to" && expected_type == "B" && actual_type == "A"
        )),
        "expected FieldTypeMismatch error, got {:?}",
        report.errors
    );
}
