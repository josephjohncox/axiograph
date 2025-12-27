# Axiograph for Knowledge Generation and Learning

**Diataxis:** Explanation  
**Audience:** contributors

> NOTE (Rust+Lean release): the trusted semantics/checker layer is Lean. Any Idris snippets in this document are historical notes from an earlier prototype and should be ported to Lean.

## The Problem

Domain expertise is hard to acquire:
- **Tacit knowledge**: Experts know things they can't easily articulate
- **Safety critical**: Mistakes can be costly (broken tools) or dangerous
- **Scattered resources**: Knowledge spread across manuals, forums, tribal wisdom
- **No guardrails**: Novices don't know what they don't know

## Axiograph's Solution

Axiograph transforms domain knowledge into a **queryable, verifiable, educational system**:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    KNOWLEDGE LIFECYCLE                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  INGESTION          REPRESENTATION        APPLICATION               │
│  ─────────          ──────────────        ───────────               │
│                                                                     │
│  Conversations  →   Probabilistic   →   Guardrails                  │
│  Books/PDFs     →   Facts (0.85      →   "WARNING: Titanium..."    │
│  Confluence     →   confidence)      →                              │
│  Expert Review  →                    →   Learning Suggestions       │
│                     Modal Logic      →   "Learn about thermal..."   │
│                     (Obligation,     →                              │
│                     Knowledge)       →   Query Answering            │
│                                      →   "Why can't I use HSS?"     │
│                     HoTT Paths       →                              │
│                     (Equivalences,   →   Proof Generation           │
│                     Migrations)      →   "Here's why X implies Y"   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Key Capabilities

### 1. Knowledge Ingestion from Multiple Sources

```bash
# Ingest machinist conversations
axiograph ingest conversation --input shop_floor_chat.txt \
  --domain machining --out machining_facts.axi

# Ingest technical books
axiograph ingest pdf --input machining_handbook.pdf \
  --chunks chunks.json --facts extracted_facts.axi

# Ingest Confluence documentation
axiograph ingest confluence --input wiki_export.html \
  --space "Manufacturing" --out wiki_knowledge.axi
```

The system extracts:
- **Explicit facts**: "Titanium cutting speed should be 30-60 m/min"
- **Tacit patterns**: "When [experienced machinist] says 'it sounds right', they mean..."
- **Relationships**: Material → requires → Tool coating

### 2. Probabilistic Truth with Provenance

Not all knowledge is certain. Axiograph tracks:

```idris
record TacitFact where
  claim : String
  confidence : Prob           -- 0.0 to 1.0
  sources : List Source       -- Where it came from
  supportingFacts : List Fact -- What confirms it
  contradictingFacts : List Fact -- What challenges it
```

Example:
```
Fact: "Carbide is better than HSS for titanium"
Confidence: 0.92
Sources: 
  - Tool manufacturer data (0.95 weight)
  - Shop floor experience (0.90 weight)  
  - One contradicting blog post (ignored, low credibility)
```

### 3. Guardrails That Teach

When a novice makes a potentially dangerous choice:

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🚨 CRITICAL: Titanium cutting speed exceeds safe limit              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│ Your operation specifies 120 m/min cutting speed for Ti-6Al-4V.    │
│ This exceeds the maximum recommended speed of 60 m/min.             │
│                                                                     │
│ RISKS:                                                              │
│ • Rapid tool wear (carbide failure in <30 seconds)                  │
│ • Work hardening of workpiece surface                               │
│ • Potential ignition of titanium chips (fire hazard)                │
│                                                                     │
│ LEARN MORE:                                                         │
│ 📚 Thermal Conductivity in Machining (Beginner)                     │
│ 📚 Work Hardening (Intermediate) - requires: Thermal Conductivity   │
│ 📚 Titanium Alloy Properties (Advanced)                             │
│                                                                     │
│ SIMILAR EXAMPLES:                                                   │
│ ❌ Ti roughing at 120 m/min → Tool failure after 3 parts            │
│ ✅ Ti roughing at 45 m/min → 200+ parts before tool change          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 4. Progressive Disclosure

Information density adapts to user experience:

| Experience Level | What They See |
|------------------|---------------|
| **Novice** (0-20%) | Simple warnings, one clear action |
| **Basic** (20-40%) | Explanation + primary suggestion |
| **Standard** (40-60%) | Full explanation, all suggestions, learning links |
| **Detailed** (60-80%) | + Evidence paths, technical data |
| **Expert** (80-100%) | + Proofs, underlying physics, exceptions |

### 5. Modal Logic for Obligations and Knowledge

**Deontic Logic** (What SHOULD be):
```idris
-- It is obligatory to use coolant for deep holes
Obligatory(isDrilling(op) && isDeep(op) -> hasCoolant(op))

-- It is permitted to use HSS for aluminum
Permitted(hasMaterial(op, Al) && usesTool(op, HSS))

-- It is forbidden to override safety interlocks
Forbidden(overrideSafetyInterlock)
```

**Epistemic Logic** (Who KNOWS what):
```idris
-- Identify what a novice needs to learn
knowledge_gap = 
  { x | Knows(Expert, x) && not(Knows(Novice, x)) }

-- Find the minimum learning path
learning_path = shortestPath(NoviceKnowledge, ExpertKnowledge)
```

### 6. Query-Based Learning

Users can ask natural questions, translated to semantic queries:

**"What should I know before machining titanium?"**
```rust
SemanticQuery::FollowPath {
    start: "Titanium",
    path: vec!["relatedTo", "requires"],  // Transitive closure
}
```

**"What could go wrong if I use high speed?"**
```rust
SemanticQuery::AndQuery {
    left: Box::new(SemanticQuery::FollowPath {
        start: "HighSpeed",
        path: vec!["causes"],
    }),
    right: Box::new(SemanticQuery::FindByType {
        type_name: "NegativeOutcome",
    }),
}
```

**"Show me that titanium is dangerous at high speed"**
```rust
// Returns proof witness
let result = executor.execute(&query);
for derivation in result.derivations {
    println!("Proof: {:?}", derivation.proof_witness);
    // ReachStep(Ti → highSpeed → danger) 
    //   with confidence 0.95
    //   from sources [handbook, experience]
}
```

### 6.1 Typed “concept graph” extraction (Rust)

In the Rust runtime we treat “learning” as an **extension structure** over the
ordinary knowledge graph: Concepts, prerequisites, guidelines, and examples are
all just entities and relations — but we extract a *typed* view so the engine
can reason safely and efficiently.

- Extractor: `rust/crates/axiograph-pathdb/src/learning.rs`
- Uses schema-scoped typing witnesses: `rust/crates/axiograph-pathdb/src/axi_typed.rs`

This is the Rust analogue of dependent types: the extractor returns *data +
evidence that it is well-typed under a specific `.axi` schema*, so downstream
code does not have to re-check.

## Architecture for Verified Learning

```
┌─────────────────────────────────────────────────────────────────────┐
│                        VERIFICATION STACK                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  LAYER 1: TYPE SAFETY (Idris)                                       │
│  ───────────────────────────                                        │
│  • Probability values are in [0,1] by construction                  │
│  • Path lengths tracked at type level                               │
│  • Reachability carries proof witnesses                             │
│                                                                     │
│  LAYER 2: RUNTIME VERIFICATION (Verus)                              │
│  ─────────────────────────────────────                              │
│  • Bitmap operations preserve bounds                                │
│  • Modal frames maintain consistency                                │
│  • Query results satisfy postconditions                             │
│                                                                     │
│  LAYER 3: DOMAIN GUARDRAILS                                         │
│  ─────────────────────────────                                      │
│  • Safety rules enforced before dangerous operations                │
│  • Knowledge gaps identified and addressed                          │
│  • Learning resources linked to violations                          │
│                                                                     │
│  LAYER 4: PROVENANCE TRACKING                                       │
│  ────────────────────────────                                       │
│  • Every fact traceable to source                                   │
│  • Confidence based on evidence strength                            │
│  • Contradictions surfaced, not hidden                              │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## Example: Onboarding a New Machinist

### Day 1: Safety Foundations

```
axiograph learn --domain machining --level novice --topic safety

OUTPUT:
═══════════════════════════════════════════════════════════════════════
                    MACHINING SAFETY FOUNDATIONS
═══════════════════════════════════════════════════════════════════════

📚 Core Concepts (Start Here):
   1. Personal Protective Equipment
   2. Machine Interlock Purpose
   3. Emergency Stop Procedures

⚠️  Critical Rules (Never Violate):
   • Always wear safety glasses
   • Never override interlocks
   • Report unusual sounds/smells immediately

✅ You'll be tested on:
   • Identifying PPE requirements
   • Locating emergency stops
   • Basic hazard recognition
```

### Week 2: Material-Specific Knowledge

```
axiograph learn --domain machining --level basic --topic "titanium machining"

OUTPUT:
═══════════════════════════════════════════════════════════════════════
                    TITANIUM MACHINING BASICS
═══════════════════════════════════════════════════════════════════════

📚 Prerequisites (review first):
   • Thermal Conductivity (completed ✅)
   • Basic Cutting Theory (in progress ⏳)

📖 Key Concepts:
   1. Why titanium is different
      - Low thermal conductivity (heat stays in cut)
      - Work hardening tendency
      - Reactive at high temperatures
   
   2. Safe parameter ranges
      - Cutting speed: 30-60 m/min (NEVER exceed)
      - Feed: 0.1-0.2 mm/rev (must cut, not rub)
      - Coolant: Required (flood, not mist)

🔬 Learn by Example:
   • [Success] Ti roughing at 45 m/min → 200+ parts
   • [Failure] Ti roughing at 120 m/min → Catastrophic tool failure

💡 Pro Tips (from experienced machinists):
   "Listen to the cut - titanium has a distinctive sound when
    it's cutting right. Higher pitched = too fast, chattering
    = too aggressive, dull thud = feed too light."
```

### Month 3: Advanced Troubleshooting

```
axiograph query --domain machining "why is my titanium part work hardening?"

OUTPUT:
═══════════════════════════════════════════════════════════════════════
                    DIAGNOSIS: WORK HARDENING
═══════════════════════════════════════════════════════════════════════

🔍 Most likely causes (confidence-ranked):

1. Feed rate too light (92% confidence)
   Path: Operation → lowFeed → rubbing → workHardening
   Evidence: Tool manufacturer data, shop floor reports
   
   FIX: Increase feed rate to ensure positive cutting action.
        Minimum 0.1 mm/rev for roughing.

2. Dull tool (78% confidence)
   Path: Tool → wornEdge → rubbing → workHardening
   Evidence: Tool life data, visual inspection reports
   
   FIX: Check tool edge under magnification. Replace if 
        flank wear exceeds 0.3mm.

3. Interrupted cut with dwell (65% confidence)
   Path: Operation → dwell → heatBuild → workHardening
   Evidence: Case studies, thermal analysis
   
   FIX: Minimize dwelling in cut. Use through-feed 
        strategies where possible.

📚 Deep Dive Resources:
   • Work Hardening Mechanics (paper, 15 min read)
   • Titanium Cutting Strategies (video, 8 min)
   • Thermal Management in Machining (advanced, 1 hour)
```

## Benefits Summary

| Stakeholder | Benefit |
|-------------|---------|
| **Novices** | Learn faster with guardrails preventing costly mistakes |
| **Journeymen** | Fill knowledge gaps, understand "why" not just "what" |
| **Experts** | Capture tacit knowledge for preservation |
| **Organizations** | Reduce onboarding time, prevent accidents, preserve expertise |

## Getting Started

1. **Define your domain schema** (entities, relations, constraints)
2. **Ingest existing knowledge** (books, docs, expert interviews)
3. **Add guardrail rules** for safety-critical operations
4. **Link learning resources** to violations
5. **Deploy and iterate** based on user feedback

The knowledge graph grows organically as:
- Users ask questions (gaps identified)
- Experts provide answers (facts added)
- Guardrails trigger (learning moments captured)
- Contradictions surface (knowledge refined)

## References

- [Axiograph Verification and Guardrails](VERIFICATION_AND_GUARDRAILS.md)
- [PathDB Design](PATHDB_DESIGN.md)
- [HoTT for Knowledge Graphs](HOTT_FOR_KNOWLEDGE_GRAPHS.md)
- [LLM Query Integration](LLM_QUERY_INTEGRATION.md)
