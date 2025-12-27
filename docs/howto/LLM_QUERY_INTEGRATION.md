# LLM Query Integration

**Diataxis:** How-to  
**Audience:** users (and contributors)

This document describes how LLMs can query Axiograph with rich semantic understanding, going far beyond traditional RAG.

Note: This doc mixes **conceptual** representations with what the code actually
parses today.

**Today**, the REPL supports two structured LLM integration modes:

- **Query mode**: the LLM proposes a structured query (`query_ir_v1` preferred; AxQL fallback).
- **Tool-loop mode** (`llm agent ...`, recommended): the LLM calls tools like `fts_chunks` and `axql_run`;
  Rust executes them against the snapshot; the LLM produces a grounded answer.

See:

- `docs/reference/QUERY_LANG.md` (AxQL + SQL-ish)
- `docs/reference/LLM_REPL_PLUGIN.md` (plugin protocol; v2 query mode + v3 tool-loop mode)

The JSON blocks below mix **conceptual** “semantic query” ideas with the
concrete `query_ir_v1` wire format. Not every concept here is implemented as a
first-class IR atom yet; when in doubt, prefer the exact `query_ir_v1` schema in
`docs/reference/LLM_REPL_PLUGIN.md`.

## Why structured queries > traditional RAG

| Traditional RAG | Axiograph structured queries (AxQL) |
|-----------------|---------------------------|
| Vector similarity on text chunks | Type-aware structured queries |
| No understanding of relationships | Path-based reasoning |
| Single-hop retrieval | Multi-hop traversal |
| No reasoning about equivalences | HoTT-based equivalence queries |
| Flat confidence scores | Probabilistic provenance |
| No query composition | Boolean algebra on queries |
| No schema awareness | Meta-queries on schema |

## Query Types

### 1. Type Queries
Find all entities of a given type:

```json
{
  "type": "FindByType",
  "type_name": "Material"
}
```

**Natural language**: "What materials do we have?"

### 2. Relation Queries
Find entities by relationship:

```json
{
  "type": "FindByRelation",
  "relation": "RecommendedFor",
  "role": "object",
  "value": "Titanium"
}
```

**Natural language**: "What tools are recommended for titanium?"

### 3. Path Traversal
Follow a chain of relationships:

```json
{
  "type": "FollowPath",
  "start": "Alice",
  "path": ["Parent", "Sibling", "Child"]
}
```

**Natural language**: "Who are Alice's cousins?" (parent's sibling's child)

### 4. Path Discovery
Find all paths between entities:

```json
{
  "type": "FindPaths",
  "from": "Steel_Billet",
  "to": "Customer_X",
  "max_depth": 6
}
```

**Natural language**: "How does steel get from raw material to the customer?"

### 5. Equivalence Queries (HoTT!)
Find equivalent entities:

```json
{
  "type": "FindEquivalent",
  "entity": "RawMetal_A",
  "equivalence_type": "SupplierEquiv"
}
```

**Natural language**: "What other suppliers can provide the same material?"

### 6. Constrained Queries
Filter results by constraints:

```json
{
  "type": "ConstrainedQuery",
  "base": {
    "type": "FindByType",
    "type_name": "Material"
  },
  "constraints": [
    { "type": "AttrCompare", "attr": "hardness", "op": "Gt", "value": "50" },
    { "type": "HasRelation", "relation": "MachinableWith" }
  ]
}
```

**Natural language**: "Find materials harder than 50 HRC that can be machined"

### 7. Probabilistic Queries
Filter by confidence:

```json
{
  "type": "ProbabilisticQuery",
  "base": {
    "type": "FindByRelation",
    "relation": "RecommendedSpeed",
    "role": "object",
    "value": "100_SFM"
  },
  "min_confidence": 0.8
}
```

**Natural language**: "What are the confident recommendations for 100 SFM cutting speed?"

### 8. Meta Queries
Query the schema itself:

```json
{
  "type": "MetaQuery",
  "about": { "ListTypes": null }
}
```

**Natural language**: "What kinds of entities exist in this knowledge base?"

### 9. Composite Queries
Boolean combinations:

```json
{
  "type": "And",
  "left": {
    "type": "FindByType",
    "type_name": "Tool"
  },
  "right": {
    "type": "FindByRelation",
    "relation": "RecommendedFor",
    "role": "subject",
    "value": "Carbide"
  }
}
```

**Natural language**: "Find carbide tools that are recommended for something"

## LLM Integration Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                         User Question                            │
│     "What suppliers can I use instead of RawMetal_A?"           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    LLM Query Parser                              │
│                                                                  │
│  Input: Question + Schema Context + Examples                     │
│  Output: Parsed AxQL + Confidence + Alternatives                 │
│                                                                  │
│  select ?s2 where name("RawMetal_A") -SupplierEquiv-> ?s2 limit 20│
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Query Execution Engine                         │
│                                                                  │
│  - Traverse knowledge graph                                      │
│  - Apply constraints                                             │
│  - Compute paths                                                 │
│  - Score by confidence                                           │
│  - Find equivalences (HoTT)                                      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Query Results                                 │
│                                                                  │
│  [{ entity_id: "RawMetal_B",                                     │
│     score: 1.0,                                                  │
│     derivation: "Equivalent via SupplierEquiv",                  │
│     evidence: [{ text: "RawMetal_A ≃ RawMetal_B (QualityEquiv)", │
│                  confidence: 1.0 }] }]                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    LLM Answer Generation                         │
│                                                                  │
│  Input: Results + Original Question + Schema                     │
│  Output: Grounded Answer with Citations                          │
│                                                                  │
│  "RawMetal_B is an equivalent supplier to RawMetal_A.            │
│   They both provide Steel_Billet at Q_Premium quality level.     │
│   [Source: SupplierEquiv relation in SupplyChain ontology]"      │
└─────────────────────────────────────────────────────────────────┘
```

## Example: Complete Query Session

### User Question
"How can materials get from suppliers to customers, and what alternative routes exist?"

### Step 1: LLM Parses to Semantic Query
```json
{
  "type": "FindPaths",
  "from": "RawMetal_A",
  "to": "Customer_X",
  "max_depth": 8
}
```

### Step 2: Execute Path Discovery
Results show multiple paths through the supply chain.

### Step 3: Follow-up with Equivalence Query
```json
{
  "type": "And",
  "left": {
    "type": "FindPaths",
    "from": "RawMetal_B",
    "to": "Customer_X",
    "max_depth": 8
  },
  "right": {
    "type": "FindEquivalent",
    "entity": "Route_Via_SupplierA",
    "equivalence_type": "RouteEquivalence"
  }
}
```

### Step 4: Grounded Answer

> **Answer**: There are 3 main routes from suppliers to customers:
>
> 1. **RawMetal_A → RawMaterial_WH → Machining_Plant → WIP_WH → Assembly_Plant → Finished_WH → Customer_X** (7 steps)
>
> 2. **RawMetal_B → RawMaterial_WH → Machining_Plant → ...** (equivalent route via backup supplier)
>
> 3. **Direct route** (skipping warehouse staging)
>
> **Key insight**: Routes via RawMetal_A and RawMetal_B are *equivalent* (proven by SupplierEquiv relation - both provide Steel_Billet at Q_Premium quality). This means dual-sourcing is valid.
>
> **Citations**:
> - [SupplyChain.RouteEquivalence] Route_Via_SupplierA ≃ Route_Via_SupplierB (SameMaterial)
> - [SupplyChain.SupplierEquiv] RawMetal_A ≃ RawMetal_B (QualityEquiv)

## Advanced: Reasoning Chains

For complex questions, the LLM can decompose into multiple queries:

### Question
"If I want to machine titanium with good surface finish, what should I do?"

### Decomposition
1. **Find constraints**: What affects surface finish?
   ```json
   { "type": "FindByRelation", "relation": "AffectsSurfaceFinish", "role": "any", "value": "*" }
   ```

2. **Find recommendations**: What's recommended for titanium?
   ```json
   { "type": "FindByRelation", "relation": "RecommendedFor", "role": "object", "value": "Titanium" }
   ```

3. **Follow heuristics**: What do experts say?
   ```json
   {
     "type": "ConstrainedQuery",
     "base": { "type": "FindByType", "type_name": "Heuristic" },
     "constraints": [
       { "type": "AttrCompare", "attr": "domain", "op": "Eq", "value": "machining" },
       { "type": "MinConfidence", "threshold": 0.7 }
     ]
   }
   ```

4. **Combine results**: Apply constraints to recommendations

### Grounded Answer
> Based on the knowledge graph:
>
> 1. **Speed**: Use low cutting speed (~100 SFM) [Source: TitaniumLowSpeed heuristic, conf: 0.9]
>
> 2. **Feed**: High feed rate (0.004+ IPT) to avoid rubbing [Source: Sample conversation, Sarah at 10:36]
>
> 3. **Tool**: TiAlN coated carbide [Source: Mike's recommendation, conf: 0.7]
>
> 4. **Coolant**: Flood coolant, high pressure [Source: Sarah at 10:40, conf: 0.8]
>
> **Reasoning**: Titanium has poor thermal conductivity (HeatPartitionHeuristic), so heat must go into the chip. Lower speeds give more time for chip formation; higher feeds ensure material removal rather than rubbing.

## Schema Context for LLM

When prompting an LLM to parse queries, provide:

```json
{
  "types": [
    { "name": "Material", "attributes": ["hardness", "conductivity"] },
    { "name": "Tool", "attributes": ["coating", "geometry"] },
    { "name": "Supplier", "attributes": ["lead_time", "quality_level"] }
  ],
  "relations": [
    { "name": "RecommendedFor", "source_type": "Tool", "target_type": "Material" },
    { "name": "SupplierEquiv", "source_type": "Supplier", "target_type": "Supplier", "is_symmetric": true }
  ],
  "example_queries": [
    {
      "natural_language": "What tools work for aluminum?",
      "axql": "select ?t where ?t is Tool, ?t -RecommendedFor-> name(\"Aluminum\") limit 20"
    }
  ]
}
```

## Comparison: RAG vs Axiograph

| Question | Traditional RAG | Axiograph |
|----------|-----------------|-----------|
| "What tools for titanium?" | Search chunks containing "titanium" and "tool" | Follow `RecommendedFor` relation with `object=Titanium` |
| "How is Alice related to Bob?" | Can't answer (no reasoning) | `FindPaths(Alice, Bob, 5)` returns kinship chain |
| "Can I use Supplier B instead of A?" | Requires chunk to explicitly state | `FindEquivalent(SupplierA)` uses HoTT equivalence |
| "High confidence recommendations only" | No confidence model | `ProbabilisticQuery(base, min_confidence=0.8)` |
| "What entities exist?" | Can't introspect | `MetaQuery(ListTypes)` |

## Implementation Notes

- **Query parsing** happens in the LLM (with schema context)
- **Query execution** happens in Rust (`axiograph-cli` AxQL engine over PathDB)
- **Semantic/spec checking** happens in Lean (certificates)
- **Answer generation** returns to LLM (with grounded results)

The LLM never hallucinates structure—it can only query what exists in the typed ontology.

## REPL support (today)

The `axiograph` REPL can run:

- **Deterministic NL templates**: `ask …` → AxQL
- **LLM-assisted** (pluggable): `llm ask …` / `llm answer …`

The LLM layer is intentionally “untrusted”: it produces candidate queries; the
engine executes them (and later: can produce certificates for Lean checking).
