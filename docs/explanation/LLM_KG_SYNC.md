# LLM Evidence And Ontology Proposal Loop

**Diataxis:** Explanation  
**Audience:** contributors

## Overview

Axiograph uses LLMs as evidence extraction and planning components. LLM output
may ground answers, propose facts, suggest relations, and draft schema/theory
deltas, but it does not directly mutate accepted ontology state.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    EVIDENCE / PROPOSAL LOOP                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│     LLM                 TOOLING ENGINE              AXIOGRAPH       │
│    ┌───┐                   ┌───────┐                   ┌───┐       │
│    │   │◄──── grounding ───┤       ├──── query ───────►│   │       │
│    │   │                   │       │                   │   │       │
│    │   │──── extraction ──►│       │◄─── typed refs ───┤   │       │
│    │   │                   │       │                   │   │       │
│    │   │◄─── diagnostics ──┤       ├──── proposals ───►│   │       │
│    └───┘                   └───────┘                   └───┘       │
│                                                                     │
│  AXIOGRAPH → LLM                LLM → EVIDENCE/PROPOSALS          │
│  • Grounding context            • Fact extraction                   │
│  • Schema information           • Entity creation                   │
│  • Guardrail warnings           • Relation proposals                │
│  • Citation support             • Schema/theory delta proposals     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Direction 1: Axiograph → LLM Grounding

Accepted `.axi`, compiled IR, typed reports, and evidence overlays provide
grounding context to LLM generation:

### Grounding Context

```rust
GroundingContext {
    facts: vec![
        GroundedFact {
            id: 42,
            natural: "Titanium requires cutting speeds under 60 m/min",
            structured: "{type: Rule, domain: machining}",
            confidence: 0.95,
            citation: vec!["machining", "titanium", "speed_limit"],
        },
        // ... more facts
    ],
    schema_context: Some(SchemaContext { ... }),
    active_guardrails: vec![ ... ],
    suggested_queries: vec![
        "What tools are recommended for titanium?",
        "What are the risks of high-speed titanium cutting?",
    ],
}
```

### Usage in LLM Prompt

```
You are generating a response using Axiograph facts as grounding.

GROUNDING CONTEXT:
[fact:42] Titanium requires cutting speeds under 60 m/min (confidence: 95%)
[fact:43] Carbide tools are preferred for titanium (confidence: 88%)
[fact:44] Flood coolant required for titanium machining (confidence: 92%)

ACTIVE GUARDRAILS:
- MACH-002 [Critical]: Never exceed titanium speed limits
- MACH-003 [Warning]: Always use appropriate coolant

USER QUERY: What speed should I use for milling Ti-6Al-4V?

Instructions:
1. Use grounding facts to inform response
2. Cite facts using [fact:ID] notation
3. Respect all guardrails
4. If unsure, suggest queries for more information
```

### Grounded Response

```
For milling Ti-6Al-4V, you should use cutting speeds between 30-60 m/min [fact:42].

Key considerations:
- Use carbide or ceramic inserts [fact:43]
- Maintain flood coolant throughout the operation [fact:44]
- Start at the lower end (30-40 m/min) for roughing

⚠️ CRITICAL: Never exceed 60 m/min to prevent tool failure and work hardening.
```

## Direction 2: LLM → Evidence/Proposal Generation

LLM conversations generate evidence and proposed ontology deltas:

### Fact Extraction Pipeline

```
Conversation → Pattern Extraction → LLM Extraction → Runtime Validation → Review/Promotion
                    ↓                     ↓              ↓            ↓
              Simple patterns      Complex facts    Schema check   Proposal overlay
              "X is a Y"           Tacit knowledge  Conflict check Review branch
              "X causes Y"         Multi-entity     Human review   Provenance
```

### Extraction Example

**Input Conversation:**
```
User: I've been machining titanium for 20 years. Never use high speeds.
      The heat stays in the cut zone because titanium doesn't conduct
      heat well. I always keep it under 50 m/min to be safe.
```

**Extracted Facts:**
```json
{
  "facts": [
    {
      "type": "tacit_knowledge",
      "claim": "Never use high speeds with titanium",
      "structured": {
        "rule": "hasMaterial(op, Ti) -> preferLowSpeed(op)",
        "confidence": 0.95,
        "domain": "machining"
      },
      "source": {
        "provider": "conversation",
        "turn": 1,
        "experience": "20 years"
      },
      "status": "validated"
    },
    {
      "type": "entity",
      "claim": "Titanium has low thermal conductivity",
      "structured": {
        "entity_type": "MaterialProperty",
        "name": "TitaniumThermalConductivity",
        "attributes": {
          "value": "low",
          "effect": "heat concentration in cut zone"
        }
      },
      "confidence": 0.88,
      "status": "validated"
    },
    {
      "type": "constraint",
      "claim": "Keep titanium cutting speed under 50 m/min",
      "structured": {
        "name": "TitaniumSpeedLimit",
        "condition": "cuttingSpeed <= 50",
        "severity": "Warning"
      },
      "confidence": 0.85,
      "status": "needs_review"
    }
  ]
}
```

### Proposal Flow

```rust
// Process a conversation
let stats = sync_manager.process_conversation(&conversation).await?;

// Or propose individual facts
let status = sync_manager.propose_fact(extracted_fact).await?;

match status {
    FactStatus::Integrated { entity_ids } => {
        println!("Evidence materialized for review as entities: {:?}", entity_ids);
    }
    FactStatus::Validated => {
        println!("Fact validated, pending review/promotion");
    }
    FactStatus::Conflicting { conflicts_with } => {
        println!("Conflict detected with existing facts");
    }
    FactStatus::NeedsReview { reason } => {
        println!("Human review required: {}", reason);
    }
}
```

## Conflict Resolution

When new facts conflict with existing knowledge:

### Conflict Types

| Type | Description | Default Resolution |
|------|-------------|-------------------|
| `Contradiction` | Directly opposing facts | Human review |
| `AttributeMismatch` | Same entity, different values | Merge with weights |
| `ConfidenceConflict` | Same fact, different confidence | Keep higher confidence |
| `SchemaViolation` | Doesn't fit schema | Reject or extend schema |

### Resolution Strategies

```rust
enum Resolution {
    ReplaceOld,                    // New fact wins
    KeepOld,                       // Existing fact wins
    Merge { weights: (f32, f32) }, // Weighted combination
    HumanReview,                   // Requires human decision
}
```

### Example Conflict

```
NEW FACT:
  "Titanium cutting speed should be under 40 m/min"
  Confidence: 0.85, Source: Junior machinist

EXISTING FACT:
  "Titanium cutting speed should be under 60 m/min"
  Confidence: 0.95, Source: Handbook + 10 senior machinists

CONFLICT TYPE: AttributeMismatch (same rule, different threshold)

SUGGESTED RESOLUTION: Merge
  - New threshold: 50 m/min (conservative)
  - Increase confidence of caution
  - Add note about variance in recommendations
```

## Tool/Message Shapes

The evidence loop uses structured JSON messages at tooling boundaries:

### Axiograph → LLM Messages

```json
// Grounding context
{
  "type": "GroundingContext",
  "request_id": "req-123",
  "context": {
    "facts": [...],
    "schema_context": {...},
    "active_guardrails": [...]
  }
}

// Query result
{
  "type": "QueryResult",
  "request_id": "req-124",
  "query": "FindByType Person",
  "results": [...],
  "confidence": 0.95
}

// Guardrail warning
{
  "type": "GuardrailWarning",
  "request_id": "req-125",
  "rule_id": "MACH-002",
  "severity": "Critical",
  "message": "Titanium speed limit exceeded"
}
```

### LLM → Evidence/Proposal Messages

```json
// Query
{
  "type": "Query",
  "request_id": "req-200",
  "query": {
    "type": "FollowPath",
    "start": "Titanium",
    "path": ["hasMaterial", "requires"]
  },
  "max_results": 10
}

// Propose evidence fact
{
  "type": "ProposeFact",
  "request_id": "req-201",
  "fact": {
    "claim": "Titanium work hardens easily",
    "structured": {...},
    "confidence": 0.88
  },
  "reasoning": "Extracted from expert conversation"
}

// Schema/theory delta proposal
{
  "type": "ProposeSchemaExtension",
  "request_id": "req-202",
  "extension": {
    "new_entity_types": [{
      "name": "WorkHardeningProperty",
      "description": "Material tendency to harden under stress"
    }]
  },
  "reasoning": "New concept not in current schema"
}
```

## Provenance Tracking

Every fact tracks its origin:

```rust
FactSource {
    session_id: Uuid,           // Conversation session
    provider: LLMProvider,      // Which LLM
    conversation_turns: [3, 4], // Which turns
    extraction_timestamp: DateTime,
    human_verified: bool,       // Has a human approved?
}
```

This enables:
- **Audit trails**: Know where evidence came from
- **Quality metrics**: Track accuracy by source
- **Reconciliation**: Quarantine or supersede bad sources
- **Trust scoring**: Weight evidence by source reliability

## Review And Version Control

Accepted ontology versioning happens through semantic VCS. Evidence/review
state can still use local checkpoints while a proposal is being prepared:

```rust
// Create checkpoint before major changes
let version = sync_manager.checkpoint();

// Make changes
sync_manager.process_conversation(&risky_conversation).await?;

// Rollback if needed
if problems_detected {
    sync_manager.rollback(version)?;
}
```

## Best Practices

### For LLM Integration

1. **Always provide grounding context** for factual queries
2. **Require citations** (`[fact:ID]`) for important claims
3. **Check guardrails** before presenting information
4. **Propose evidence facts** when user shares expertise

### For Knowledge Curation

1. **Set confidence thresholds** for advisory proposal ranking
2. **Require human review** for constraint changes
3. **Track provenance** for audit and quality
4. **Monitor conflicts** for knowledge gaps

### For Schema Evolution

1. **Propose extensions** when new concepts appear
2. **Validate against existing ontology before review**
3. **Document reasoning** for schema changes
4. **Version control** accepted schema changes through semantic VCS

## Example: Complete Flow

```rust
use axiograph_llm_sync::*;

// 1. Initialize
let pathdb = Arc::new(RwLock::new(PathDB::new()));
let sync_manager = SyncManager::new(pathdb.clone(), ...);
let grounding_engine = GroundingEngine::new(pathdb.clone(), ...);

// 2. User asks a question
let query = "How should I machine titanium?";

// 3. Build grounding context
let context = grounding_engine.build_context(query, Some("machining"));

// 4. Send to LLM with context
let prompt = PromptBuilder::new()
    .with_grounding(context)
    .build_grounded_prompt(query);

let response = llm.generate_grounded(&prompt, &context).await?;

// 5. User provides expert knowledge
let conversation = vec![
    ConversationTurn { role: Role::User, content: query, ... },
    ConversationTurn { role: Role::Assistant, content: response, ... },
    ConversationTurn { role: Role::User, 
        content: "Actually, for Ti-6Al-4V specifically, I never go above 45 m/min...", ... },
];

// 6. Extract evidence and prepare proposals
let stats = sync_manager.process_conversation(&conversation).await?;
println!("Prepared {} evidence facts for review", stats.facts_integrated);

// 7. Next query can use reviewed grounding or advisory evidence, depending on policy
let updated_context = grounding_engine.build_context("Ti-6Al-4V speed?", Some("machining"));
// Advisory evidence remains distinct from accepted ontology truth.
```

## API Reference

See `axiograph-llm-sync` crate documentation for full API:

- `SyncManager`: Orchestrates evidence extraction and review preparation
- `GroundingEngine`: Builds context for LLM
- `FactExtractor`: Extracts facts from text
- `PromptBuilder`: Constructs LLM prompts
