//! End-to-end tests for runtime evidence storage.

use super::*;
use tempfile::tempdir;

/// Helper to create test storage
fn test_storage() -> (UnifiedStorage, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
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
fn test_entity_materializes_to_evidence_record_and_pathdb_cache() {
    let (storage, _dir) = test_storage();

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
    let results = storage.flush().unwrap();

    // Generated `.axi` is an in-memory proposal, never an accepted-file append.
    let axi_content = results[0].axi_lines.join("\n");
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
fn test_relation_lands_in_query_cache_and_review_proposal() {
    let (storage, _dir) = test_storage();

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
    let results = storage.flush().unwrap();

    let content = results[0].axi_lines.join("\n");
    assert!(content.contains("usedWith"), "Should contain relation type");
    assert!(content.contains("EndMill"), "Should contain source");
    assert!(content.contains("Ti6Al4V"), "Should contain target");

    // Verify PathDB relation endpoints use resolved entity IDs, not placeholder
    // IDs from insertion order.
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    let source_ids = UnifiedStorage::entity_ids_by_storage_name(&db, "EndMill");
    let target_ids = UnifiedStorage::entity_ids_by_storage_name(&db, "Ti6Al4V");
    assert_eq!(source_ids.len(), 1);
    assert_eq!(target_ids.len(), 1);
    assert!(
        db.follow_one(source_ids[0], "usedWith")
            .contains(target_ids[0]),
        "PathDB should store EndMill -> Ti6Al4V"
    );
    assert!(
        !db.follow_one(target_ids[0], "usedWith")
            .contains(source_ids[0]),
        "PathDB should not store the old placeholder Ti6Al4V -> EndMill edge"
    );
}

#[test]
fn test_relation_with_unresolved_endpoint_fails_closed() {
    let (storage, dir) = test_storage();

    let facts = vec![
        StorableFact::Entity {
            name: "Ti6Al4V".to_string(),
            entity_type: "Material".to_string(),
            attributes: vec![],
        },
        StorableFact::Relation {
            name: Some("bad_recommendation".to_string()),
            rel_type: "usedWith".to_string(),
            source: "MissingTool".to_string(),
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

    let err = storage.flush().unwrap_err();
    let semantic = err
        .downcast_ref::<StorageSemanticError>()
        .expect("expected typed storage semantic error");
    assert!(matches!(
        semantic,
        StorageSemanticError::UnresolvedRelationEndpoint {
            relation_name,
            rel_type,
            endpoint: RelationEndpointRole::Source,
            entity_name,
        } if relation_name.as_deref() == Some("bad_recommendation")
            && rel_type == "usedWith"
            && entity_name == "MissingTool"
    ));
    assert!(
        storage.pathdb().read().relations.is_empty(),
        "failed relation should not create a placeholder PathDB edge"
    );
    assert!(
        storage.pathdb().read().entities.is_empty(),
        "failed relation should reject the full change before partial entity writes"
    );
    assert!(
        storage.changelog().is_empty(),
        "failed relation should not be recorded as applied"
    );
    assert_eq!(
        storage.pending().len(),
        1,
        "failed changes must remain pending for inspection or retry"
    );
    assert!(
        !dir.path().join("api_additions.axi").exists(),
        "failed relation should not append .axi output"
    );
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
    let (storage, _dir) = test_storage();

    let facts = vec![StorableFact::Constraint {
        name: "SpeedLimit".to_string(),
        condition: "speed <= 60".to_string(),
        severity: "error".to_string(),
        message: Some("Speed too high for titanium".to_string()),
    }];

    storage
        .add_facts(facts, ChangeSource::UserEdit { user_id: None })
        .unwrap();
    let results = storage.flush().unwrap();

    let content = results[0].axi_lines.join("\n");
    assert!(content.contains("constraint"));
    assert!(content.contains("SpeedLimit"));
    assert!(content.contains("speed <= 60"));
}

#[test]
fn test_in_memory_changelog_tracks_applied_changes() {
    let (storage, _dir) = test_storage();
    for i in 0..3 {
        storage
            .add_facts(
                vec![StorableFact::Entity {
                    name: format!("Entity{i}"),
                    entity_type: "Test".to_string(),
                    attributes: vec![],
                }],
                ChangeSource::System {
                    reason: format!("test batch {i}"),
                },
            )
            .unwrap();
        storage.flush().unwrap();
    }
    assert_eq!(storage.changelog().len(), 3);
    assert!(storage
        .changelog()
        .iter()
        .all(|change| matches!(change.status, ChangeStatus::Applied)));
}

#[test]
fn test_batch_operations() {
    let (storage, _dir) = test_storage();

    // Add many facts in batch
    let facts: Vec<StorableFact> = (0..50)
        .map(|i| StorableFact::Entity {
            name: format!("BatchEntity{i}"),
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
fn rollback_updates_log_and_rebuilds_in_memory_pathdb() {
    let (storage, _dir) = test_storage();
    let first_change = storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "Keep".to_string(),
                entity_type: "Test".to_string(),
                attributes: vec![],
            }],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();
    storage.flush().unwrap();
    storage
        .add_facts(
            vec![StorableFact::Concept {
                name: "RemoveAfterRollback".to_string(),
                description: "temporary".to_string(),
                difficulty: "easy".to_string(),
                prerequisites: vec![],
            }],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();
    storage.flush().unwrap();

    storage.rollback_to(first_change).unwrap();

    let changelog = storage.changelog();
    assert_eq!(changelog.len(), 2);
    assert!(matches!(changelog[0].status, ChangeStatus::Applied));
    assert!(matches!(changelog[1].status, ChangeStatus::Rolled { .. }));
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    assert_eq!(
        UnifiedStorage::entity_ids_by_storage_name(&db, "Keep").len(),
        1
    );
    assert!(UnifiedStorage::entity_ids_by_storage_name(&db, "RemoveAfterRollback").is_empty());
}

#[test]
fn test_concept_and_guideline_storage() {
    let (storage, _dir) = test_storage();

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
    let results = storage.flush().unwrap();

    // Verify in PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    assert!(db.find_by_type("Concept").is_some());
    assert!(db.find_by_type("SafetyGuideline").is_some());

    let content = results[0].axi_lines.join("\n");
    assert!(content.contains("concept ChipFormation"));
    assert!(content.contains("guideline CoolantRequired"));
}
