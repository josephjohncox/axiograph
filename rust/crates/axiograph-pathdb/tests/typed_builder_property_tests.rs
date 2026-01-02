use axiograph_pathdb::axi_meta::{ATTR_AXI_RELATION, ATTR_AXI_SCHEMA, META_REL_FACT_OF, REL_AXI_FACT_IN_CONTEXT};
use axiograph_pathdb::{CheckedDb, CheckedDbMut, PathDB};
use proptest::prelude::*;

const DEMO_AXI: &str = r#"
module TypedBuilders

schema Demo:
  object Person
  object Context
  object Time

  relation Parent(child: Person, parent: Person) @context Context @temporal Time
  relation Spouse(a: Person, b: Person) @context Context

theory DemoRules on Demo:
  constraint key Parent(child, parent, ctx, time)
  constraint key Spouse(a, b, ctx)

instance DemoInst of Demo:
  Person = {P0, P1, P2, P3, P4, P5}
  Context = {C0, C1}
  Time = {T0, T1}

  Parent = {}
  Spouse = {}
"#;

fn demo_db() -> PathDB {
    let mut db = PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_into_pathdb(&mut db, DEMO_AXI)
        .expect("import demo module");
    db
}

fn ids_of_type(db: &PathDB, ty: &str) -> Vec<u32> {
    db.find_by_type(ty)
        .map(|ids| ids.iter().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn outgoing_targets(db: &PathDB, source: u32, rel: &str) -> Vec<u32> {
    let Some(rel_id) = db.interner.id_of(rel) else {
        return Vec::new();
    };
    let mut out = db
        .relations
        .outgoing(source, rel_id)
        .into_iter()
        .map(|e| e.target)
        .collect::<Vec<_>>();
    out.sort_unstable();
    out
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 192,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn typed_fact_builder_commit_creates_required_edges(
        child_idx in 0usize..6,
        parent_idx in 0usize..6,
        ctx_idx in 0usize..2,
        time_idx in 0usize..2,
    ) {
        let mut db = demo_db();
        let persons = ids_of_type(&db, "Person");
        let contexts = ids_of_type(&db, "Context");
        let times = ids_of_type(&db, "Time");
        prop_assume!(!persons.is_empty() && !contexts.is_empty() && !times.is_empty());

        let child = persons[child_idx % persons.len()];
        let parent = persons[parent_idx % persons.len()];
        let ctx = contexts[ctx_idx % contexts.len()];
        let time = times[time_idx % times.len()];

        let mut checked = CheckedDbMut::new(&mut db).expect("CheckedDbMut");
        let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder");
        b.set_field("child", child).expect("set child");
        b.set_field("parent", parent).expect("set parent");
        b.set_field("ctx", ctx).expect("set ctx");
        b.set_field("time", time).expect("set time");
        let fact = b.commit().expect("commit");

        let db = checked.db();

        // Required meta attrs for canonical fact nodes.
        let schema_attr = db.interner.id_of(ATTR_AXI_SCHEMA).expect("axi_schema interned");
        let rel_attr = db.interner.id_of(ATTR_AXI_RELATION).expect("axi_relation interned");
        prop_assert!(db.entities.get_attr(fact, schema_attr).is_some());
        prop_assert!(db.entities.get_attr(fact, rel_attr).is_some());

        // Field edges are present and unique.
        prop_assert_eq!(outgoing_targets(db, fact, "child"), vec![child]);
        prop_assert_eq!(outgoing_targets(db, fact, "parent"), vec![parent]);
        prop_assert_eq!(outgoing_targets(db, fact, "ctx"), vec![ctx]);
        prop_assert_eq!(outgoing_targets(db, fact, "time"), vec![time]);

        // Uniform context edge matches the `ctx` field (invariant).
        prop_assert_eq!(outgoing_targets(db, fact, REL_AXI_FACT_IN_CONTEXT), vec![ctx]);

        // Fact nodes are linked to their meta-plane relation declaration.
        prop_assert_eq!(outgoing_targets(db, fact, META_REL_FACT_OF).len(), 1);

        // Whole-DB check should still pass (typed construction stays "checked").
        prop_assert!(CheckedDb::check(db).expect("check").ok);
    }

    #[test]
    fn typed_fact_builder_rejects_unknown_field(
        child_idx in 0usize..6,
        ctx_idx in 0usize..2,
        time_idx in 0usize..2,
    ) {
        let mut db = demo_db();
        let persons = ids_of_type(&db, "Person");
        let contexts = ids_of_type(&db, "Context");
        let times = ids_of_type(&db, "Time");
        prop_assume!(!persons.is_empty() && !contexts.is_empty() && !times.is_empty());

        let child = persons[child_idx % persons.len()];
        let ctx = contexts[ctx_idx % contexts.len()];
        let time = times[time_idx % times.len()];

        let mut checked = CheckedDbMut::new(&mut db).expect("CheckedDbMut");
        let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder");
        b.set_field("child", child).expect("set child");
        b.set_field("ctx", ctx).expect("set ctx");
        b.set_field("time", time).expect("set time");

        prop_assert!(b.set_field("does_not_exist", child).is_err());
    }

    #[test]
    fn typed_fact_builder_rejects_wrong_value_type(
        parent_idx in 0usize..6,
        ctx_idx in 0usize..2,
        time_idx in 0usize..2,
    ) {
        let mut db = demo_db();
        let persons = ids_of_type(&db, "Person");
        let contexts = ids_of_type(&db, "Context");
        let times = ids_of_type(&db, "Time");
        prop_assume!(!persons.is_empty() && !contexts.is_empty() && !times.is_empty());

        let parent = persons[parent_idx % persons.len()];
        let ctx = contexts[ctx_idx % contexts.len()];
        let time = times[time_idx % times.len()];

        // Intentionally use a Context where a Person is expected.
        let mut checked = CheckedDbMut::new(&mut db).expect("CheckedDbMut");
        let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder");
        prop_assert!(b.set_field("child", ctx).is_err());

        // But valid assignments should work.
        b.set_field("child", parent).expect("set child");
        b.set_field("parent", parent).expect("set parent");
        b.set_field("ctx", ctx).expect("set ctx");
        b.set_field("time", time).expect("set time");
        b.commit().expect("commit");
    }

    #[test]
    fn commit_into_existing_rejects_conflicting_field_values(
        child_idx in 0usize..6,
        parent_idx in 0usize..6,
        ctx_idx in 0usize..2,
        time_idx in 0usize..2,
    ) {
        let mut db = demo_db();
        let persons = ids_of_type(&db, "Person");
        let contexts = ids_of_type(&db, "Context");
        let times = ids_of_type(&db, "Time");
        prop_assume!(persons.len() >= 2 && !contexts.is_empty() && !times.is_empty());

        let child = persons[child_idx % persons.len()];
        let parent = persons[parent_idx % persons.len()];
        let other_parent = persons[(parent_idx + 1) % persons.len()];
        let ctx = contexts[ctx_idx % contexts.len()];
        let time = times[time_idx % times.len()];

        let mut checked = CheckedDbMut::new(&mut db).expect("CheckedDbMut");

        // Create a baseline well-typed fact node.
        let fact = {
            let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder");
            b.set_field("child", child).expect("set child");
            b.set_field("parent", parent).expect("set parent");
            b.set_field("ctx", ctx).expect("set ctx");
            b.set_field("time", time).expect("set time");
            b.commit().expect("commit")
        };

        // Re-applying the same field values into the same fact is OK.
        {
            let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder");
            b.set_field("child", child).expect("set child");
            b.set_field("parent", parent).expect("set parent");
            b.set_field("ctx", ctx).expect("set ctx");
            b.set_field("time", time).expect("set time");
            let out = b.commit_into_existing(fact).expect("commit_into_existing");
            prop_assert_eq!(out, fact);
        }

        // Conflicting existing field values must be rejected.
        {
            let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder");
            b.set_field("child", child).expect("set child");
            b.set_field("parent", other_parent).expect("set parent");
            b.set_field("ctx", ctx).expect("set ctx");
            b.set_field("time", time).expect("set time");
            prop_assert!(b.commit_into_existing(fact).is_err());
        }
    }
}

