//! End-to-end tests for unified storage

use super::*;
use tempfile::tempdir;

/// Helper to create test storage
fn test_storage() -> (UnifiedStorage, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        require_review: ReviewPolicy {
            constraints: false,
            low_confidence_threshold: None,
            schema_changes: false,
        },
        max_pending: 100,
    };
    let storage = UnifiedStorage::new(config).unwrap();
    (storage, dir)
}

#[test]
fn test_entity_lands_in_both_formats() {
    let (storage, dir) = test_storage();

    // Add entity
    let facts = vec![StorableFact::Entity {
        name: "Titanium".to_string(),
        entity_type: "Material".to_string(),
        attributes: vec![
            ("hardness".to_string(), "36".to_string()),
            ("density".to_string(), "4.5".to_string()),
        ],
    }];

    storage
        .add_facts(
            facts,
            ChangeSource::UserEdit {
                user_id: Some("test".to_string()),
            },
        )
        .unwrap();
    storage.flush().unwrap();

    // Verify .axi file
    let axi_path = dir.path().join("user_edits.axi");
    assert!(axi_path.exists(), ".axi file should exist");
    let axi_content = std::fs::read_to_string(&axi_path).unwrap();
    assert!(
        axi_content.contains("Titanium"),
        "Should contain entity name"
    );
    assert!(
        axi_content.contains("Material"),
        "Should contain entity type"
    );
    assert!(axi_content.contains("hardness"), "Should contain attribute");

    // Verify PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    let materials = db.find_by_type("Material");
    assert!(materials.is_some(), "Should find Materials in PathDB");
    assert!(
        !materials.unwrap().is_empty(),
        "Should have at least one Material"
    );
}

#[test]
fn test_relation_lands_in_both_formats() {
    let (storage, dir) = test_storage();

    let facts = vec![
        StorableFact::Entity {
            name: "Ti6Al4V".to_string(),
            entity_type: "Material".to_string(),
            attributes: vec![],
        },
        StorableFact::Entity {
            name: "EndMill".to_string(),
            entity_type: "Tool".to_string(),
            attributes: vec![],
        },
        StorableFact::Relation {
            name: Some("recommended_for".to_string()),
            rel_type: "usedWith".to_string(),
            source: "EndMill".to_string(),
            target: "Ti6Al4V".to_string(),
            confidence: 0.9,
            attributes: vec![],
        },
    ];

    storage
        .add_facts(
            facts,
            ChangeSource::API {
                client_id: "test".to_string(),
            },
        )
        .unwrap();
    storage.flush().unwrap();

    // Verify .axi
    let axi_path = dir.path().join("api_additions.axi");
    let content = std::fs::read_to_string(&axi_path).unwrap();
    assert!(content.contains("usedWith"), "Should contain relation type");
    assert!(content.contains("EndMill"), "Should contain source");
    assert!(content.contains("Ti6Al4V"), "Should contain target");
}

#[test]
fn test_tacit_knowledge_storage() {
    let (storage, _dir) = test_storage();

    let facts = vec![StorableFact::TacitKnowledge {
        name: "CoolantRule".to_string(),
        rule: "cutting(Ti) -> useCoolant".to_string(),
        confidence: 0.92,
        domain: "machining".to_string(),
        source: "Expert machinist".to_string(),
    }];

    storage
        .add_facts(
            facts,
            ChangeSource::LLMExtraction {
                session_id: uuid::Uuid::new_v4(),
                model: "test-model".to_string(),
                confidence: 0.92,
            },
        )
        .unwrap();
    storage.flush().unwrap();

    // Verify in PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    let tacit = db.find_by_type("TacitKnowledge");
    assert!(tacit.is_some());
}

#[test]
fn test_constraint_storage() {
    let (storage, dir) = test_storage();

    let facts = vec![StorableFact::Constraint {
        name: "SpeedLimit".to_string(),
        condition: "speed <= 60".to_string(),
        severity: "error".to_string(),
        message: Some("Speed too high for titanium".to_string()),
    }];

    storage
        .add_facts(facts, ChangeSource::UserEdit { user_id: None })
        .unwrap();
    storage.flush().unwrap();

    // Constraints go to .axi only
    let axi_path = dir.path().join("user_edits.axi");
    let content = std::fs::read_to_string(&axi_path).unwrap();
    assert!(content.contains("constraint"));
    assert!(content.contains("SpeedLimit"));
    assert!(content.contains("speed <= 60"));
}

#[test]
fn test_changelog_persistence() {
    let (storage, dir) = test_storage();

    // Add multiple batches
    for i in 0..3 {
        storage
            .add_facts(
                vec![StorableFact::Entity {
                    name: format!("Entity{}", i),
                    entity_type: "Test".to_string(),
                    attributes: vec![],
                }],
                ChangeSource::System {
                    reason: format!("test batch {}", i),
                },
            )
            .unwrap();
        storage.flush().unwrap();
    }

    // Verify changelog
    let changelog_path = dir.path().join("changelog.json");
    assert!(changelog_path.exists());
    let changelog_content = std::fs::read_to_string(&changelog_path).unwrap();
    let changelog: Vec<Change> = serde_json::from_str(&changelog_content).unwrap();
    assert_eq!(changelog.len(), 3, "Should have 3 changes");

    // Verify all marked as Applied
    for change in &changelog {
        assert!(matches!(change.status, ChangeStatus::Applied));
    }
}

#[test]
fn test_source_segregation() {
    let (storage, dir) = test_storage();

    // Add from different sources
    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "LLMEntity".to_string(),
                entity_type: "Test".to_string(),
                attributes: vec![],
            }],
            ChangeSource::LLMExtraction {
                session_id: uuid::Uuid::new_v4(),
                model: "test".to_string(),
                confidence: 0.9,
            },
        )
        .unwrap();

    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "UserEntity".to_string(),
                entity_type: "Test".to_string(),
                attributes: vec![],
            }],
            ChangeSource::UserEdit {
                user_id: Some("user1".to_string()),
            },
        )
        .unwrap();

    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "APIEntity".to_string(),
                entity_type: "Test".to_string(),
                attributes: vec![],
            }],
            ChangeSource::API {
                client_id: "api-client".to_string(),
            },
        )
        .unwrap();

    storage.flush().unwrap();

    // Verify separate .axi files
    assert!(dir.path().join("llm_extracted.axi").exists());
    assert!(dir.path().join("user_edits.axi").exists());
    assert!(dir.path().join("api_additions.axi").exists());

    // Verify content separation
    let llm_content = std::fs::read_to_string(dir.path().join("llm_extracted.axi")).unwrap();
    assert!(llm_content.contains("LLMEntity"));
    assert!(!llm_content.contains("UserEntity"));

    let user_content = std::fs::read_to_string(dir.path().join("user_edits.axi")).unwrap();
    assert!(user_content.contains("UserEntity"));
    assert!(!user_content.contains("LLMEntity"));
}

#[test]
fn test_batch_operations() {
    let (storage, _dir) = test_storage();

    // Add many facts in batch
    let facts: Vec<StorableFact> = (0..50)
        .map(|i| StorableFact::Entity {
            name: format!("BatchEntity{}", i),
            entity_type: "BatchTest".to_string(),
            attributes: vec![("index".to_string(), i.to_string())],
        })
        .collect();

    storage
        .add_facts(
            facts,
            ChangeSource::System {
                reason: "batch test".to_string(),
            },
        )
        .unwrap();
    storage.flush().unwrap();

    // Verify all in PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    let entities = db.find_by_type("BatchTest");
    assert!(entities.is_some());
    assert_eq!(entities.unwrap().len(), 50);
}

#[test]
fn test_pending_and_flush() {
    let (storage, _dir) = test_storage();

    // Add without flush
    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "Pending1".to_string(),
                entity_type: "Test".to_string(),
                attributes: vec![],
            }],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();

    // Check pending
    let pending = storage.pending();
    assert_eq!(pending.len(), 1);

    // Flush
    let results = storage.flush().unwrap();
    assert_eq!(results.len(), 1);

    // Pending should be empty
    assert!(storage.pending().is_empty());

    // Changelog should have entry
    assert_eq!(storage.changelog().len(), 1);
}

#[test]
fn test_pathdb_persistence() {
    let dir = tempdir().unwrap();
    let pathdb_path = dir.path().join("persistent.axpd");

    // Create and populate
    {
        let config = StorageConfig {
            axi_dir: dir.path().to_path_buf(),
            pathdb_path: pathdb_path.clone(),
            changelog_path: dir.path().join("changelog.json"),
            watch_files: false,
            ..Default::default()
        };
        let storage = UnifiedStorage::new(config).unwrap();

        storage
            .add_facts(
                vec![StorableFact::Entity {
                    name: "Persistent".to_string(),
                    entity_type: "Test".to_string(),
                    attributes: vec![],
                }],
                ChangeSource::UserEdit { user_id: None },
            )
            .unwrap();
        storage.flush().unwrap();
    }

    // Verify file exists
    assert!(pathdb_path.exists());

    // Reload and verify
    {
        let config = StorageConfig {
            axi_dir: dir.path().to_path_buf(),
            pathdb_path: pathdb_path.clone(),
            changelog_path: dir.path().join("changelog.json"),
            watch_files: false,
            ..Default::default()
        };
        let storage = UnifiedStorage::new(config).unwrap();

        let pathdb = storage.pathdb();
        let db = pathdb.read();
        let entities = db.find_by_type("Test");
        assert!(entities.is_some());
        assert!(!entities.unwrap().is_empty());
    }
}

#[test]
fn test_concept_and_guideline_storage() {
    let (storage, dir) = test_storage();

    let facts = vec![
        StorableFact::Concept {
            name: "ChipFormation".to_string(),
            description: "The process of metal removal during cutting".to_string(),
            difficulty: "intermediate".to_string(),
            prerequisites: vec!["MaterialScience".to_string(), "Mechanics".to_string()],
        },
        StorableFact::SafetyGuideline {
            name: "CoolantRequired".to_string(),
            title: "Always Use Coolant for Titanium".to_string(),
            severity: "warning".to_string(),
            explanation: "Titanium has poor thermal conductivity...".to_string(),
        },
    ];

    storage
        .add_facts(facts, ChangeSource::UserEdit { user_id: None })
        .unwrap();
    storage.flush().unwrap();

    // Verify in PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    assert!(db.find_by_type("Concept").is_some());
    assert!(db.find_by_type("SafetyGuideline").is_some());

    // Verify in .axi
    let content = std::fs::read_to_string(dir.path().join("user_edits.axi")).unwrap();
    assert!(content.contains("concept ChipFormation"));
    assert!(content.contains("guideline CoolantRequired"));
}
