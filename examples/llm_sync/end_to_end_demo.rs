//! End-to-End Demo: LLM ↔ Knowledge Graph Sync with Unified Storage
//!
//! This example demonstrates the complete flow:
//! 1. LLM extracts knowledge from conversation
//! 2. Facts are validated and integrated
//! 3. Data lands in both .axi files AND PathDB
//! 4. LLM can query back with grounded context
//!
//! Run: cargo run --example end_to_end_demo

use axiograph_llm_sync::{
    SyncManager, SyncConfig, SyncEvent,
    UnifiedStorage, StorageConfig,
    ConversationTurn, Role, LLMProvider,
};
use std::sync::Arc;
use chrono::Utc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Axiograph LLM ↔ KG End-to-End Demo ===\n");

    // Step 1: Initialize unified storage
    println!("1. Initializing unified storage (axi + PathDB)...");
    let config = StorageConfig {
        axi_dir: "./demo_knowledge".into(),
        pathdb_path: "./demo_knowledge.axpd".into(),
        changelog_path: "./demo_changelog.json".into(),
        watch_files: false,
        ..Default::default()
    };
    
    let storage = Arc::new(UnifiedStorage::new(config)?);
    println!("   ✓ Storage initialized\n");

    // Step 2: Create sync manager with event logging
    println!("2. Creating sync manager...");
    let mut sync = SyncManager::new(
        Arc::clone(&storage),
        SyncConfig {
            auto_integrate_threshold: 0.85,
            batch_size: 50,
            human_review_constraints: true,
            track_provenance: true,
            auto_resolve_conflicts: false,
        },
        LLMProvider::Custom { 
            name: "demo".to_string(), 
            endpoint: "local".to_string() 
        },
    );

    // Add event logging
    sync.on_event(Box::new(|event| {
        match event {
            SyncEvent::FactsExtracted { count, source, .. } => {
                println!("   📥 Extracted {} facts from {}", count, source);
            }
            SyncEvent::FactsValidated { valid, invalid, needs_review } => {
                println!("   ✅ Valid: {}, ❌ Invalid: {}, 👀 Review: {}", valid, invalid, needs_review);
            }
            SyncEvent::ConflictsDetected { count, .. } => {
                println!("   ⚠️  {} conflicts detected", count);
            }
            SyncEvent::FactsIntegrated { count, axi_files, pathdb_ids } => {
                println!("   💾 Integrated {} facts:", count);
                println!("      - .axi files: {:?}", axi_files);
                println!("      - PathDB IDs: {:?}", pathdb_ids);
            }
            _ => {}
        }
    }));
    println!("   ✓ Sync manager ready\n");

    // Step 3: Simulate machinist conversation
    println!("3. Processing machinist conversation...\n");
    let conversation = vec![
        ConversationTurn {
            role: Role::User,
            content: "What do you know about cutting titanium?".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
        ConversationTurn {
            role: Role::Assistant,
            content: r#"
                Titanium is a Material with hardness of 36 HRC.
                Ti-6Al-4V is a Material with tensile strength of 950 MPa.
                
                When cutting titanium, you should always use coolant because of heat buildup.
                Never exceed 60 SFM when dry cutting titanium.
                
                The chip formation in titanium is characterized by saw-tooth patterns 
                due to adiabatic shear.
            "#.to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
        ConversationTurn {
            role: Role::User,
            content: "What about tool selection?".to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
        ConversationTurn {
            role: Role::Assistant,
            content: r#"
                Carbide is a Tool material that works well for titanium.
                Always use sharp tools when machining titanium.
                Tool wear in titanium is primarily caused by diffusion wear.
            "#.to_string(),
            timestamp: Utc::now(),
            metadata: Default::default(),
        },
    ];

    // Sync from conversation
    let result = sync.sync_from_conversation(&conversation, None).await?;
    
    println!("\n   Sync Result:");
    println!("   - Integrated: {} facts", result.integrated_count);
    println!("   - Pending review: {}", result.pending_review);
    println!("   - Conflicts: {}", result.conflicts);
    println!("   - Invalid: {}", result.invalid_count);

    // Step 4: Verify data in both formats
    println!("\n4. Verifying data in both formats...\n");

    // Check .axi file
    let axi_path = std::path::Path::new("./demo_knowledge/llm_extracted.axi");
    if axi_path.exists() {
        let contents = std::fs::read_to_string(axi_path)?;
        println!("   📄 .axi file contents (first 500 chars):");
        println!("   {}", contents.chars().take(500).collect::<String>().replace('\n', "\n   "));
    } else {
        println!("   📄 .axi file: (would be created at {})", axi_path.display());
    }

    // Check PathDB
    let pathdb = storage.pathdb();
    let db = pathdb.read();
    println!("\n   🗄️  PathDB contents:");
    if let Some(materials) = db.find_by_type("Material") {
        println!("   - Materials: {} entities", materials.len());
    }
    if let Some(tools) = db.find_by_type("Tool") {
        println!("   - Tools: {} entities", tools.len());
    }
    if let Some(tacit) = db.find_by_type("TacitKnowledge") {
        println!("   - Tacit knowledge: {} rules", tacit.len());
    }
    drop(db);

    // Step 5: Build grounding context for LLM query
    println!("\n5. Building grounding context for LLM...\n");
    
    let query = "What should I know about cutting titanium with carbide tools?";
    let context = sync.build_grounding_context(query, 10)?;
    
    println!("   Query: \"{}\"", query);
    println!("\n   Grounding Context:");
    println!("   - Facts provided: {}", context.facts.len());
    for fact in &context.facts {
        println!("     • {} (confidence: {:.0}%)", fact.natural, fact.confidence * 100.0);
    }
    
    if let Some(schema) = &context.schema_context {
        println!("   - Entity types: {:?}", schema.entity_types);
        println!("   - Relation types: {:?}", schema.relation_types);
    }
    
    println!("   - Active guardrails: {}", context.active_guardrails.len());
    for guardrail in &context.active_guardrails {
        println!("     ⚠️  [{}] {}", guardrail.severity, guardrail.description);
    }
    
    println!("   - Suggested follow-ups:");
    for suggestion in &context.suggested_queries {
        println!("     → {}", suggestion);
    }

    // Step 6: Show pending review items
    let pending = sync.pending_review();
    if !pending.is_empty() {
        println!("\n6. Items pending review:\n");
        for fact in &pending {
            println!("   [{}] {} (confidence: {:.0}%)", 
                match &fact.status {
                    axiograph_llm_sync::FactStatus::NeedsReview { reason } => reason.as_str(),
                    _ => "pending"
                },
                fact.claim,
                fact.confidence * 100.0
            );
        }
    }

    // Step 7: Show changelog
    let changelog = storage.changelog();
    println!("\n7. Change history ({} entries):\n", changelog.len());
    for (i, change) in changelog.iter().take(3).enumerate() {
        println!("   {}. {} - {} facts", 
            i + 1, 
            change.timestamp.format("%H:%M:%S"),
            change.facts.len()
        );
    }

    // Stats
    let stats = sync.stats();
    println!("\n=== Final Statistics ===");
    println!("Total integrated: {}", stats.total_integrated);
    println!("Pending review: {}", stats.pending_review);
    println!("Unresolved conflicts: {}", stats.unresolved_conflicts);
    println!("KG version: {}", stats.kg_version);

    println!("\n✅ Demo complete!");
    println!("   - .axi file: ./demo_knowledge/llm_extracted.axi");
    println!("   - PathDB: ./demo_knowledge.axpd");
    println!("   - Changelog: ./demo_changelog.json");

    Ok(())
}

