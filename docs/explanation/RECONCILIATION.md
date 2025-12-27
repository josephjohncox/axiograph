# Fact Reconciliation System

**Diataxis:** Explanation  
**Audience:** contributors

## E2E Architecture

The reconciliation system is implemented in Rust (untrusted engine). In the
certificate-first architecture, reconciliation decisions are intended to become
**certificate-backed** and **Lean-checkable** over time.

```
┌──────────────────────────────────────────────────────────────────────────┐
│                    RECONCILIATION E2E FLOW                               │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                  │
│  │   New Fact  │───►│    Rust     │───►│   Binary    │                  │
│  │  (any src)  │    │ Reconciler  │    │  (.axrc)    │                  │
│  └─────────────┘    └─────────────┘    └──────┬──────┘                  │
│                            │                   │                         │
│                            ▼                   ▼                         │
│                     ┌─────────────┐    ┌─────────────┐                  │
│                     │  Unified    │    │   Lean      │                  │
│                     │  Storage    │    │  Checker    │                  │
│                     │ (.axi+axpd) │    │ (certs)     │                  │
│                     └─────────────┘    └─────────────┘                  │
│                            │                   │                         │
│                            └───────────────────┘                         │
│                                    │                                     │
│                            ┌───────▼───────┐                            │
│                            │  Verified KG  │                            │
│                            └───────────────┘                            │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

## Binary Format (.axrc)

Rust uses a binary format for reconciliation state:

```
Header (48 bytes):
  Magic:      4 bytes  "AXRC"
  Version:    4 bytes  u32 LE
  FactCount:  4 bytes  u32 LE
  SourceCount: 4 bytes u32 LE
  ConflictCount: 4 bytes u32 LE
  Reserved:   4 bytes
  Offsets:    24 bytes (3 x u64 LE)

Sources:     Variable
Facts:       Variable
Conflicts:   Variable
```

### Certificates (planned)

For “Rust computes, Lean verifies”, reconciliation will eventually emit a
certificate that explains why a merge/replace/review decision is valid under
the formal semantics (snapshot-scoped, anchored to canonical `.axi` inputs).

## Overview

When knowledge comes from multiple sources (LLMs, users, documents, sensors), conflicts are inevitable. Axiograph provides a principled reconciliation system using:

1. **Bayesian Updates** — Update beliefs with new evidence
2. **Weighted Evidence** — Source credibility affects weight
3. **Voting** — Upvote/downvote facts
4. **Temporal Decay** — Old facts lose weight
5. **Conflict Resolution** — Automatic or human-in-the-loop

## Architecture

```
New Fact ──┬──► No Conflict ──► Direct Integration
           │
           └──► Conflict Detected
                    │
                    ├──► Same Source ──► Replace (newer wins)
                    │
                    ├──► Different Sources
                    │         │
                    │         ├──► Weight diff > threshold ──► Auto-resolve
                    │         │
                    │         ├──► Weight diff small ──► Merge
                    │         │
                    │         └──► Critical domain ──► Human Review
                    │
                    └──► Schema Conflict ──► Reject/Schema Extension
```

## Core Concepts

### Weight

Every fact has a weight ∈ [0, 1]:

```rust
pub struct Weight(f32);

impl Weight {
    pub fn new(w: f32) -> Self {
        Self(w.clamp(0.0, 1.0))
    }
    
    pub fn combine(&self, other: Weight) -> Weight {
        Weight::new(self.0 * other.0)
    }
}
```

### Bayesian Updates

Update belief given new evidence:

$$P(H|E) = \frac{P(E|H) \cdot P(H)}{P(E)}$$

For binary hypotheses:

$$P(H|E) = \frac{P(E|H) \cdot P(H)}{P(E|H) \cdot P(H) + P(E|\neg H) \cdot P(\neg H)}$$

```rust
// Update prior belief with evidence
let posterior = prior.bayesian_update(likelihood, prior_evidence);

// Binary update (simpler API)
let new_weight = engine.bayesian_update(
    fact_id,
    likelihood_if_true,   // P(E|H)
    likelihood_if_false,  // P(E|¬H)
);
```

### Source Credibility

Sources have credibility that affects their evidence weight:

```rust
pub struct SourceCredibility {
    pub source_id: String,
    pub base_credibility: Weight,      // Initial trust level
    pub domain_expertise: HashMap<String, Weight>, // Per-domain expertise
    pub track_record: TrackRecord,     // Historical accuracy
}

impl SourceCredibility {
    // Effective credibility for a domain
    pub fn credibility_for(&self, domain: &str) -> Weight {
        let domain_weight = self.domain_expertise
            .get(domain)
            .copied()
            .unwrap_or(Weight::new(0.5));
        
        self.base_credibility
            .combine(domain_weight)
            .combine(Weight::new(self.track_record.accuracy()))
    }
}
```

### Track Record

Sources build reputation over time:

```rust
pub struct TrackRecord {
    pub correct: u32,
    pub incorrect: u32,
}

impl TrackRecord {
    pub fn accuracy(&self) -> f32 {
        let total = self.correct + self.incorrect;
        if total == 0 { 0.5 } else { self.correct as f32 / total as f32 }
    }
}
```

## Conflict Types

| Type | Description | Example |
|------|-------------|---------|
| `DirectContradiction` | Mutually exclusive | "X is true" vs "X is false" |
| `AttributeMismatch` | Same entity, different attrs | Steel hardness: 50 vs 45 |
| `ConfidenceDisagreement` | Different confidence levels | 0.9 vs 0.3 |
| `SchemaViolation` | Doesn't fit schema | Unknown entity type |

## Resolution Strategies

### Replace Old
New fact wins, old is discarded.
```rust
Resolution::ReplaceOld
```

### Keep Old
Old fact wins, new is discarded.
```rust
Resolution::KeepOld
```

### Merge
Weighted combination of both facts.
```rust
Resolution::Merge { weights: (0.6, 0.4) }
```

### Human Review
Requires manual intervention.
```rust
Resolution::HumanReview
```

## Usage

### Basic Reconciliation

```rust
use axiograph_llm_sync::reconciliation::*;

// Create engine with config
let config = ReconciliationConfig {
    auto_resolve_threshold: 0.3,  // Weight diff needed for auto-resolve
    discard_threshold: 0.1,       // Below this, facts are pruned
    decay_half_life: 30.0,        // Days until weight halves
    human_review_threshold: 0.7,  // High-weight conflicts need review
    expert_override: true,        // Experts can override
    expert_domains: vec!["safety".to_string()],
};

let mut engine = ReconciliationEngine::new(config);

// Register sources
engine.register_source(SourceCredibility::new("expert_machinist", 0.95));
engine.register_source(SourceCredibility::new("llm_claude", 0.75));
engine.register_source(SourceCredibility::new("user_anonymous", 0.3));

// Reconcile a new fact
let result = engine.reconcile(new_fact);

match result.action {
    ReconciliationAction::Integrated => println!("Fact added"),
    ReconciliationAction::Merged => println!("Merged with existing"),
    ReconciliationAction::Discarded => println!("Existing fact wins"),
    ReconciliationAction::PendingReview => println!("Needs human review"),
}
```

### Voting

```rust
// Upvote a fact
let new_weight = engine.upvote(fact_id, "user123", 0.8);

// Downvote
let new_weight = engine.downvote(fact_id, "user456", 0.6);
```

### Direct Bayesian Update

```rust
// New experiment suggests fact is likely true
let posterior = engine.bayesian_update(
    fact_id,
    0.95,  // P(result | fact_true) = 95%
    0.10,  // P(result | fact_false) = 10%
);
```

### Temporal Decay

```rust
// Apply decay to all facts (run periodically)
engine.decay_all();

// Prune facts below threshold
let pruned = engine.prune_dead_facts();
println!("Removed {} dead facts", pruned.len());
```

## Lean checking (roadmap)

- Move reconciliation invariants into Lean (`VProb` bounds, monotonicity, etc.).
- Extend certificates beyond reachability into reconciliation derivations.
- Keep “unknown vs false” explicit: reconciliation should not silently assume closed world.

## Resolution Algorithm

```
function resolve(newFact, existingFact, config):
    domain = infer_domain(newFact)
    
    # Expert override for critical domains
    if config.expert_override 
       and domain in config.expert_domains
       and newFact.source.credibility > 0.9:
        return ReplaceOld
    
    # High-impact conflicts need review
    if existingFact.weight > config.human_review_threshold
       and conflict_type in [Contradiction, SchemaViolation]:
        return HumanReview
    
    weight_diff = newFact.weight - existingFact.weight
    
    # Clear winner
    if |weight_diff| > config.auto_resolve_threshold:
        return ReplaceOld if weight_diff > 0 else KeepOld
    
    # Close call - merge
    total = newFact.weight + existingFact.weight
    return Merge(newFact.weight/total, existingFact.weight/total)
```

## Configuration

```rust
pub struct ReconciliationConfig {
    /// Minimum weight difference to auto-resolve (0.0-1.0)
    pub auto_resolve_threshold: f32,
    
    /// Weight below which facts are pruned
    pub discard_threshold: f32,
    
    /// Half-life for temporal decay (days)
    pub decay_half_life: f64,
    
    /// Weight above which conflicts need human review
    pub human_review_threshold: f32,
    
    /// Allow expert sources to override
    pub expert_override: bool,
    
    /// Domains where expert override applies
    pub expert_domains: Vec<String>,
}
```

## Example: Machining Knowledge

```rust
// Setup
let mut engine = ReconciliationEngine::new(config);

engine.register_source(SourceCredibility {
    source_id: "master_machinist".to_string(),
    base_credibility: Weight::new(0.95),
    domain_expertise: [
        ("machining".to_string(), Weight::new(0.99)),
        ("materials".to_string(), Weight::new(0.9)),
    ].into_iter().collect(),
    track_record: TrackRecord { correct: 50, incorrect: 2 },
});

engine.register_source(SourceCredibility {
    source_id: "llm_extraction".to_string(),
    base_credibility: Weight::new(0.7),
    domain_expertise: HashMap::new(),
    track_record: TrackRecord::default(),
});

// Fact from LLM
let llm_fact = ExtractedFact {
    structured: StructuredFact::TacitKnowledge {
        rule: "titanium -> speed <= 60 SFM".to_string(),
        confidence: 0.85,
        domain: "machining".to_string(),
    },
    confidence: 0.85,
    ..
};
engine.reconcile(llm_fact);

// Same fact from expert (higher weight)
let expert_fact = ExtractedFact {
    structured: StructuredFact::TacitKnowledge {
        rule: "titanium -> speed <= 80 SFM with coolant".to_string(),
        confidence: 0.95,
        domain: "machining".to_string(),
    },
    confidence: 0.95,
    ..
};

// Expert overrides LLM due to higher credibility
let result = engine.reconcile(expert_fact);
assert!(matches!(result.action, ReconciliationAction::Integrated));
```

## Decay Curve

Facts decay exponentially over time:

$$w(t) = w_0 \cdot 2^{-t/\tau}$$

where:
- $w_0$ = initial weight
- $t$ = time since last update
- $\tau$ = half-life (default 30 days)

After 30 days with default config:
- Weight 1.0 → 0.5
- Weight 0.5 → 0.25
- Weight 0.8 → 0.4

## Best Practices

1. **Calibrate Sources**: Track record matters more than claimed expertise
2. **Domain Expertise**: Sources may be expert in some areas, not others
3. **Regular Decay**: Run `decay_all()` daily or weekly
4. **Prune Dead Facts**: Remove facts below threshold
5. **Review Queue**: Prioritize human review by impact
6. **Update Track Records**: Verify facts and update source credibility

## Integration with Storage

```rust
// After reconciliation, persist to unified storage
match result.action {
    ReconciliationAction::Integrated | ReconciliationAction::Merged => {
        let storable = weighted_fact_to_storable(&engine.get_fact(result.fact_id));
        storage.add_facts(vec![storable], source)?;
        storage.flush()?;
    }
    _ => {}
}
```
