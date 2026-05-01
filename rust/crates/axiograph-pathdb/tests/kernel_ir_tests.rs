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
  relation Parent(parent: Person, child: Person, ctx: Context)

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
