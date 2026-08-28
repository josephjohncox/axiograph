use axiograph_pathdb::axi_meta::META_ATTR_NAME;
use axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb;
use axiograph_pathdb::PathDB;

fn find_named_entity(db: &PathDB, type_name: &str, name: &str) -> u32 {
    let key_id = db
        .interner
        .id_of(META_ATTR_NAME)
        .expect("name attr interned after import");
    let value_id = db.interner.id_of(name).expect("entity name interned");
    db.find_by_type(type_name)
        .expect("type exists")
        .iter()
        .find(|entity_id| db.entities.get_attr(*entity_id, key_id) == Some(value_id))
        .expect("named entity exists")
}

#[test]
fn importer_uses_declared_role_order_for_binary_carrier() {
    let mut db = PathDB::new();
    let axi = r#"
module Demo

schema S:
  object Person
  object Context
  relation Parent(parent: Person, child: Person, ctx: Context @context)

instance I of S:
  Person = {Alice, Bob}
  Context = {C0}
  Parent = {(parent=Bob, child=Alice, ctx=C0)}
"#;

    import_axi_schema_v1_into_pathdb(&mut db, axi).expect("import module");

    let alice = find_named_entity(&db, "Person", "Alice");
    let bob = find_named_entity(&db, "Person", "Bob");
    let parent_rel = db.interner.id_of("Parent").expect("derived edge label");

    assert!(
        db.relations.has_edge(bob, parent_rel, alice),
        "compiled relation semantics should preserve declared parent->child order"
    );
    assert!(
        !db.relations.has_edge(alice, parent_rel, bob),
        "endpoint choice should not be silently reordered by name-based child/parent heuristics"
    );
}

#[test]
fn importer_materializes_composable_explicit_generator_arrows() {
    let mut db = PathDB::new();
    let axi = r#"
module GeneratorDemo

schema S:
  object A
  object B
  object C
  function f: A -> B
  function g: B -> C

instance I of S:
  A = {a}
  B = {b}
  C = {c}
  f = {(source=a, target=b)}
  g = {(source=b, target=c)}
"#;

    let summary = import_axi_schema_v1_into_pathdb(&mut db, axi).expect("import module");
    let a = find_named_entity(&db, "A", "a");
    let b = find_named_entity(&db, "B", "b");
    let c = find_named_entity(&db, "C", "c");
    let f = db.interner.id_of("f").expect("f edge label");
    let g = db.interner.id_of("g").expect("g edge label");

    assert!(db.relations.has_edge(a, f, b));
    assert!(db.relations.has_edge(b, g, c));
    assert_eq!(summary.derived_edges_added, 2);
}

#[test]
fn importer_uses_compiled_homotopy_view_not_morphism_guessing() {
    let mut db = PathDB::new();
    let axi = r#"
module Demo

schema S:
  object World
  object Route
  object Context
  relation RouteEquivalence(from: World, to: World, route1: Route, route2: Route, ctx: Context)

instance I of S:
  World = {A, B}
  Route = {R1, R2}
  Context = {C0}
  RouteEquivalence = {
    (from=A, to=B, route1=R1, route2=R2, ctx=C0)
  }
"#;

    import_axi_schema_v1_into_pathdb(&mut db, axi).expect("import module");

    let r1 = find_named_entity(&db, "Route", "R1");
    let r2 = find_named_entity(&db, "Route", "R2");
    let homotopies = db.find_by_type("Homotopy").expect("homotopy virtual type");
    let tuple_entity = homotopies.iter().next().expect("homotopy tuple exists");
    let lhs_rel = db.interner.id_of("lhs").expect("lhs edge");
    let rhs_rel = db.interner.id_of("rhs").expect("rhs edge");
    let route_equiv_rel = db
        .interner
        .id_of("RouteEquivalence")
        .expect("derived equivalence edge");

    assert!(db.relations.has_edge(tuple_entity, lhs_rel, r1));
    assert!(db.relations.has_edge(tuple_entity, rhs_rel, r2));
    assert!(db.relations.has_edge(r1, route_equiv_rel, r2));
    assert!(
        db.find_by_type("Morphism")
            .map(|ids| !ids.contains(tuple_entity))
            .unwrap_or(true),
        "equivalence tuples should not also be projected as morphisms"
    );
}

#[test]
fn importer_keeps_world_typed_scope_as_data_without_context_declaration() {
    let mut db = PathDB::new();
    let axi = r#"
module Demo

schema S:
  object Person
  object World
  relation Parent(parent: Person, child: Person, scope: World)

instance I of S:
  Person = {Alice, Bob}
  World = {W0}
  Parent = {(parent=Bob, child=Alice, scope=W0)}
"#;

    import_axi_schema_v1_into_pathdb(&mut db, axi).expect("import module");

    let alice = find_named_entity(&db, "Person", "Alice");
    let bob = find_named_entity(&db, "Person", "Bob");
    let w0 = find_named_entity(&db, "World", "W0");
    let parent_rel = db.interner.id_of("Parent").expect("derived edge label");

    assert!(
        !db.relations.has_edge(bob, parent_rel, alice),
        "world-typed extra data roles should not be silently dropped into carrier inference"
    );
    assert!(
        !db.relations.has_edge(alice, parent_rel, bob),
        "no fallback carrier should be synthesized when there are 3 data roles"
    );
    let scope_rel = db.interner.id_of("scope").expect("scope field edge");
    let parent_facts = db.find_by_type("Parent").expect("fact tuples exist");
    let fact = parent_facts.iter().next().expect("parent fact exists");
    assert!(db.relations.has_edge(fact, scope_rel, w0));
}

#[test]
fn derived_runtime_index_retains_canonical_snapshot_only_in_process() {
    let axi = r#"
module Demo

schema S:
  object Person
  relation Parent(child: Person, parent: Person)
  relation Review(parent_fact: relation(Parent))
  function manager: Person -> Person

instance I of S:
  Person = {Alice, Bob}
  Parent = {parent_1: (child=Alice, parent=Bob)}
  Review = {(parent_fact=parent_1)}
  manager = {(source=Alice, target=Bob), (source=Bob, target=Bob)}
"#;
    let module = axiograph_dsl::axi_v1::parse_axi_v1(axi).expect("parse fixture");
    let index = axiograph_pathdb::derive_runtime_module_index(&module, axi)
        .expect("derive canonical-backed runtime index");

    let snapshot = index
        .canonical_snapshot()
        .expect("in-process runtime index must retain canonical authority");
    assert_eq!(snapshot.ir().version(), "kernel_snapshot_ir_v2");
    assert_eq!(snapshot.ir().ordered_module_closure().len(), 1);
    let surface = index.runtime_semantic_index();
    assert_eq!(surface.refs.len(), snapshot.ir().refs().len());
    assert!(surface
        .refs
        .iter()
        .all(|reference| matches!(reference, axiograph_pathdb::RuntimeIrRef::Canonical { .. })));
    assert!(surface.refs.iter().any(|reference| matches!(
        reference,
        axiograph_pathdb::RuntimeIrRef::Canonical { citation }
            if citation.label == "manager"
                && matches!(&citation.reference, axiograph_pathdb::KernelRefV2::Generator { .. })
    )));
    assert!(surface.refs.iter().any(|reference| matches!(
        reference,
        axiograph_pathdb::RuntimeIrRef::Canonical { citation }
            if citation.label == "Review.parent_fact"
                && matches!(&citation.reference, axiograph_pathdb::KernelRefV2::Role { .. })
    )));
    let schema = &snapshot.ir().schemas()[0];
    let review = schema
        .relations
        .iter()
        .find(|relation| relation.label == "Review")
        .expect("Review relation object");
    assert_eq!(review.roles[0].declared_order, 0);
    assert!(matches!(
        &review.roles[0].type_expr,
        axiograph_kernel::TypeExprIr::RelationObject { .. }
    ));

    let serialized = serde_json::to_string(&index).expect("serialize derived runtime index");
    let restored: axiograph_pathdb::RuntimeModuleIndex =
        serde_json::from_str(&serialized).expect("deserialize runtime citation");
    assert!(
        restored.canonical_snapshot().is_none(),
        "serialized runtime citations must not recreate canonical authority"
    );
    assert_eq!(restored.runtime_semantic_index().refs, surface.refs);
}

#[test]
fn runtime_package_index_uses_imported_schema_and_retains_package_snapshot() {
    let base = axiograph_kernel::CanonicalModuleSource::parse(
        b"module Base\n\nschema Shared:\n  object Person\n  relation Parent(child: Person, parent: Person)\n"
            .to_vec(),
    )
    .expect("parse base");
    let root = axiograph_kernel::CanonicalModuleSource::parse(
        b"module Root\nimport Base\n\ntheory Rules on Shared:\n  constraint key Parent(child)\n\ninstance I of Shared:\n  Person = {Alice, Bob}\n  Parent = {(child=Alice, parent=Bob)}\n"
            .to_vec(),
    )
    .expect("parse root");
    let snapshot =
        axiograph_kernel::CanonicalCompiler::compile(axiograph_kernel::KernelCompilationRequest {
            repository_id: axiograph_kernel::RepositoryIdV2::from_descriptor_bytes(
                b"runtime-package-test",
            ),
            accepted_snapshot_id: axiograph_kernel::SnapshotIdV2::from_canonical_fields(&[
                base.exact_text().as_bytes(),
                root.exact_text().as_bytes(),
            ]),
            root_module: "Root".to_string(),
            modules: vec![root.clone(), base.clone()],
        })
        .expect("compile package");

    let index = axiograph_pathdb::derive_runtime_package_index(&snapshot, &[base, root])
        .expect("derive package runtime index");
    assert_eq!(index.schemas.len(), 1);
    assert_eq!(index.theories.len(), 1);
    assert_eq!(index.instances.len(), 1);
    assert_eq!(
        index
            .canonical_snapshot()
            .expect("retained package snapshot")
            .ir()
            .ir_digest(),
        snapshot.ir().ir_digest()
    );
}
