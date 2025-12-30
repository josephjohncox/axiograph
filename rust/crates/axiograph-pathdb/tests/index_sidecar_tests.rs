use axiograph_pathdb::axi_meta::{ATTR_AXI_RELATION, ATTR_AXI_SCHEMA};
use axiograph_pathdb::{PathDB, PathSig};
use std::sync::Arc;
use std::time::Duration;

fn wait_for(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..50 {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn sig(db: &PathDB, rels: &[&str]) -> PathSig {
    let mut ids = Vec::with_capacity(rels.len());
    for rel in rels {
        let id = db
            .interner
            .id_of(rel)
            .unwrap_or_else(|| panic!("missing relation {rel}"));
        ids.push(id);
    }
    PathSig::new(ids)
}

#[test]
fn test_fact_index_invalidates_on_mutation() {
    let mut db = PathDB::new();
    let f1 = db.add_entity(
        "Fact",
        vec![(ATTR_AXI_RELATION, "rel"), (ATTR_AXI_SCHEMA, "schema")],
    );

    let hits = db.fact_nodes_by_axi_relation("rel");
    assert!(hits.contains(f1));

    let f2 = db.add_entity(
        "Fact",
        vec![(ATTR_AXI_RELATION, "rel"), (ATTR_AXI_SCHEMA, "schema")],
    );

    let hits = db.fact_nodes_by_axi_relation("rel");
    assert!(hits.contains(f1));
    assert!(hits.contains(f2));
}

#[test]
fn test_text_index_invalidates_on_mutation() {
    let mut db = PathDB::new();
    let a = db.add_entity("Node", vec![("name", "alpha beta")]);

    let hits = db.entities_with_attr_fts("name", "alpha");
    assert!(hits.contains(a));

    let b = db.add_entity("Node", vec![("name", "alpha gamma")]);
    let hits = db.entities_with_attr_fts("name", "alpha");
    assert!(hits.contains(b));
}

#[test]
fn test_async_fact_and_text_builds() {
    let mut db = PathDB::new();
    let f1 = db.add_entity(
        "Fact",
        vec![(ATTR_AXI_RELATION, "rel"), (ATTR_AXI_SCHEMA, "schema")],
    );
    let e1 = db.add_entity("Node", vec![("name", "alpha beta")]);

    let db = Arc::new(db);
    db.attach_async_index_source(Arc::downgrade(&db));

    let hits = db.fact_nodes_by_axi_relation("rel");
    assert!(hits.contains(f1));
    let hits = db.entities_with_attr_fts("name", "alpha");
    assert!(hits.contains(e1));

    let name_id = db.interner.id_of("name").unwrap();
    assert!(wait_for(|| db.snapshot_index_sidecar(None).fact_index.is_some()));
    assert!(wait_for(|| db.snapshot_index_sidecar(None).text_indexes.contains_key(&name_id)));
}

#[test]
fn test_index_sidecar_roundtrip_with_lru() {
    let mut db = PathDB::new();
    let a = db.add_entity("Node", vec![("name", "a")]);
    let b = db.add_entity("Node", vec![("name", "b")]);
    let c = db.add_entity("Node", vec![("name", "c")]);

    db.add_relation("r1", a, b, 1.0, vec![]);
    db.add_relation("r1", b, c, 1.0, vec![]);

    db.build_indexes_with_depth(1);
    db.enable_path_index_lru_async(16);
    db.set_path_index_lru_capacity(4);

    let sig_r1_r1 = sig(&db, &["r1", "r1"]);
    let targets = db.follow_path(a, &["r1", "r1"]);
    assert!(targets.contains(c));
    assert!(wait_for(|| db.path_index_lru_contains(&sig_r1_r1)));

    let sidecar = db.snapshot_index_sidecar(Some("snap".to_string()));
    let mut buf = Vec::new();
    ciborium::ser::into_writer(&sidecar, &mut buf).unwrap();
    let sidecar: axiograph_pathdb::PathDbIndexSidecarV1 =
        ciborium::de::from_reader(buf.as_slice()).unwrap();

    let bytes = db.to_bytes().unwrap();
    let mut db2 = PathDB::from_bytes(&bytes).unwrap();
    db2.enable_path_index_lru_async(16);
    db2.set_path_index_lru_capacity(4);
    db2.load_index_sidecar(sidecar);

    let sig_r1_r1_b = sig(&db2, &["r1", "r1"]);
    assert!(wait_for(|| db2.path_index_lru_contains(&sig_r1_r1_b)));
}

#[test]
fn test_path_index_lru_eviction_respects_capacity() {
    let mut db = PathDB::new();
    let a = db.add_entity("Node", vec![("name", "a")]);
    let b = db.add_entity("Node", vec![("name", "b")]);
    let c = db.add_entity("Node", vec![("name", "c")]);
    let d = db.add_entity("Node", vec![("name", "d")]);

    db.add_relation("r1", a, b, 1.0, vec![]);
    db.add_relation("r1", b, c, 1.0, vec![]);
    db.add_relation("r1", c, d, 1.0, vec![]);

    db.build_indexes_with_depth(1);
    db.enable_path_index_lru_async(16);
    db.set_path_index_lru_capacity(1);

    let sig_r1_r1 = sig(&db, &["r1", "r1"]);
    let sig_r1_r1_r1 = sig(&db, &["r1", "r1", "r1"]);

    let _ = db.follow_path(a, &["r1", "r1"]);
    assert!(wait_for(|| db.path_index_lru_contains(&sig_r1_r1)));

    let _ = db.follow_path(a, &["r1", "r1", "r1"]);
    assert!(wait_for(|| db.path_index_lru_contains(&sig_r1_r1_r1)));
    assert!(wait_for(|| db.path_index_lru_len() <= 1));
}

#[test]
fn test_path_index_lru_clears_on_mutation() {
    let mut db = PathDB::new();
    let a = db.add_entity("Node", vec![("name", "a")]);
    let b = db.add_entity("Node", vec![("name", "b")]);
    let c = db.add_entity("Node", vec![("name", "c")]);

    db.add_relation("r1", a, b, 1.0, vec![]);
    db.add_relation("r1", b, c, 1.0, vec![]);

    db.build_indexes_with_depth(1);
    db.enable_path_index_lru_async(16);
    db.set_path_index_lru_capacity(4);

    let sig_r1_r1 = sig(&db, &["r1", "r1"]);
    let _ = db.follow_path(a, &["r1", "r1"]);
    assert!(wait_for(|| db.path_index_lru_contains(&sig_r1_r1)));

    let _ = db.add_entity("Node", vec![("name", "d")]);
    assert!(wait_for(|| db.path_index_lru_len() == 0));
}
