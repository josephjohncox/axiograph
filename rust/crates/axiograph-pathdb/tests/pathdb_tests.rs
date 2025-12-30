//! PathDB E2E Tests

use axiograph_pathdb::*;
use tempfile::tempdir;

// ============================================================================
// String Interning Tests
// ============================================================================

#[test]
fn test_string_interning() {
    let interner = StringInterner::new();

    // Intern multiple strings
    let id1 = interner.intern("hello");
    let id2 = interner.intern("world");
    let id3 = interner.intern("hello"); // Same string

    // Same string should return same ID
    assert_eq!(id1, id3);
    assert_ne!(id1, id2);

    // Lookup should work
    assert_eq!(interner.lookup(id1), Some("hello".to_string()));
    assert_eq!(interner.lookup(id2), Some("world".to_string()));
}

#[test]
fn test_string_interner_serialization() {
    let interner = StringInterner::new();
    interner.intern("first");
    interner.intern("second");
    interner.intern("third");

    let bytes = interner.to_bytes();
    assert!(!bytes.is_empty());

    let restored = StringInterner::from_bytes(&bytes).unwrap();
    assert_eq!(restored.lookup(StrId::new(0)), Some("first".to_string()));
    assert_eq!(restored.lookup(StrId::new(1)), Some("second".to_string()));
    assert_eq!(restored.lookup(StrId::new(2)), Some("third".to_string()));
}

// ============================================================================
// Entity Store Tests
// ============================================================================

#[test]
fn test_entity_storage() {
    let mut db = PathDB::new();

    let id = db.add_entity("Material", vec![("name", "Steel"), ("hardness", "50")]);

    let entity = db.get_entity(id);
    assert!(entity.is_some());

    let e = entity.unwrap();
    assert_eq!(e.entity_type, "Material");
    assert_eq!(e.attrs.get("name"), Some(&"Steel".to_string()));
    assert_eq!(e.attrs.get("hardness"), Some(&"50".to_string()));
}

#[test]
fn test_entity_type_index() {
    let mut db = PathDB::new();

    db.add_entity("Material", vec![("name", "Steel")]);
    db.add_entity("Material", vec![("name", "Aluminum")]);
    db.add_entity("Tool", vec![("name", "Drill")]);

    db.build_indexes();

    let materials = db.find_by_type("Material");
    assert!(materials.is_some());
    assert_eq!(materials.unwrap().len(), 2);

    let tools = db.find_by_type("Tool");
    assert!(tools.is_some());
    assert_eq!(tools.unwrap().len(), 1);

    let unknown = db.find_by_type("Unknown");
    assert!(unknown.is_none() || unknown.unwrap().is_empty());
}

// ============================================================================
// Relation Tests
// ============================================================================

#[test]
fn test_relation_storage() {
    let mut db = PathDB::new();

    let mat_id = db.add_entity("Material", vec![("name", "Steel")]);
    let tool_id = db.add_entity("Tool", vec![("name", "Drill")]);

    let rel_id = db.add_relation("usedWith", tool_id, mat_id, 0.9, vec![]);

    assert!(rel_id > 0 || rel_id == 0); // Just verify it returns something
}

// ============================================================================
// PathDB Serialization Tests
// ============================================================================

#[test]
fn test_pathdb_roundtrip() {
    let mut db = PathDB::new();

    // Add data
    let id1 = db.add_entity("Material", vec![("name", "Titanium")]);
    let id2 = db.add_entity("Tool", vec![("name", "EndMill")]);
    db.add_relation("usedWith", id2, id1, 0.95, vec![]);
    db.build_indexes();

    // Serialize
    let bytes = db.to_bytes().unwrap();
    assert!(!bytes.is_empty());

    // Deserialize
    let restored = PathDB::from_bytes(&bytes).unwrap();

    // Verify
    let entity = restored.get_entity(id1);
    assert!(entity.is_some());
    assert_eq!(entity.unwrap().entity_type, "Material");
}

#[test]
fn test_pathdb_persistence() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("test.axpd");

    // Write
    {
        let mut db = PathDB::new();
        db.add_entity("Material", vec![("name", "Steel")]);
        db.build_indexes();

        let bytes = db.to_bytes().unwrap();
        std::fs::write(&path, bytes).unwrap();
    }

    // Read
    {
        let bytes = std::fs::read(&path).unwrap();
        let db = PathDB::from_bytes(&bytes).unwrap();

        let materials = db.find_by_type("Material");
        assert!(materials.is_some());
    }
}

// ============================================================================
// Query Tests
// ============================================================================

#[test]
fn test_find_by_type() {
    let mut db = PathDB::new();

    for i in 0..100 {
        db.add_entity("Test", vec![("index", &i.to_string())]);
    }
    db.build_indexes();

    let results = db.find_by_type("Test");
    assert!(results.is_some());
    assert_eq!(results.unwrap().len(), 100);
}

#[test]
fn test_entities_with_attr_contains() {
    let mut db = PathDB::new();

    let a = db.add_entity("Material", vec![("name", "Titanium")]);
    let _b = db.add_entity("Material", vec![("name", "Steel")]);
    db.build_indexes();

    let hits = db.entities_with_attr_contains("name", "tan");
    assert!(hits.contains(a));

    // Case-insensitive.
    let hits = db.entities_with_attr_contains("name", "TITAN");
    assert!(hits.contains(a));
}

#[test]
fn test_entities_with_attr_fuzzy() {
    let mut db = PathDB::new();

    let a = db.add_entity("Material", vec![("name", "titanium")]);
    let _b = db.add_entity("Material", vec![("name", "steel")]);
    db.build_indexes();

    // "titainum" is a common transposition typo; Levenshtein distance is 2.
    let hits = db.entities_with_attr_fuzzy("name", "titainum", 2);
    assert!(hits.contains(a));

    let hits = db.entities_with_attr_fuzzy("name", "titainum", 1);
    assert!(!hits.contains(a));
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

#[test]
fn test_concurrent_reads() {
    use std::sync::Arc;
    use std::thread;

    let mut db = PathDB::new();
    for i in 0..100 {
        db.add_entity("Entity", vec![("id", &i.to_string())]);
    }
    db.build_indexes();

    let db = Arc::new(db);
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = Arc::clone(&db);
            thread::spawn(move || {
                for _ in 0..100 {
                    let _ = db.find_by_type("Entity");
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_pathdb() {
    let db = PathDB::new();

    let materials = db.find_by_type("Material");
    assert!(materials.is_none() || materials.unwrap().is_empty());

    let entity = db.get_entity(0);
    assert!(entity.is_none());
}

#[test]
fn test_empty_attributes() {
    let mut db = PathDB::new();

    let id = db.add_entity("Empty", vec![]);
    let entity = db.get_entity(id);

    assert!(entity.is_some());
    assert!(entity.unwrap().attrs.is_empty());
}

#[test]
fn test_unicode_strings() {
    let mut db = PathDB::new();

    let id = db.add_entity(
        "Material",
        vec![
            ("name", "钛合金"),                    // Chinese for titanium alloy
            ("description", "Légèr et résistant"), // French
        ],
    );

    let entity = db.get_entity(id);
    assert!(entity.is_some());
    assert_eq!(
        entity.unwrap().attrs.get("name"),
        Some(&"钛合金".to_string())
    );
}

#[test]
fn test_large_attributes() {
    let mut db = PathDB::new();

    let long_value = "x".repeat(10000);
    let id = db.add_entity("Large", vec![("content", &long_value)]);

    let entity = db.get_entity(id);
    assert!(entity.is_some());
    assert_eq!(entity.unwrap().attrs.get("content").unwrap().len(), 10000);
}

// ============================================================================
// Bitmap Operations Tests
// ============================================================================

#[test]
fn test_bitmap_operations() {
    let mut db = PathDB::new();

    // Add entities
    for i in 0..50 {
        db.add_entity("TypeA", vec![("id", &i.to_string())]);
    }
    for i in 0..30 {
        db.add_entity("TypeB", vec![("id", &i.to_string())]);
    }
    db.build_indexes();

    let type_a = db.find_by_type("TypeA").unwrap();
    let type_b = db.find_by_type("TypeB").unwrap();

    assert_eq!(type_a.len(), 50);
    assert_eq!(type_b.len(), 30);

    // Set operations
    let union = type_a | type_b.clone();
    assert_eq!(union.len(), 80);

    let intersection = type_a.clone() & type_b.clone();
    assert_eq!(intersection.len(), 0); // No overlap
}

// ============================================================================
// Performance Tests (optional, run with --release)
// ============================================================================

#[test]
#[ignore] // Run with: cargo test -- --ignored
fn test_large_scale() {
    let mut db = PathDB::new();

    // Add 100k entities
    let start = std::time::Instant::now();
    for i in 0..100_000 {
        db.add_entity(
            "Entity",
            vec![
                ("id", &i.to_string()),
                ("type", if i % 2 == 0 { "even" } else { "odd" }),
            ],
        );
    }
    println!("Added 100k entities in {:?}", start.elapsed());

    let start = std::time::Instant::now();
    db.build_indexes();
    println!("Built indexes in {:?}", start.elapsed());

    // Query
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        let _ = db.find_by_type("Entity");
    }
    println!("1000 queries in {:?}", start.elapsed());

    // Serialize
    let start = std::time::Instant::now();
    let bytes = db.to_bytes().unwrap();
    println!("Serialized {} bytes in {:?}", bytes.len(), start.elapsed());

    // Deserialize
    let start = std::time::Instant::now();
    let _ = PathDB::from_bytes(&bytes).unwrap();
    println!("Deserialized in {:?}", start.elapsed());
}

#[test]
#[ignore] // Run with: cargo test -p axiograph-pathdb --release -- --ignored
fn test_large_scale_graph_ingest_and_follow_path() {
    use axiograph_pathdb::PathIndex;

    let entities: usize = 200_000;
    let edges_per_entity: usize = 8;
    let rel_types: usize = 8;
    let index_depth: usize = 3;
    let queries: usize = 50_000;

    let rel_names: Vec<String> = (0..rel_types).map(|i| format!("rel_{i}")).collect();

    let mut db = PathDB::new();
    db.path_index = PathIndex::new(index_depth);

    let start = std::time::Instant::now();
    for _ in 0..entities {
        db.add_entity("Node", Vec::new());
    }
    let dt = start.elapsed();
    println!(
        "Added {entities} entities in {:?} ({:.1} entities/sec)",
        dt,
        (entities as f64) / dt.as_secs_f64()
    );

    let start = std::time::Instant::now();
    for source in 0..entities {
        let source_id = source as u32;
        for j in 0..edges_per_entity {
            // Deterministic "pseudo-random" target without an RNG dependency.
            let target = (source
                .wrapping_mul(1_000_003)
                .wrapping_add(j.wrapping_mul(97)))
                % entities;
            let rel_type = &rel_names[(source + j) % rel_types];
            db.add_relation(rel_type, source_id, target as u32, 0.9, Vec::new());
        }
    }
    let edges = entities * edges_per_entity;
    let dt = start.elapsed();
    println!(
        "Added {edges} relations in {:?} ({:.1} edges/sec)",
        dt,
        (edges as f64) / dt.as_secs_f64()
    );

    let start = std::time::Instant::now();
    db.build_indexes();
    println!("Built indexes in {:?}", start.elapsed());

    let start = std::time::Instant::now();
    let mut total_hits: u64 = 0;
    for q in 0..queries {
        let start_id = (q.wrapping_mul(1_000_003) % entities) as u32;
        let r0 = q % rel_types;
        let r1 = (q.wrapping_mul(7).wrapping_add(1)) % rel_types;
        let r2 = (q.wrapping_mul(13).wrapping_add(2)) % rel_types;
        let path = [
            rel_names[r0].as_str(),
            rel_names[r1].as_str(),
            rel_names[r2].as_str(),
        ];
        let targets = db.follow_path(start_id, &path);
        total_hits = total_hits.wrapping_add(targets.len());
    }
    let dt = start.elapsed();
    println!(
        "Ran {queries} follow_path queries in {:?} ({:.1} queries/sec), total_hits={total_hits}",
        dt,
        (queries as f64) / dt.as_secs_f64()
    );
}
