//! End-to-end snapshot export/import tests (`.axpd` ↔ `.axi`).
//!
//! These tests exercise the “GraphRAG → PathDB → snapshot” loop:
//! - facts land in PathDB via `UnifiedStorage`,
//! - we export PathDB into the reversible `.axi` snapshot schema (`PathDBExportV1`),
//! - import back, and ensure the snapshot representation is stable.

use axiograph_pathdb::axi_export::{export_pathdb_to_axi_v1, import_pathdb_from_axi_v1};
use axiograph_storage::{ChangeSource, StorableFact, StorageConfig, UnifiedStorage};
use tempfile::tempdir;

#[test]
fn pathdb_snapshot_export_import_roundtrip_preserves_entities_and_relations() {
    let dir = tempdir().unwrap();
    let pathdb_path = dir.path().join("test.axpd");

    let storage = UnifiedStorage::new(StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: pathdb_path.clone(),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    })
    .unwrap();

    // Important: relation endpoints in `axiograph-storage` are currently placeholder ids (0→1),
    // so we add the source and target entities first to make this deterministic.
    storage
        .add_facts(
            vec![
                StorableFact::Entity {
                    name: "Alice".to_string(),
                    entity_type: "Person".to_string(),
                    attributes: vec![
                        ("name".to_string(), "Alice".to_string()),
                        ("note".to_string(), "prefers espresso".to_string()),
                    ],
                },
                StorableFact::Entity {
                    name: "Bob".to_string(),
                    entity_type: "Person".to_string(),
                    attributes: vec![("name".to_string(), "Bob".to_string())],
                },
                StorableFact::Relation {
                    name: Some("knows_edge".to_string()),
                    rel_type: "knows".to_string(),
                    source: "Alice".to_string(),
                    target: "Bob".to_string(),
                    confidence: 0.9,
                    attributes: vec![("since".to_string(), "2024".to_string())],
                },
            ],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();
    storage.flush().unwrap();

    let bytes = std::fs::read(&pathdb_path).expect("read written axpd");
    let db = axiograph_pathdb::PathDB::from_bytes(&bytes).expect("parse axpd");

    let axi_1 = export_pathdb_to_axi_v1(&db).expect("export snapshot to axi");
    let imported = import_pathdb_from_axi_v1(&axi_1).expect("import snapshot back to PathDB");
    let axi_2 = export_pathdb_to_axi_v1(&imported).expect("re-export snapshot to axi");
    assert_eq!(
        axi_1, axi_2,
        "expected `.axi` snapshot export to be stable across a round-trip"
    );

    // Spot-check: entities and relations still function after import.
    let persons = imported
        .find_by_type("Person")
        .expect("should have Person type");
    assert_eq!(persons.len(), 2);

    let alice = imported.get_entity(0).expect("entity 0 (Alice)");
    assert_eq!(alice.entity_type, "Person");
    assert_eq!(alice.attrs.get("name"), Some(&"Alice".to_string()));
    assert_eq!(
        alice.attrs.get("note"),
        Some(&"prefers espresso".to_string())
    );

    let knows_targets = imported.follow_one(0, "knows");
    assert!(
        knows_targets.contains(1),
        "expected exported/imported relation to keep 0 -knows-> 1"
    );
}
