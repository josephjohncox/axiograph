# Unified Storage Layer

**Diataxis:** Explanation  
**Audience:** contributors

> NOTE (Rust+Lean release): `.axi` is canonical and `.axpd` is derived. Certificate checking is done in Lean.
> Any Idris references below are historical notes from an earlier prototype and should be ported/updated to Lean.

## Overview

Axiograph's Unified Storage ensures all knowledge—whether from LLM extraction, user edits, or file imports—lands in **both formats**:

1. **`.axi` files** — Human-readable, version-controllable, git-friendly
2. **PathDB** — Binary indexed format for fast queries

This is not eventual consistency—writes are **atomic** across both formats.

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────┐
│                        UNIFIED STORAGE LAYER                             │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Sources                    Storage Manager              Destinations   │
│   ─────────                  ───────────────              ────────────   │
│                                                                          │
│   ┌─────────┐               ┌──────────────┐             ┌───────────┐  │
│   │   LLM   │──────────────►│              │────────────►│  .axi     │  │
│   │  Sync   │               │   Unified    │             │  files    │  │
│   └─────────┘               │   Storage    │             └───────────┘  │
│                             │   Manager    │                    │       │
│   ┌─────────┐               │              │                    │ sync  │
│   │  User   │──────────────►│              │                    ▼       │
│   │  Edits  │               │              │             ┌───────────┐  │
│   └─────────┘               │              │────────────►│  PathDB   │  │
│                             │              │             │  (binary) │  │
│   ┌─────────┐               │              │             └───────────┘  │
│   │  File   │──────────────►│              │                    │       │
│   │ Import  │               └──────────────┘                    │       │
│   └─────────┘                      │                            │       │
│                                    ▼                            │       │
│   ┌─────────┐               ┌──────────────┐                    │       │
│   │   API   │──────────────►│  Changelog   │◄───────────────────┘       │
│   │  Calls  │               └──────────────┘                            │
│   └─────────┘                                                            │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

## Usage

### Basic Setup

```rust
use axiograph_storage::{UnifiedStorage, StorageConfig};

let config = StorageConfig {
    axi_dir: "./knowledge".into(),
    pathdb_path: "./knowledge.axpd".into(),
    changelog_path: "./changelog.json".into(),
    watch_files: true,  // Hot reload on external changes
    require_review: ReviewPolicy {
        constraints: true,           // Constraints need approval
        low_confidence_threshold: Some(0.7),
        schema_changes: true,
    },
    max_pending: 100,
};

let storage = UnifiedStorage::new(config)?;
```

### Adding Facts

```rust
use axiograph_storage::{StorableFact, ChangeSource};

// From any source
let facts = vec![
    StorableFact::Entity {
        name: "Titanium".to_string(),
        entity_type: "Material".to_string(),
        attributes: vec![
            ("hardness".to_string(), "36".to_string()),
        ],
    },
    StorableFact::TacitKnowledge {
        name: "CoolantRule".to_string(),
        rule: "cutting(Ti) -> useCoolant".to_string(),
        confidence: 0.92,
        domain: "machining".to_string(),
        source: "Expert".to_string(),
    },
];

// Add with source tracking
let change_id = storage.add_facts(
    facts,
    ChangeSource::LLMExtraction {
        session_id: uuid::Uuid::new_v4(),
        model: "claude-3".to_string(),
        confidence: 0.92,
    }
)?;

// Flush writes to both formats
storage.flush()?;
```

### What Gets Written

**`.axi` file (`llm_extracted.axi`):**
```
-- Added at 2024-01-15T10:30:00Z
-- Source: LLM extraction (model: claude-3, confidence: 0.92)

Titanium : Material {
  hardness = "36"
}

tacit "CoolantRule" {
  rule: cutting(Ti) -> useCoolant
  confidence: 0.92
  domain: "machining"
  source: "Expert"
}
```

**PathDB:**
- Entity added with ID, type index, attribute store
- Relations indexed for path traversal
- Confidence scores for probabilistic queries

### LLM Sync Integration

```rust
use axiograph_llm_sync::{SyncManager, SyncConfig, LLMProvider};

// Create sync manager with unified storage
let sync = SyncManager::new(
    Arc::new(storage),
    SyncConfig::default(),
    LLMProvider::Anthropic { model: "claude-3-opus".to_string() },
);

// Extract from conversation → lands in both formats
let result = sync.sync_from_conversation(&conversation, None).await?;

// Query back with grounded context
let context = sync.build_grounding_context("titanium cutting parameters", 10)?;
```

## StorableFact Types

| Type | Description | PathDB Storage | .axi Format |
|------|-------------|----------------|-------------|
| `Entity` | Named typed object | Entity table + type index | `name : Type { attrs }` |
| `Relation` | Link between entities | Relation table + path index | `rel(src, tgt)` |
| `Constraint` | Rule/invariant | — (interpreted) | `constraint name { ... }` |
| `TacitKnowledge` | Probabilistic rule | Special entity type | `tacit "name" { ... }` |
| `Concept` | Learning topic | Entity with `Concept` type | `concept name { ... }` |
| `SafetyGuideline` | Warning/guardrail | Entity with `SafetyGuideline` type | `guideline name { ... }` |

## Change Sources

Each change is tagged with its source for provenance:

```rust
pub enum ChangeSource {
    LLMExtraction { session_id, model, confidence },
    UserEdit { user_id },
    FileImport { path },
    API { client_id },
    System { reason },
}
```

This enables:
- Filtering changelog by source
- Separate `.axi` files per source type
- Confidence tracking for LLM-derived facts
- Rollback to pre-LLM state

## File Organization

```
./knowledge/
├── schema.axi              # User-defined schema
├── instances.axi           # User-created instances
├── llm_extracted.axi       # Auto-populated by LLM sync
├── user_edits.axi          # User corrections
├── api_additions.axi       # API-added facts
└── system_inferred.axi     # System-derived facts

./knowledge.axpd            # Binary PathDB (all data)
./changelog.json            # Full change history
```

## Sync Modes

### Immediate Flush
```rust
storage.add_facts(facts, source)?;
storage.flush()?;  // Writes NOW
```

### Batched
```rust
for batch in data.chunks(100) {
    storage.add_facts(batch.to_vec(), source)?;
}
// Auto-flushes when batch_size reached, or:
storage.flush()?;
```

### From .axi Files
```rust
// User edits .axi file externally
// If watch_files is enabled, auto-syncs to PathDB
// Or manually:
storage.sync_from_axi()?;
```

## Rollback

```rust
// Get changelog
let changes = storage.changelog();

// Find the change to roll back to
let target = changes.iter()
    .find(|c| c.timestamp < some_time)
    .map(|c| c.id);

// Rollback
if let Some(id) = target {
    storage.rollback_to(id)?;
    // Rebuilds PathDB from changelog
    // .axi files need manual revert (git)
}
```

## Review Workflow

For low-confidence or constraint changes:

```rust
// Check pending
let pending = sync.pending_review();
for fact in pending {
    println!("{}: {}", fact.id, fact.claim);
}

// Approve
sync.approve_fact(fact_id)?;  // Integrates to storage

// Or reject
sync.reject_fact(fact_id, "Incorrect information")?;
```

## Conflict Resolution

```rust
let conflicts = sync.unresolved_conflicts();

for (i, conflict) in conflicts.iter().enumerate() {
    println!("Conflict: {} vs existing", conflict.new_fact.claim);
    
    // Resolve
    sync.resolve_conflict(i, Resolution::ReplaceOld)?;
    // or Resolution::KeepOld
    // or Resolution::Merge { weights: (0.7, 0.3) }
    // or Resolution::HumanReview
}
```

## Best Practices

1. **Use source tracking** — Always tag changes with their source
2. **Review thresholds** — Set `low_confidence_threshold` appropriately
3. **Batch writes** — Use batching for bulk imports
4. **Version control** — Git the `.axi` files, not the `.axpd`
5. **Periodic compaction** — Rebuild PathDB from .axi for consistency

## Performance

| Operation | .axi | PathDB |
|-----------|------|--------|
| Write | O(1) append | O(log n) index update |
| Read by type | O(n) scan | O(1) bitmap lookup |
| Path query | N/A | O(path length) |
| Full rebuild | O(n) parse | O(n) from changelog |

The unified storage provides the best of both worlds:
- **Human-friendly** .axi files for review and version control
- **Machine-efficient** PathDB for queries

## Integration Points

### Idris Verification
```idris
-- Load PathDB in Idris for type-checked queries
pathdb <- loadPathDB "./knowledge.axpd"
result <- runQuery (FindByType "Material") pathdb
```

### REST API
```bash
# Add fact via API
curl -X POST /api/facts -d '{"type":"Entity","name":"Steel"}'
# Lands in both formats via unified storage
```

### CLI
```bash
# Import file
axiograph import data.json --format=json

# Query (uses PathDB)
axiograph query "FindByType Material"

# Export to .axi
axiograph export --format=axi
```
