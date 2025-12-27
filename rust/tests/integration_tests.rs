//! Integration tests for the complete Axiograph pipeline
//!
//! These tests verify end-to-end functionality across crates:
//! - DSL parsing → Compiler → Idris output
//! - Storage → PathDB → Query
//! - LLM Sync → Storage → Both formats
//!
//! Run with: cargo test --test integration_tests

use std::sync::Arc;
use tempfile::tempdir;

// ============================================================================
// `.axi` parsing (canonical: `axi_v1`)
// ============================================================================

#[test]
fn test_axi_v1_parse_schema_module_minimal() {
    use axiograph_dsl::axi_v1::parse_axi_v1;

    let source = r#"
        module TestSchema
        
        schema S:
          object Material
          object Tool
          relation usedWith(tool: Tool, material: Material)

        instance I of S:
          Material = { Ti6Al4V }
          Tool = { CarbideEndMill }
          usedWith = { (tool=CarbideEndMill, material=Ti6Al4V) }
    "#;

    let module = parse_axi_v1(source).expect("should parse");
    assert_eq!(module.module_name, "TestSchema");
    assert_eq!(module.schemas.len(), 1);
    assert_eq!(module.instances.len(), 1);
}

#[test]
fn test_axi_v1_parse_schema_module() {
    use axiograph_dsl::axi_v1::parse_axi_v1;

    let source = r#"
        module Tiny

        schema S:
          object A
          object B
          relation R(a: A, b: B)
    "#;

    let module = parse_axi_v1(source).expect("should parse");
    assert_eq!(module.module_name, "Tiny");
    assert_eq!(module.schemas.len(), 1);
    assert_eq!(module.schemas[0].objects.len(), 2);
    assert_eq!(module.schemas[0].relations.len(), 1);
}

// ============================================================================
// Storage → PathDB Integration
// ============================================================================

#[test]
fn test_storage_pathdb_sync() {
    use axiograph_storage::{ChangeSource, StorableFact, StorageConfig, UnifiedStorage};

    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = UnifiedStorage::new(config).unwrap();

    // Add various fact types
    let facts = vec![
        StorableFact::Entity {
            name: "Ti6Al4V".to_string(),
            entity_type: "Material".to_string(),
            attributes: vec![
                ("hardness".to_string(), "36".to_string()),
                ("tensile_strength".to_string(), "950".to_string()),
            ],
        },
        StorableFact::Entity {
            name: "CarbideEndMill".to_string(),
            entity_type: "Tool".to_string(),
            attributes: vec![("diameter".to_string(), "10".to_string())],
        },
        StorableFact::Relation {
            name: Some("recommends".to_string()),
            rel_type: "usedWith".to_string(),
            source: "CarbideEndMill".to_string(),
            target: "Ti6Al4V".to_string(),
            confidence: 0.9,
            attributes: vec![],
        },
        StorableFact::TacitKnowledge {
            name: "CoolantRule".to_string(),
            rule: "cutting(Ti) -> useCoolant".to_string(),
            confidence: 0.95,
            domain: "machining".to_string(),
            source: "Expert".to_string(),
        },
    ];

    storage
        .add_facts(facts, ChangeSource::UserEdit { user_id: None })
        .unwrap();
    storage.flush().unwrap();

    // Verify in PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();

    assert!(db.find_by_type("Material").is_some());
    assert!(db.find_by_type("Tool").is_some());
    assert!(db.find_by_type("TacitKnowledge").is_some());

    // Verify .axi file
    let axi_path = dir.path().join("user_edits.axi");
    assert!(axi_path.exists());
    let content = std::fs::read_to_string(axi_path).unwrap();
    assert!(content.contains("Ti6Al4V"));
    assert!(content.contains("CarbideEndMill"));
    assert!(content.contains("usedWith"));

    // Verify changelog
    let changelog = storage.changelog();
    assert_eq!(changelog.len(), 1);
}

// ============================================================================
// PathDB Query Tests
// ============================================================================

#[test]
fn test_pathdb_queries() {
    use axiograph_pathdb::PathDB;

    let mut db = PathDB::new();

    // Add entities
    let mat_id = db.add_entity("Material", vec![("name", "Steel"), ("hardness", "50")]);
    let tool_id = db.add_entity("Tool", vec![("name", "Drill"), ("diameter", "8")]);

    // Add relation
    db.add_relation("usedWith", tool_id, mat_id, 0.9, vec![]);

    // Build indexes
    db.build_indexes();

    // Query by type
    let materials = db.find_by_type("Material");
    assert!(materials.is_some());
    assert!(materials.unwrap().contains(mat_id));

    let tools = db.find_by_type("Tool");
    assert!(tools.is_some());
    assert!(tools.unwrap().contains(tool_id));

    // Get entity
    let entity = db.get_entity(mat_id);
    assert!(entity.is_some());
    assert_eq!(entity.unwrap().entity_type, "Material");
}

// ============================================================================
// LLM Sync → Storage Integration
// ============================================================================

#[tokio::test]
async fn test_llm_sync_storage_integration() {
    use axiograph_llm_sync::{ConversationTurn, LLMProvider, Role, SyncConfig, SyncManager};
    use axiograph_storage::{StorageConfig, UnifiedStorage};
    use chrono::Utc;

    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());
    let sync = SyncManager::new(
        Arc::clone(&storage),
        SyncConfig {
            auto_integrate_threshold: 0.7,
            ..Default::default()
        },
        LLMProvider::Custom {
            name: "test".to_string(),
            endpoint: "local".to_string(),
        },
    );

    // Machinist conversation
    let conversation = vec![ConversationTurn {
        role: Role::Assistant,
        content: r#"
                Titanium is a Material known for strength.
                Always use coolant when cutting titanium.
                Carbide tools are recommended for titanium.
            "#
        .to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    // Sync
    let result = sync
        .sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // Should have processed facts
    let total = result.integrated_count + result.pending_review;
    assert!(total > 0, "Should extract facts from conversation");

    // Check grounding
    let context = sync
        .build_grounding_context("titanium cutting", 10)
        .unwrap();
    assert!(!context.suggested_queries.is_empty());

    // Stats
    let stats = sync.stats();
    println!("Integration test stats: {:?}", stats);
}

// ============================================================================
// Full Pipeline Test
// ============================================================================

#[tokio::test]
async fn test_complete_pipeline() {
    use axiograph_dsl::axi_v1::parse_axi_v1;
    use axiograph_llm_sync::{ConversationTurn, LLMProvider, Role, SyncConfig, SyncManager};
    use axiograph_storage::{ChangeSource, StorableFact, StorageConfig, UnifiedStorage};
    use chrono::Utc;
    use serde_json;

    let dir = tempdir().unwrap();

    // Step 1: Parse initial schema
    let schema_source = r#"
        module MachiningKnowledge
        
        schema S:
          object Material
          object Tool
          relation usedWith(tool: Tool, material: Material)
    "#;

    let module = parse_axi_v1(schema_source).unwrap();
    let module_name = module.module_name.clone();
    assert_eq!(module_name, "MachiningKnowledge");

    // Step 2: Create storage
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("machining.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());

    // Step 3: Add initial facts
    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "Steel".to_string(),
                entity_type: "Material".to_string(),
                attributes: vec![("hardness".to_string(), "50".to_string())],
            }],
            ChangeSource::UserEdit {
                user_id: Some("admin".to_string()),
            },
        )
        .unwrap();
    storage.flush().unwrap();

    // Step 4: LLM extracts more facts
    let sync = SyncManager::new(
        Arc::clone(&storage),
        SyncConfig::default(),
        LLMProvider::Custom {
            name: "test".to_string(),
            endpoint: "local".to_string(),
        },
    );

    let conversation = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Aluminum is a Material that is soft and lightweight.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    sync.sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // Step 5: Query the combined knowledge
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    let materials = db.find_by_type("Material");
    assert!(materials.is_some());

    // Step 6: Build grounding context
    let context = sync
        .build_grounding_context("material properties", 10)
        .unwrap();
    println!("Grounding context has {} facts", context.facts.len());

    // Step 7: Serialize parsed module (interchange / debugging)
    let json = serde_json::to_string(&module).unwrap();
    assert!(json.contains("MachiningKnowledge"));

    // Step 8: Verify changelog
    let changelog = storage.changelog();
    assert!(!changelog.is_empty());

    println!("Complete pipeline test passed!");
}

// ============================================================================
// Persistence Tests
// ============================================================================

#[tokio::test]
async fn test_persistence_across_restarts() {
    use axiograph_storage::{ChangeSource, StorableFact, StorageConfig, UnifiedStorage};

    let dir = tempdir().unwrap();
    let pathdb_path = dir.path().join("persistent.axpd");
    let changelog_path = dir.path().join("changelog.json");

    // First session: add data
    {
        let config = StorageConfig {
            axi_dir: dir.path().to_path_buf(),
            pathdb_path: pathdb_path.clone(),
            changelog_path: changelog_path.clone(),
            watch_files: false,
            ..Default::default()
        };

        let storage = UnifiedStorage::new(config).unwrap();

        storage
            .add_facts(
                vec![StorableFact::Entity {
                    name: "PersistentEntity".to_string(),
                    entity_type: "Test".to_string(),
                    attributes: vec![("key".to_string(), "value".to_string())],
                }],
                ChangeSource::UserEdit { user_id: None },
            )
            .unwrap();
        storage.flush().unwrap();
    }

    // Second session: verify data persisted
    {
        let config = StorageConfig {
            axi_dir: dir.path().to_path_buf(),
            pathdb_path: pathdb_path.clone(),
            changelog_path: changelog_path.clone(),
            watch_files: false,
            ..Default::default()
        };

        let storage = UnifiedStorage::new(config).unwrap();

        // PathDB should have the entity
        let pathdb = storage.pathdb();
        let db = pathdb.read();
        let entities = db.find_by_type("Test");
        assert!(entities.is_some(), "Entity should persist");

        // Changelog should exist
        let changelog = storage.changelog();
        assert!(!changelog.is_empty(), "Changelog should persist");
    }
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_writes() {
    use axiograph_storage::{ChangeSource, StorableFact, StorageConfig, UnifiedStorage};
    use tokio::task::JoinSet;

    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("concurrent.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());

    // Spawn multiple tasks
    let mut set = JoinSet::new();

    for i in 0..10 {
        let storage_clone = Arc::clone(&storage);
        set.spawn(async move {
            storage_clone
                .add_facts(
                    vec![StorableFact::Entity {
                        name: format!("ConcurrentEntity{}", i),
                        entity_type: "Test".to_string(),
                        attributes: vec![],
                    }],
                    ChangeSource::System {
                        reason: format!("task {}", i),
                    },
                )
                .unwrap();
        });
    }

    // Wait for all
    while let Some(result) = set.join_next().await {
        result.unwrap();
    }

    // Flush all
    storage.flush().unwrap();

    // Verify all entities
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    let entities = db.find_by_type("Test");
    assert!(entities.is_some());
    assert_eq!(entities.unwrap().len(), 10);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_invalid_axi_parsing() {
    use axiograph_dsl::axi_v1::parse_axi_v1;

    let invalid = "this is not valid axi syntax }{}{";
    let result = parse_axi_v1(invalid);

    // Should return error, not panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_sync() {
    use axiograph_llm_sync::{LLMProvider, SyncConfig, SyncManager};
    use axiograph_storage::{StorageConfig, UnifiedStorage};

    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());
    let sync = SyncManager::new(
        storage,
        SyncConfig::default(),
        LLMProvider::Custom {
            name: "test".to_string(),
            endpoint: "local".to_string(),
        },
    );

    // Empty conversation should not panic
    let result = sync.sync_from_conversation(&[], None).await.unwrap();
    assert_eq!(result.integrated_count, 0);
}
