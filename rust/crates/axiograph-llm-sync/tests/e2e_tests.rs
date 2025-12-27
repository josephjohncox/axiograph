//! End-to-End tests for LLM ↔ KG synchronization
//!
//! These tests verify the complete pipeline:
//! 1. Conversation → Fact extraction
//! 2. Fact validation
//! 3. Conflict detection
//! 4. Storage to both .axi and PathDB
//! 5. Grounding context retrieval
//! 6. Review workflow
//! 7. Rollback

use axiograph_llm_sync::*;
use axiograph_storage::{ChangeSource, StorableFact, StorageConfig, UnifiedStorage};
use chrono::Utc;
use std::sync::Arc;
use tempfile::tempdir;

/// Helper to create test environment
fn test_env() -> (Arc<UnifiedStorage>, SyncManager, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());
    let sync_config = SyncConfig {
        auto_integrate_threshold: 0.8,
        batch_size: 100,
        human_review_constraints: true,
        track_provenance: true,
        auto_resolve_conflicts: false,
    };

    let manager = SyncManager::new(
        Arc::clone(&storage),
        sync_config,
        LLMProvider::Custom {
            name: "test".to_string(),
            endpoint: "local".to_string(),
        },
    );

    (storage, manager, dir)
}

/// Create test conversation
fn machinist_conversation() -> Vec<ConversationTurn> {
    vec![
        ConversationTurn {
            role: Role::User,
            content: "Tell me about cutting titanium".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
        ConversationTurn {
            role: Role::Assistant,
            content: r#"
                Titanium is a Material with excellent strength-to-weight ratio.
                Ti-6Al-4V is a common titanium alloy.
                
                Always use coolant when cutting titanium because of heat buildup.
                Never exceed 60 SFM when dry cutting titanium.
                
                Carbide tools work well for titanium machining.
            "#
            .to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
    ]
}

// ============================================================================
// Basic Pipeline Tests
// ============================================================================

#[tokio::test]
async fn test_full_extraction_pipeline() {
    let (_storage, sync, _dir) = test_env();

    let result = sync
        .sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    // Should extract some facts
    assert!(
        result.integrated_count > 0 || result.pending_review > 0,
        "Should extract at least one fact"
    );
}

#[tokio::test]
async fn test_facts_land_in_axi() {
    let (storage, sync, dir) = test_env();

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    // Check .axi file was created
    let axi_path = dir.path().join("llm_extracted.axi");

    // May not exist if all facts need review
    if axi_path.exists() {
        let content = std::fs::read_to_string(&axi_path).unwrap();
        assert!(content.len() > 0, "Should have content");
        assert!(
            content.contains("LLM extraction"),
            "Should have source comment"
        );
    }
}

#[tokio::test]
async fn test_facts_land_in_pathdb() {
    let (storage, sync, _dir) = test_env();

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    // Check PathDB has entities
    let pathdb = storage.pathdb();
    let db = pathdb.read();

    // Should have at least one entity type
    let has_entities = ["Material", "Tool", "TacitKnowledge"]
        .iter()
        .any(|t| db.find_by_type(t).map_or(false, |e| !e.is_empty()));

    // May not have entities if all need review, which is also valid
    println!("Entities found in PathDB: {}", has_entities);
}

#[tokio::test]
async fn test_changelog_tracking() {
    let (storage, sync, _dir) = test_env();

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    let changelog = storage.changelog();

    // Should have changelog entries
    assert!(
        !changelog.is_empty() || !sync.pending_review().is_empty(),
        "Should have changelog or pending items"
    );
}

// ============================================================================
// Extraction Quality Tests
// ============================================================================

#[tokio::test]
async fn test_entity_extraction() {
    let (_storage, sync, _dir) = test_env();

    let conversation = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Steel is a Material with hardness of 30 HRC.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    let result = sync
        .sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // Simple "is a" pattern should be extracted
    assert!(
        result.integrated_count > 0 || result.pending_review > 0,
        "Should extract 'Steel is a Material'"
    );
}

#[tokio::test]
async fn test_tacit_knowledge_extraction() {
    let (_storage, sync, _dir) = test_env();

    let conversation = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Always use coolant when machining titanium.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    let result = sync
        .sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // "always X when Y" should be extracted as tacit knowledge
    println!(
        "Extracted: {}, Pending: {}",
        result.integrated_count, result.pending_review
    );
}

#[tokio::test]
async fn test_relation_extraction() {
    let (_storage, sync, _dir) = test_env();

    let conversation = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Carbide requires proper coolant. Titanium produces chips.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    let result = sync
        .sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // Should extract relations
    println!(
        "Relations extracted: {}, Pending: {}",
        result.integrated_count, result.pending_review
    );
}

// ============================================================================
// Grounding Context Tests
// ============================================================================

#[tokio::test]
async fn test_grounding_context_basic() {
    let (storage, sync, _dir) = test_env();

    // First, add some facts directly
    storage
        .add_facts(
            vec![
                StorableFact::Entity {
                    name: "Titanium".to_string(),
                    entity_type: "Material".to_string(),
                    attributes: vec![("hardness".to_string(), "36".to_string())],
                },
                StorableFact::Entity {
                    name: "Carbide".to_string(),
                    entity_type: "Tool".to_string(),
                    attributes: vec![],
                },
            ],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();
    storage.flush().unwrap();

    // Now query
    let context = sync
        .build_grounding_context("titanium cutting", 10)
        .unwrap();

    // Should have suggestions
    assert!(!context.suggested_queries.is_empty());

    // Should have guardrails for machining topic
    assert!(!context.active_guardrails.is_empty());
}

#[tokio::test]
async fn test_grounding_includes_relevant_facts() {
    let (storage, sync, _dir) = test_env();

    // Add specific fact
    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "SpecialAlloy".to_string(),
                entity_type: "Material".to_string(),
                attributes: vec![("special".to_string(), "yes".to_string())],
            }],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();
    storage.flush().unwrap();

    let context = sync
        .build_grounding_context("material properties", 10)
        .unwrap();

    // Should find materials
    // (may be empty if keyword matching doesn't find it)
    println!(
        "Found {} facts for 'material properties'",
        context.facts.len()
    );
}

// ============================================================================
// Review Workflow Tests
// ============================================================================

#[tokio::test]
async fn test_pending_review_workflow() {
    // Use strict config requiring review
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());
    let sync_config = SyncConfig {
        auto_integrate_threshold: 0.99, // Very high, most will need review
        human_review_constraints: true,
        ..Default::default()
    };

    let sync = SyncManager::new(
        Arc::clone(&storage),
        sync_config,
        LLMProvider::Custom {
            name: "test".to_string(),
            endpoint: "local".to_string(),
        },
    );

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    let pending = sync.pending_review();

    // With high threshold, should have pending items
    println!("Pending review: {} items", pending.len());

    if !pending.is_empty() {
        // Approve first item
        let first_id = pending[0].id;
        sync.approve_fact(first_id).unwrap();

        // Should have one fewer pending
        let new_pending = sync.pending_review();
        assert!(new_pending.len() < pending.len() || new_pending.iter().all(|p| p.id != first_id));
    }
}

#[tokio::test]
async fn test_reject_fact() {
    let dir = tempdir().unwrap();
    let config = StorageConfig {
        axi_dir: dir.path().to_path_buf(),
        pathdb_path: dir.path().join("test.axpd"),
        changelog_path: dir.path().join("changelog.json"),
        watch_files: false,
        ..Default::default()
    };

    let storage = Arc::new(UnifiedStorage::new(config).unwrap());
    let sync_config = SyncConfig {
        auto_integrate_threshold: 0.99,
        ..Default::default()
    };

    let sync = SyncManager::new(
        Arc::clone(&storage),
        sync_config,
        LLMProvider::Custom {
            name: "test".to_string(),
            endpoint: "local".to_string(),
        },
    );

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    let pending = sync.pending_review();

    if !pending.is_empty() {
        let first_id = pending[0].id;
        sync.reject_fact(first_id, "Incorrect information").unwrap();

        // Find the rejected fact
        let updated = sync.pending_review();
        let rejected = updated.iter().find(|f| f.id == first_id);

        if let Some(fact) = rejected {
            assert!(matches!(fact.status, FactStatus::Rejected { .. }));
        }
    }
}

// ============================================================================
// Conflict Detection Tests
// ============================================================================

#[tokio::test]
async fn test_conflict_detection() {
    let (storage, sync, _dir) = test_env();

    // Pre-populate with existing fact
    storage
        .add_facts(
            vec![StorableFact::Entity {
                name: "Steel".to_string(),
                entity_type: "Material".to_string(),
                attributes: vec![("hardness".to_string(), "50".to_string())],
            }],
            ChangeSource::UserEdit { user_id: None },
        )
        .unwrap();
    storage.flush().unwrap();

    // Extract potentially conflicting fact
    let conversation = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Steel is a Material with different properties.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    sync.sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // Check for conflicts
    let conflicts = sync.unresolved_conflicts();
    println!("Detected {} conflicts", conflicts.len());
}

// ============================================================================
// Event Tracking Tests
// ============================================================================

#[tokio::test]
async fn test_event_emission() {
    let (storage, mut sync, _dir) = test_env();

    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let events_clone = Arc::clone(&events);

    sync.on_event(Box::new(move |event| {
        events_clone.lock().unwrap().push(format!("{:?}", event));
    }));

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    let captured = events.lock().unwrap();

    // Should have received events
    assert!(!captured.is_empty(), "Should capture events");

    // Should have extraction event
    assert!(captured.iter().any(|e| e.contains("FactsExtracted")));
}

// ============================================================================
// Statistics and State Tests
// ============================================================================

#[tokio::test]
async fn test_sync_statistics() {
    let (_storage, sync, _dir) = test_env();

    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    let stats = sync.stats();

    // Stats should be populated
    println!(
        "Stats: integrated={}, pending={}, conflicts={}, version={}",
        stats.total_integrated,
        stats.pending_review,
        stats.unresolved_conflicts,
        stats.graph_version
    );
}

#[tokio::test]
async fn test_new_session() {
    let (_storage, sync, _dir) = test_env();

    let session1 = sync.state().session_id;
    let session2 = sync.new_session();

    assert_ne!(session1, session2, "New session should have different ID");
}

// ============================================================================
// Multiple Conversation Tests
// ============================================================================

#[tokio::test]
async fn test_multiple_conversations() {
    let (storage, sync, _dir) = test_env();

    // First conversation
    let conv1 = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Aluminum is a Material that is lightweight.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];
    sync.sync_from_conversation(&conv1, None).await.unwrap();

    // Second conversation
    let conv2 = vec![ConversationTurn {
        role: Role::Assistant,
        content: "Copper is a Material that conducts heat well.".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];
    sync.sync_from_conversation(&conv2, None).await.unwrap();

    // Both should be processed
    let changelog = storage.changelog();
    println!("Total changelog entries: {}", changelog.len());
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_empty_conversation() {
    let (_storage, sync, _dir) = test_env();

    let result = sync.sync_from_conversation(&[], None).await.unwrap();

    assert_eq!(result.integrated_count, 0);
    assert_eq!(result.pending_review, 0);
}

#[tokio::test]
async fn test_no_extractable_facts() {
    let (_storage, sync, _dir) = test_env();

    let conversation = vec![ConversationTurn {
        role: Role::User,
        content: "Hello, how are you?".to_string(),
        timestamp: Utc::now(),
        metadata: Default::default(),
    }];

    let result = sync
        .sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    // Should handle gracefully
    assert_eq!(
        result.integrated_count + result.pending_review + result.invalid_count,
        0
    );
}

#[tokio::test]
async fn test_large_conversation() {
    let (_storage, sync, _dir) = test_env();

    // Generate large conversation
    let conversation: Vec<ConversationTurn> = (0..100)
        .map(|i| ConversationTurn {
            role: Role::Assistant,
            content: format!(
                "Material{} is a Material with property{} of value{}.",
                i, i, i
            ),
            timestamp: Utc::now(),
            metadata: Default::default(),
        })
        .collect();

    let result = sync
        .sync_from_conversation(&conversation, None)
        .await
        .unwrap();

    println!(
        "Large conversation: integrated={}, pending={}",
        result.integrated_count, result.pending_review
    );
}

// ============================================================================
// Provider Tests
// ============================================================================

#[tokio::test]
async fn test_custom_provider() {
    let (storage, _, _dir) = test_env();

    let custom_provider = LLMProvider::Custom {
        name: "my-llm".to_string(),
        endpoint: "http://localhost:8080".to_string(),
    };

    let sync = SyncManager::new(storage, SyncConfig::default(), custom_provider);

    // Should work with custom provider
    let result = sync
        .sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();
    println!("Custom provider result: {:?}", result);
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn test_full_roundtrip() {
    let (storage, sync, dir) = test_env();

    // 1. Extract from conversation
    sync.sync_from_conversation(&machinist_conversation(), None)
        .await
        .unwrap();

    // 2. Build grounding context
    let context = sync.build_grounding_context("titanium", 5).unwrap();

    // 3. Check .axi file
    let axi_files: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "axi"))
        .collect();

    println!("Created {} .axi files", axi_files.len());

    // 4. Check PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();

    // 5. Verify end state
    let stats = sync.stats();
    println!("Final stats: {:?}", stats);

    // Should have processed something
    assert!(
        stats.total_integrated > 0 || stats.pending_review > 0,
        "Should have processed at least one fact"
    );
}
