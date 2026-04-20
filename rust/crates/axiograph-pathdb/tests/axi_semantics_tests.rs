use axiograph_pathdb::axi_semantics::{AxiTypeCheckError, MetaPlaneIndex};
use axiograph_pathdb::kernel_ir::{compile_schema_ir, CarrierSource, WitnessViewIr};
use axiograph_pathdb::PathDB;
use std::fs;
use std::path::PathBuf;

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
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, text)
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
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, text)
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

#[test]
fn meta_plane_compiled_ir_matches_schema_ast_for_endpoint_and_homotopy_semantics() {
    let text = r#"
module TestSemantics

schema S:
  object Person
  object World
  object Route
  relation Parent(parent: Person, child: Person, scope: World)
  relation RouteWitness(from: World, to: World, route1: Route, route2: Route)

instance I of S:
  Person = {Alice, Bob}
  World = {W0, W1}
  Route = {R1, R2}
  Parent = {(parent=Bob, child=Alice, scope=W0)}
  RouteWitness = {(from=W0, to=W1, route1=R1, route2=R2)}
"#;

    let module = axiograph_dsl::axi_v1::parse_axi_v1(text).expect("parse axi");
    let schema_ast = module
        .schemas
        .iter()
        .find(|schema| schema.name == "S")
        .expect("schema S");
    let expected_ir = compile_schema_ir(schema_ast);

    let mut db = PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, text)
        .expect("import module");
    db.build_indexes();

    let meta = MetaPlaneIndex::from_db(&db).expect("build meta index");
    let reconstructed_ir = meta.compiled_schema_ir("S").expect("compiled schema ir");

    assert_eq!(
        reconstructed_ir, expected_ir,
        "meta-plane reconstruction should stay in lockstep with schema-AST kernel IR compilation"
    );

    let parent = reconstructed_ir
        .relation("Parent")
        .expect("parent relation semantics");
    assert_eq!(parent.carrier_field_names(), None);

    let route_witness = reconstructed_ir
        .relation("RouteWitness")
        .expect("route witness semantics");
    assert_eq!(
        route_witness.carrier_field_names(),
        Some(("route1", "route2"))
    );
    assert_eq!(
        route_witness.witness_view,
        WitnessViewIr::Homotopy {
            lhs_role: 2,
            rhs_role: 3,
        }
    );
    assert_eq!(
        route_witness.carrier.as_ref().map(|carrier| carrier.source),
        Some(CarrierSource::HomotopyConvention)
    );
    assert_eq!(
        route_witness
            .carrier
            .as_ref()
            .map(|carrier| carrier.fiber_roles.clone()),
        Some(Vec::new())
    );
}

#[test]
fn canonical_examples_compiled_ir_match_meta_plane_ir() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize repo root");

    let fixtures = [
        repo_root.join("examples/economics/EconomicFlows.axi"),
        repo_root.join("examples/ontology/SchemaEvolution.axi"),
    ];

    for fixture in fixtures {
        let text = fs::read_to_string(&fixture).expect("read canonical fixture");
        let module = axiograph_dsl::axi_v1::parse_axi_v1(&text).expect("parse canonical fixture");

        let mut db = PathDB::new();
        axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, &text)
            .expect("import canonical fixture");
        db.build_indexes();

        let meta = MetaPlaneIndex::from_db(&db).expect("build meta index");
        for schema_ast in &module.schemas {
            let expected_ir = compile_schema_ir(schema_ast);
            let reconstructed_ir = meta
                .compiled_schema_ir(&schema_ast.name)
                .expect("compiled schema ir");
            assert_eq!(
                reconstructed_ir,
                expected_ir,
                "fixture={} schema={}",
                fixture.display(),
                schema_ast.name
            );
        }
    }
}
