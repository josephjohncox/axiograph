# Example LLM Query Session: Machining Knowledge

This shows how an LLM interacts with Axiograph for machining questions.

---

## Session Start

**Schema Context provided to LLM:**
```json
{
  "types": [
    "Material", "Tool", "Operation", "Feature", "Heuristic",
    "CuttingCondition", "ChipFormation", "Observation"
  ],
  "relations": [
    "RecommendedFor(Tool, Material)",
    "CausesObservation(CuttingCondition, Observation)",
    "HeuristicApplies(Heuristic, Domain)",
    "AffectsSurfaceFinish(Parameter, Direction)"
  ],
  "equivalences": [
    "SupplierEquiv", "RouteEquivalence", "ProcessEquiv"
  ]
}
```

---

## Question 1: Simple Lookup

**User**: "What cutting speed should I use for titanium?"

**LLM parses to**:
```json
{
  "type": "ConstrainedQuery",
  "base": {
    "type": "FindByRelation",
    "relation": "RecommendedSpeed",
    "role": "object",
    "value": "Titanium"
  },
  "constraints": [
    { "type": "MinConfidence", "threshold": 0.7 }
  ]
}
```

**Query Results**:
```json
{
  "results": [
    {
      "entity_id": "TitaniumLowSpeed",
      "entity_type": "Heuristic",
      "score": 0.9,
      "derivation": "Heuristic match",
      "attributes": {
        "rule": "Cut titanium at low speed high feed",
        "speed_range": "80-120 SFM"
      },
      "evidence": [
        {
          "source": "HeuristicConfidence",
          "text": "TitaniumLowSpeed: VeryHigh confidence",
          "confidence": 0.95
        }
      ]
    },
    {
      "entity_id": "conv_4",
      "entity_type": "ExtractedFact",
      "score": 0.75,
      "derivation": "Conversation extraction",
      "attributes": {
        "statement": "Try reducing to 100 SFM for titanium",
        "speaker": "Sarah"
      }
    }
  ],
  "suggestions": [
    "What feed rate for titanium?",
    "Why low speed for titanium?",
    "What tool coating for titanium?"
  ]
}
```

**LLM Answer**:
> For titanium, use **80-120 SFM** (surface feet per minute).
>
> **Why**: Titanium has poor thermal conductivity, so lower speeds allow heat to transfer into the chip rather than the workpiece. The TitaniumLowSpeed heuristic has 95% confidence.
>
> **Source**: Shop conversation with Sarah: "Try reducing to 100 SFM for titanium"
>
> **Follow-up**: Would you like to know the recommended feed rate?

---

## Question 2: Reasoning Chain

**User**: "Why do I get chatter when machining titanium at 3000 RPM?"

**LLM decomposes into**:

1. Find chatter-related observations:
```json
{ "type": "FindByRelation", "relation": "CausesObservation", "role": "object", "value": "Chatter" }
```

2. Find stability heuristics:
```json
{
  "type": "ConstrainedQuery",
  "base": { "type": "FindByType", "type_name": "Heuristic" },
  "constraints": [
    { "type": "AttrCompare", "attr": "name", "op": "Contains", "value": "Stability" }
  ]
}
```

3. Check if 3000 RPM is in unstable region:
```json
{ "type": "FindByRelation", "relation": "StabilityLobeData", "role": "any", "value": "*" }
```

**Combined Results**:
```json
{
  "results": [
    {
      "entity_id": "ChatterStabilityLobes",
      "entity_type": "Heuristic",
      "score": 0.95,
      "attributes": {
        "rule": "Stable zones exist between RPM and DOC",
        "rationale": "Regenerative vibration creates unstable regions"
      }
    },
    {
      "entity_id": "chatter_condition_1",
      "entity_type": "Observation",
      "score": 0.85,
      "derivation": "Pattern extraction from conversation",
      "attributes": {
        "threshold": "3000",
        "unit": "rpm"
      }
    }
  ]
}
```

**LLM Answer**:
> Chatter at 3000 RPM is likely due to **regenerative vibration** in an unstable zone.
>
> **Explanation**: The ChatterStabilityLobes heuristic (95% confidence) states that stable cutting exists in specific "lobes" between spindle speed and depth of cut. At 3000 RPM, you may be in an unstable region.
>
> **Recommendations**:
> 1. Try increasing or decreasing RPM by 10-20% to find a stable lobe
> 2. Reduce depth of cut
> 3. Check tool/workpiece rigidity
>
> **Source**: PhysicsKnowledge.axi - RegenerativeFrequency relation

---

## Question 3: Equivalence Query (HoTT)

**User**: "Can I use a different supplier for the steel billets?"

**LLM parses to**:
```json
{
  "type": "FindEquivalent",
  "entity": "RawMetal_A",
  "equivalence_type": "SupplierEquiv"
}
```

**Query Results**:
```json
{
  "results": [
    {
      "entity_id": "RawMetal_B",
      "entity_type": "Supplier",
      "score": 1.0,
      "derivation": "Equivalent via SupplierEquiv",
      "evidence": [
        {
          "source": "SupplyChain ontology",
          "text": "RawMetal_A ≃ RawMetal_B (QualityEquiv proof)",
          "confidence": 1.0,
          "source_type": "equivalence"
        }
      ],
      "attributes": {
        "quality_level": "Q_Premium",
        "provides": "Steel_Supply"
      }
    }
  ],
  "suggestions": [
    "What is the lead time for RawMetal_B?",
    "Are there other equivalent routes through the supply chain?"
  ]
}
```

**LLM Answer**:
> Yes, **RawMetal_B** is a proven equivalent supplier.
>
> **Proof**: The SupplierEquiv relation shows RawMetal_A ≃ RawMetal_B with QualityEquiv proof. Both provide Steel_Billet at Q_Premium quality level.
>
> **What this means**: You can substitute RawMetal_B for RawMetal_A in any supply chain path and get the same material quality. This is formally verified by the type system.
>
> **Caution**: Lead times may differ (7 days vs 14 days). Check if this affects your schedule.

---

## Question 4: Path Discovery

**User**: "How is the cutting force related to surface finish?"

**LLM parses to**:
```json
{
  "type": "FindPaths",
  "from": "CuttingForce",
  "to": "SurfaceFinish",
  "max_depth": 4
}
```

**Query Results**:
```json
{
  "results": [
    {
      "entity_id": "SurfaceFinish",
      "entity_type": "Quality",
      "score": 0.5,
      "derivation": "Path found: CuttingForce -> SurfaceFinish (2 steps)",
      "path": [
        { "from": "CuttingForce", "relation": "Causes", "to": "Vibration" },
        { "from": "Vibration", "relation": "Affects", "to": "SurfaceFinish" }
      ]
    },
    {
      "entity_id": "SurfaceFinish",
      "entity_type": "Quality",
      "score": 0.33,
      "derivation": "Path found: CuttingForce -> SurfaceFinish (3 steps)",
      "path": [
        { "from": "CuttingForce", "relation": "IncreasedBy", "to": "FeedRate" },
        { "from": "FeedRate", "relation": "Affects", "to": "ChipThickness" },
        { "from": "ChipThickness", "relation": "Determines", "to": "SurfaceFinish" }
      ]
    }
  ]
}
```

**LLM Answer**:
> Cutting force affects surface finish through **two main paths**:
>
> **Path 1 (Direct)**: CuttingForce → Vibration → SurfaceFinish
> - Higher cutting forces can cause vibration/chatter
> - Vibration directly degrades surface finish
>
> **Path 2 (Indirect)**: CuttingForce → FeedRate → ChipThickness → SurfaceFinish
> - Cutting force increases with feed rate
> - Higher feed creates thicker chips
> - Chip thickness affects the theoretical surface roughness (scallop height)
>
> **Recommendation**: To improve surface finish, reduce cutting forces by:
> 1. Using sharper tools
> 2. Reducing depth of cut
> 3. Optimizing feed rate (balance between rubbing and excessive force)

---

## Question 5: Meta Query

**User**: "What kinds of knowledge do we have about machining?"

**LLM parses to**:
```json
{
  "type": "MetaQuery",
  "about": { "ListTypes": null }
}
```

**Then follows up with**:
```json
{
  "type": "ConstrainedQuery",
  "base": { "type": "MetaQuery", "about": { "ListRelations": null } },
  "constraints": [
    { "type": "AttrCompare", "attr": "domain", "op": "Eq", "value": "machining" }
  ]
}
```

**LLM Answer**:
> The machining knowledge base contains:
>
> **Entity Types**:
> - Material (4 instances): Al6061_T6, Ti6Al4V, Inconel718, AISI_4140
> - Tool, Operation, Feature, CuttingCondition
> - Heuristic (6 instances): CuttingForceProportional, HeatGoesToChip, etc.
> - Observation, Measurement
>
> **Key Relations**:
> - RecommendedFor (Tool → Material)
> - CausesObservation (Condition → Observation)
> - SpecificCuttingEnergy (Material → Quantity)
> - StabilityLobeData (Tool × Setup → Limits)
>
> **Equivalences**:
> - SupplierEquiv (alternative sourcing)
> - ProcessEquiv (alternative machining strategies)
> - RouteEquivalence (alternative supply chain paths)
>
> **Would you like to explore any specific area?**

---

## Key Advantages Over RAG

1. **Structured reasoning**: "How is X related to Y?" returns actual paths, not just text mentioning both.

2. **Equivalence awareness**: "Can I substitute X?" uses formal HoTT equivalences with proofs.

3. **Confidence tracking**: Results include provenance and confidence from the probabilistic module.

4. **Schema introspection**: LLM can discover what's queryable, not just search existing text.

5. **Composable queries**: Complex questions decompose into structured query chains.

6. **Grounded answers**: Every claim has a citation path back to the ontology.

