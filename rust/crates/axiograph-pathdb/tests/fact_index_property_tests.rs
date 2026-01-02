use axiograph_pathdb::axi_meta::{ATTR_AXI_RELATION, ATTR_AXI_SCHEMA, REL_AXI_FACT_IN_CONTEXT};
use axiograph_pathdb::{CheckedDbMut, PathDB};
use proptest::prelude::*;
use roaring::RoaringBitmap;
use std::collections::HashMap;

const DEMO_AXI: &str = r#"
module FactIndexDemo

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

fn naive_fact_nodes_by_relation(db: &PathDB, relation_name: &str) -> RoaringBitmap {
    let Some(rel_attr_id) = db.interner.id_of(ATTR_AXI_RELATION) else {
        return RoaringBitmap::new();
    };
    let Some(relation_id) = db.interner.id_of(relation_name) else {
        return RoaringBitmap::new();
    };
    db.entities.entities_with_attr_value(rel_attr_id, relation_id)
}

fn naive_fact_nodes_by_schema_relation(db: &PathDB, schema: &str, relation: &str) -> RoaringBitmap {
    let Some(schema_attr_id) = db.interner.id_of(ATTR_AXI_SCHEMA) else {
        return RoaringBitmap::new();
    };
    let Some(schema_id) = db.interner.id_of(schema) else {
        return RoaringBitmap::new();
    };

    let facts = naive_fact_nodes_by_relation(db, relation);
    let in_schema = db.entities.entities_with_attr_value(schema_attr_id, schema_id);
    facts & in_schema
}

fn naive_fact_nodes_by_context(db: &PathDB, ctx_entity_id: u32) -> RoaringBitmap {
    let Some(ctx_rel_id) = db.interner.id_of(REL_AXI_FACT_IN_CONTEXT) else {
        return RoaringBitmap::new();
    };
    let mut out = RoaringBitmap::new();
    for source in 0..(db.entities.len() as u32) {
        for rel in db.relations.outgoing(source, ctx_rel_id) {
            if rel.target == ctx_entity_id {
                out.insert(source);
            }
        }
    }
    out
}

fn naive_fact_nodes_by_context_schema_relation(
    db: &PathDB,
    ctx_entity_id: u32,
    schema: &str,
    relation: &str,
) -> RoaringBitmap {
    let in_ctx = naive_fact_nodes_by_context(db, ctx_entity_id);
    let in_pair = naive_fact_nodes_by_schema_relation(db, schema, relation);
    in_ctx & in_pair
}

fn naive_key_lookup(
    db: &PathDB,
    schema: &str,
    relation: &str,
    key_fields: &[&str],
    key_values: &[u32],
) -> Vec<u32> {
    if key_fields.len() != key_values.len() {
        return Vec::new();
    }
    let facts = naive_fact_nodes_by_schema_relation(db, schema, relation);
    if facts.is_empty() {
        return Vec::new();
    }

    let mut field_rel_ids: Vec<axiograph_pathdb::StrId> = Vec::new();
    for f in key_fields {
        let Some(fid) = db.interner.id_of(f) else {
            return Vec::new();
        };
        field_rel_ids.push(fid);
    }

    let mut out: Vec<u32> = Vec::new();
    for fact in facts.iter() {
        let mut ok = true;
        for (i, &field_rel_id) in field_rel_ids.iter().enumerate() {
            let outgoing = db.relations.outgoing(fact, field_rel_id);
            if outgoing.len() != 1 || outgoing[0].target != key_values[i] {
                ok = false;
                break;
            }
        }
        if ok {
            out.push(fact);
        }
    }
    out.sort_unstable();
    out
}

#[derive(Debug, Clone)]
struct DemoFacts {
    parent: Vec<(usize, usize, usize, usize)>, // (child, parent, ctx, time)
    spouse: Vec<(usize, usize, usize)>,        // (a, b, ctx)
}

fn demo_facts_strategy() -> impl Strategy<Value = DemoFacts> {
    let parent = prop::collection::vec((0usize..6, 0usize..6, 0usize..2, 0usize..2), 1..=16);
    let spouse = prop::collection::vec((0usize..6, 0usize..6, 0usize..2), 1..=16);
    (parent, spouse).prop_map(|(parent, spouse)| DemoFacts { parent, spouse })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn fact_indexed_lookups_match_naive_scan(facts in demo_facts_strategy()) {
        let mut db = demo_db();
        let persons = ids_of_type(&db, "Person");
        let contexts = ids_of_type(&db, "Context");
        let times = ids_of_type(&db, "Time");
        prop_assume!(persons.len() >= 2 && contexts.len() >= 2 && times.len() >= 2);

        let mut inserted: HashMap<u32, (u32,u32,u32,u32)> = HashMap::new();

        let mut checked = CheckedDbMut::new(&mut db).expect("CheckedDbMut");
        for (c,p,ctx,t) in &facts.parent {
            let child = persons[*c % persons.len()];
            let parent = persons[*p % persons.len()];
            let ctx_id = contexts[*ctx % contexts.len()];
            let time = times[*t % times.len()];
            let mut b = checked.fact_builder("Demo", "Parent").expect("fact_builder Parent");
            b.set_field("child", child).expect("set child");
            b.set_field("parent", parent).expect("set parent");
            b.set_field("ctx", ctx_id).expect("set ctx");
            b.set_field("time", time).expect("set time");
            let fid = b.commit().expect("commit Parent");
            inserted.insert(fid, (child, parent, ctx_id, time));
        }
        for (a,b,ctx) in &facts.spouse {
            let a_id = persons[*a % persons.len()];
            let b_id = persons[*b % persons.len()];
            let ctx_id = contexts[*ctx % contexts.len()];
            let mut bld = checked.fact_builder("Demo", "Spouse").expect("fact_builder Spouse");
            bld.set_field("a", a_id).expect("set a");
            bld.set_field("b", b_id).expect("set b");
            bld.set_field("ctx", ctx_id).expect("set ctx");
            bld.commit().expect("commit Spouse");
        }

        let db = checked.db();

        // 1) (axi_relation) lookup
        let actual_parent = db.fact_nodes_by_axi_relation("Parent");
        let expected_parent = naive_fact_nodes_by_relation(db, "Parent");
        prop_assert_eq!(actual_parent, expected_parent);

        // 2) (axi_schema, axi_relation) lookup
        let actual_pair = db.fact_nodes_by_axi_schema_relation("Demo", "Parent");
        let expected_pair = naive_fact_nodes_by_schema_relation(db, "Demo", "Parent");
        prop_assert_eq!(actual_pair, expected_pair);

        // 3) context lookup (picked deterministically)
        let ctx0 = contexts[0];
        let actual_ctx = db.fact_nodes_by_context(ctx0);
        let expected_ctx = naive_fact_nodes_by_context(db, ctx0);
        prop_assert_eq!(actual_ctx, expected_ctx);

        // 4) context + schema + relation lookup
        let actual_ctx_pair = db.fact_nodes_by_context_axi_schema_relation(ctx0, "Demo", "Parent");
        let expected_ctx_pair = naive_fact_nodes_by_context_schema_relation(db, ctx0, "Demo", "Parent");
        prop_assert_eq!(actual_ctx_pair, expected_ctx_pair);

        // 5) key lookup should match naive for at least one inserted fact.
        let (&fact_id, &(child, parent, ctx, time)) = inserted
            .iter()
            .next()
            .expect("at least one Parent fact inserted");

        let expected = naive_key_lookup(db, "Demo", "Parent", &["child", "parent", "ctx", "time"], &[child, parent, ctx, time]);
        let actual = db.fact_nodes_by_axi_key("Demo", "Parent", &["child", "parent", "ctx", "time"], &[child, parent, ctx, time])
            .expect("key lookup available");

        let mut actual_sorted = actual;
        actual_sorted.sort_unstable();

        // Sanity: the chosen fact must be in the key result.
        prop_assert!(expected.contains(&fact_id));

        prop_assert_eq!(actual_sorted, expected);
    }
}
