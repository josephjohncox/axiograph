//! PathIndex + LRU behavior tests.

use axiograph_pathdb::{PathDB, PathSig};
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

fn sample_db() -> (PathDB, u32, u32, u32, u32) {
    let mut db = PathDB::new();
    let a = db.add_entity("Node", vec![("name", "a")]);
    let b = db.add_entity("Node", vec![("name", "b")]);
    let c = db.add_entity("Node", vec![("name", "c")]);
    let d = db.add_entity("Node", vec![("name", "d")]);

    db.add_relation("r1", a, b, 1.0, vec![]);
    db.add_relation("r1", b, c, 1.0, vec![]);
    db.add_relation("r1", c, d, 1.0, vec![]);
    db.add_relation("r2", b, d, 1.0, vec![]);
    db.add_relation("r2", a, c, 1.0, vec![]);

    (db, a, b, c, d)
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
fn test_path_index_depth_one_builds_single_hop_only() {
    let (mut db, a, _b, c, _d) = sample_db();
    db.build_indexes_with_depth(1);

    let sig_1 = sig(&db, &["r1"]);
    let sig_2 = sig(&db, &["r1", "r1"]);

    assert!(db.path_index.query(a, &sig_1).is_some());
    assert!(db.path_index.query(a, &sig_2).is_none());

    let targets = db.follow_path(a, &["r1", "r1"]);
    assert!(targets.contains(c));
}

#[test]
fn test_path_index_depth_two_includes_two_hop_paths() {
    let (mut db, a, _b, c, _d) = sample_db();
    db.build_indexes_with_depth(2);

    let sig_2 = sig(&db, &["r1", "r1"]);
    let targets = db.path_index.query(a, &sig_2).unwrap().clone();
    assert!(targets.contains(c));
}

#[test]
fn test_lru_populates_for_deep_paths() {
    let (mut db, a, _b, c, _d) = sample_db();
    db.build_indexes_with_depth(1);
    db.set_path_index_lru_capacity(4);
    db.enable_path_index_lru_async(16);

    let sig_2 = sig(&db, &["r1", "r1"]);
    assert_eq!(db.path_index_lru_len(), 0);

    let targets = db.follow_path(a, &["r1", "r1"]);
    assert!(targets.contains(c));

    assert!(wait_for(|| db.path_index_lru_contains(&sig_2)));
    let cached = db.path_index.query_lru(a, &sig_2).unwrap();
    assert!(cached.contains(c));
}

#[test]
fn test_lru_skips_shallow_paths() {
    let (mut db, a, _b, c, _d) = sample_db();
    db.build_indexes_with_depth(2);
    db.set_path_index_lru_capacity(4);
    db.enable_path_index_lru_async(16);

    let targets = db.follow_path(a, &["r1", "r1"]);
    assert!(targets.contains(c));
    assert_eq!(db.path_index_lru_len(), 0);
}

#[test]
fn test_lru_eviction_for_deep_paths() {
    let (mut db, a, _b, c, d) = sample_db();
    db.build_indexes_with_depth(1);
    db.set_path_index_lru_capacity(1);
    db.enable_path_index_lru_async(16);

    let sig_r1_r1 = sig(&db, &["r1", "r1"]);
    let sig_r1_r2 = sig(&db, &["r1", "r2"]);

    let targets_1 = db.follow_path(a, &["r1", "r1"]);
    assert!(targets_1.contains(c));
    assert!(wait_for(|| db.path_index_lru_contains(&sig_r1_r1)));

    let targets_2 = db.follow_path(a, &["r1", "r2"]);
    assert!(targets_2.contains(d));
    assert!(wait_for(|| db.path_index_lru_contains(&sig_r1_r2)));

    assert_eq!(db.path_index_lru_len(), 1);
    assert!(!db.path_index_lru_contains(&sig_r1_r1));
    assert!(db.path_index_lru_contains(&sig_r1_r2));
}

#[test]
fn test_lru_async_updates() {
    let (mut db, a, _b, c, _d) = sample_db();
    db.build_indexes_with_depth(1);
    db.set_path_index_lru_capacity(4);
    db.enable_path_index_lru_async(16);

    let sig_2 = sig(&db, &["r1", "r1"]);
    let targets = db.follow_path(a, &["r1", "r1"]);
    assert!(targets.contains(c));

    assert!(wait_for(|| db.path_index_lru_contains(&sig_2)));
}

#[test]
fn test_lru_cleared_on_rebuild() {
    let (mut db, a, _b, c, _d) = sample_db();
    db.build_indexes_with_depth(1);
    db.set_path_index_lru_capacity(4);
    db.enable_path_index_lru_async(16);

    let targets = db.follow_path(a, &["r1", "r1"]);
    assert!(targets.contains(c));
    assert!(wait_for(|| db.path_index_lru_len() == 1));

    db.build_indexes_with_depth(1);
    assert!(wait_for(|| db.path_index_lru_len() == 0));
}
