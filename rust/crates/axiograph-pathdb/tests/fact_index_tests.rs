use anyhow::Result;

#[test]
fn fact_nodes_by_axi_relation_works_without_axi_schema_attr() {
    let mut db = axiograph_pathdb::PathDB::new();

    let a = db.add_entity("Node", vec![("name", "a")]);
    let b = db.add_entity("Node", vec![("name", "b")]);

    // Minimal "fact node" shape: has `axi_relation`, but no `axi_schema`.
    let f = db.add_entity(
        "Fact",
        vec![
            ("name", "flow_0"),
            (axiograph_pathdb::axi_meta::ATTR_AXI_RELATION, "Flow"),
        ],
    );
    let _ = db.add_relation("from", f, a, 1.0, Vec::new());
    let _ = db.add_relation("to", f, b, 1.0, Vec::new());

    let facts = db.fact_nodes_by_axi_relation("Flow");
    assert!(facts.contains(f));
    assert_eq!(facts.len(), 1);
}

fn entity_id_by_name(db: &axiograph_pathdb::PathDB, name: &str) -> Result<u32> {
    let key = db
        .interner
        .id_of("name")
        .ok_or_else(|| anyhow::anyhow!("missing `name` attr key in interner"))?;
    let value = db
        .interner
        .id_of(name)
        .ok_or_else(|| anyhow::anyhow!("missing `{name}` value in interner"))?;
    let ids = db.entities.entities_with_attr_value(key, value);
    ids.iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no entity with name `{name}`"))
}

#[test]
fn fact_key_lookup_uses_meta_plane_key_constraints() -> Result<()> {
    let text = r#"
module FactIndexTest

schema S:
  object Node
  relation Flow(from: Node, to: Node)

theory T on S:
  constraint key Flow(from, to)

instance I of S:
  Node = {a, b, c}
  Flow = {
    (from=a, to=b),
    (from=a, to=c)
  }
"#;

    let m = axiograph_dsl::axi_v1::parse_axi_v1(text)?;
    let mut db = axiograph_pathdb::PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(&mut db, &m)?;

    let a = entity_id_by_name(&db, "a")?;
    let b = entity_id_by_name(&db, "b")?;

    let facts = db.fact_nodes_by_axi_schema_relation("S", "Flow");
    assert_eq!(facts.len(), 2);

    let key_hit = db.fact_nodes_by_axi_key("S", "Flow", &["from", "to"], &[a, b]);
    let key_hit = key_hit.expect("expected key index to be present");
    assert_eq!(key_hit.len(), 1);

    Ok(())
}

#[test]
fn fact_nodes_by_context_works_for_axi_context_scoping() -> Result<()> {
    let text = r#"
module FactIndexContextTest

schema S:
  object Node
  object Context
  relation Flow(from: Node, to: Node) @context Context

theory T on S:
  constraint key Flow(from, to, ctx)

instance I of S:
  Node = {a, b, c}
  Context = {Accepted, Evidence}
  Flow = {
    (from=a, to=b, ctx=Accepted),
    (from=a, to=c, ctx=Evidence)
  }
"#;

    let m = axiograph_dsl::axi_v1::parse_axi_v1(text)?;
    let mut db = axiograph_pathdb::PathDB::new();
    axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(&mut db, &m)?;

    let accepted = entity_id_by_name(&db, "Accepted")?;
    let evidence = entity_id_by_name(&db, "Evidence")?;

    let flow_all = db.fact_nodes_by_axi_schema_relation("S", "Flow");
    assert_eq!(flow_all.len(), 2);

    let flow_accepted = db.fact_nodes_by_context_axi_schema_relation(accepted, "S", "Flow");
    assert_eq!(flow_accepted.len(), 1);

    let flow_evidence = db.fact_nodes_by_context_axi_schema_relation(evidence, "S", "Flow");
    assert_eq!(flow_evidence.len(), 1);

    // The derived scoping edge exists on the fact node.
    let ctx_rel_id = db
        .interner
        .id_of(axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT)
        .expect("axi_fact_in_context must be interned after import");
    for fact in flow_accepted.iter() {
        assert!(db.relations.has_edge(fact, ctx_rel_id, accepted));
    }

    Ok(())
}
