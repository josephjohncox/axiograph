# Bidirectional LLM ↔ KG Sync Demo

This example demonstrates how Axiograph enables two-way knowledge flow between LLMs and the knowledge graph.

## Scenario: Machinist Training Assistant

An AI assistant helps train new machinists while learning from experienced ones.

---

## Session 1: Novice Asks Question

### User Query
```
New machinist: "I need to machine a titanium part. What speed should I use?"
```

### System: Query Knowledge Graph

```json
{
  "type": "Query",
  "request_id": "q-001",
  "query": {
    "type": "Combined",
    "operator": "AND",
    "queries": [
      { "type": "FindByType", "type_name": "Material" },
      { "type": "FollowPath", "start": "Titanium", "path": ["hasCuttingParameter"] }
    ]
  }
}
```

### System: Build Grounding Context

```json
{
  "type": "GroundingContext",
  "request_id": "ctx-001",
  "context": {
    "facts": [
      {
        "id": 1001,
        "natural": "Titanium (Ti-6Al-4V) is a difficult-to-machine material",
        "confidence": 0.98,
        "citation": ["Material", "Titanium", "MachinabilityRating"]
      },
      {
        "id": 1002,
        "natural": "Titanium cutting speed should not exceed 60 m/min",
        "confidence": 0.95,
        "citation": ["TacitKnowledge", "TitaniumSpeedLimit"]
      },
      {
        "id": 1003,
        "natural": "Titanium has low thermal conductivity (7.1 W/m·K)",
        "confidence": 0.99,
        "citation": ["Material", "Titanium", "ThermalConductivity"]
      }
    ],
    "active_guardrails": [
      {
        "rule_id": "MACH-002",
        "severity": "Critical",
        "description": "Titanium requires reduced cutting speeds",
        "applies_when": "hasMaterial(operation, Titanium)"
      }
    ],
    "suggested_queries": [
      "What tools are recommended for titanium?",
      "What coolant should I use for titanium?",
      "What are the signs of work hardening?"
    ]
  }
}
```

### LLM: Generate Grounded Response

```
For machining titanium (Ti-6Al-4V), you should use conservative cutting speeds:

**Recommended Speed Range**: 30-60 m/min [fact:1002]

This is significantly lower than steel or aluminum because titanium has 
very low thermal conductivity [fact:1003]. The heat from cutting stays 
concentrated at the tool tip instead of dissipating into the workpiece.

⚠️ **CRITICAL** [MACH-002]: Never exceed 60 m/min with titanium. Higher 
speeds cause:
- Rapid tool wear (carbide can fail in under 30 seconds)
- Work hardening of the cut surface
- Fire risk from titanium chips

**Suggestions:**
- Start at 40 m/min for roughing, 50-55 m/min for finishing
- Use flood coolant, not mist
- Use sharp, positive-rake carbide inserts

Would you like to know more about:
- [Recommended tool coatings for titanium]
- [Signs of work hardening]
- [Coolant requirements]
```

---

## Session 2: Expert Shares Knowledge

### Expert Input
```
Senior machinist: "I've been cutting Ti for 25 years. For Ti-6Al-4V specifically, 
I never go above 45 m/min on the first cut. The beta-annealed stuff can tolerate 
a bit more, maybe 55, but standard mill-annealed Ti-6-4 is safer at 45. Also, 
make sure you're using through-spindle coolant - mist will cause chip welding."
```

### System: Extract Facts

```json
{
  "type": "ProposeBatch",
  "request_id": "prop-001",
  "facts": [
    {
      "id": "f-001",
      "claim": "Ti-6Al-4V first cut speed should be limited to 45 m/min",
      "structured": {
        "type": "tacit_knowledge",
        "rule": "isFirstCut(op) && hasMaterial(op, Ti6Al4V) -> cuttingSpeed <= 45",
        "confidence": 0.92,
        "domain": "machining"
      },
      "source": {
        "experience": "25 years",
        "specificity": "high"
      }
    },
    {
      "id": "f-002",
      "claim": "Beta-annealed Ti-6Al-4V can tolerate higher speeds than mill-annealed",
      "structured": {
        "type": "relation",
        "rel_type": "hasHigherSpeedTolerance",
        "source": "BetaAnnealedTi6Al4V",
        "target": "MillAnnealedTi6Al4V"
      },
      "confidence": 0.88
    },
    {
      "id": "f-003",
      "claim": "Mist coolant causes chip welding on titanium",
      "structured": {
        "type": "relation",
        "rel_type": "causes",
        "source": "MistCoolant",
        "target": "ChipWelding",
        "attributes": {
          "material": "Titanium"
        }
      },
      "confidence": 0.95
    },
    {
      "id": "f-004",
      "claim": "Through-spindle coolant is required for titanium",
      "structured": {
        "type": "constraint",
        "name": "TitaniumCoolantRequirement",
        "condition": "hasMaterial(op, Ti) -> hasThroughSpindleCoolant(op)",
        "severity": "Warning"
      },
      "confidence": 0.90
    }
  ],
  "source_context": "Senior machinist with 25 years titanium experience"
}
```

### System: Validate and Integrate

```json
{
  "type": "Ack",
  "request_id": "prop-001",
  "status": "Processing",
  "details": {
    "f-001": {
      "status": "needs_review",
      "reason": "More restrictive than existing rule (45 vs 60 m/min)",
      "conflict": {
        "existing_fact": 1002,
        "type": "AttributeMismatch"
      }
    },
    "f-002": {
      "status": "validated",
      "action": "integrated",
      "kg_ids": [2001, 2002]
    },
    "f-003": {
      "status": "validated",
      "action": "integrated",
      "kg_ids": [2003]
    },
    "f-004": {
      "status": "validated",
      "action": "integrated",
      "kg_ids": [2004]
    }
  }
}
```

### System: Resolve Conflict

```json
{
  "type": "ConflictNotification",
  "request_id": "conflict-001",
  "conflict": {
    "new_fact": "f-001",
    "existing_fact": 1002,
    "conflict_type": "AttributeMismatch",
    "details": {
      "attribute": "max_speed",
      "new_value": "45 m/min",
      "existing_value": "60 m/min"
    }
  },
  "suggested_resolution": {
    "type": "Merge",
    "reasoning": "New fact is more conservative and comes from highly experienced source. Merge by creating tiered limits.",
    "merged_fact": {
      "claim": "Titanium cutting speed limits depend on cut type and heat treatment",
      "rules": [
        "First cut on mill-annealed Ti-6Al-4V: max 45 m/min",
        "Subsequent cuts on mill-annealed: max 55 m/min",
        "Beta-annealed Ti-6Al-4V: max 60 m/min"
      ]
    }
  }
}
```

---

## Session 3: Updated Knowledge Benefits Next User

### New Query
```
Another user: "I'm about to do my first cut on a Ti-6Al-4V block. Any tips?"
```

### System: Enhanced Grounding Context

```json
{
  "type": "GroundingContext",
  "request_id": "ctx-002",
  "context": {
    "facts": [
      {
        "id": 1002,
        "natural": "Titanium cutting speed limits depend on cut type and heat treatment",
        "confidence": 0.95,
        "citation": ["TacitKnowledge", "TitaniumSpeedLimits", "v2"]
      },
      {
        "id": 2001,
        "natural": "First cut on mill-annealed Ti-6Al-4V should not exceed 45 m/min",
        "confidence": 0.92,
        "citation": ["TacitKnowledge", "ExpertKnowledge", "session-002"],
        "provenance": "Senior machinist, 25 years experience"
      },
      {
        "id": 2003,
        "natural": "Mist coolant causes chip welding on titanium; use through-spindle coolant",
        "confidence": 0.95,
        "citation": ["TacitKnowledge", "CoolantRequirement"]
      },
      {
        "id": 2002,
        "natural": "Beta-annealed Ti-6Al-4V tolerates higher speeds than mill-annealed",
        "confidence": 0.88,
        "citation": ["Relation", "HeatTreatmentEffect"]
      }
    ],
    "active_guardrails": [
      {
        "rule_id": "MACH-002-v2",
        "severity": "Critical",
        "description": "First cut on Ti-6Al-4V: max 45 m/min",
        "applies_when": "isFirstCut(op) && hasMaterial(op, Ti6Al4V)"
      },
      {
        "rule_id": "MACH-005",
        "severity": "Warning",
        "description": "Use through-spindle coolant for titanium",
        "applies_when": "hasMaterial(op, Titanium)"
      }
    ],
    "learning_resources": [
      {
        "title": "Thermal Conductivity in Titanium Machining",
        "relevance": 0.95,
        "difficulty": "Beginner"
      },
      {
        "title": "Heat Treatment Effects on Machinability",
        "relevance": 0.85,
        "difficulty": "Intermediate"
      }
    ]
  }
}
```

### LLM: Generate Updated Response

```
Great question! For your **first cut** on Ti-6Al-4V, here are key tips:

## Speed
**Start at 40-45 m/min maximum** [fact:2001]

This is more conservative than general titanium guidelines because:
- First cuts encounter mill scale and surface inconsistencies
- No chip flow is established yet
- Tool is most vulnerable on entry

This recommendation comes from a machinist with 25 years of titanium experience.

## Coolant
**Use through-spindle coolant, NOT mist** [fact:2003]

Mist coolant causes chip welding on titanium. You need:
- Flood coolant at 70+ bar through the spindle
- Aim directly at the cutting zone
- Ensure chip evacuation

## Additional Tips
- Check if your material is mill-annealed or beta-annealed [fact:2002]
- Beta-annealed can handle slightly higher speeds (up to 55 m/min)
- Use sharp, positive-rake carbide inserts
- Listen for chatter - back off immediately if you hear it

⚠️ **CRITICAL** [MACH-002-v2]: For first cuts, the 45 m/min limit is firm.
You can increase slightly for subsequent passes once you know the material.

Would you like me to explain:
- [Why first cuts are more critical]
- [How to identify heat treatment type]
- [Signs of chip welding]
```

---

## Knowledge Graph Evolution

After these sessions, the KG has evolved:

### Before
```
TitaniumSpeedLimit
  └── maxSpeed: 60 m/min
  └── confidence: 0.95
  └── source: "handbook"
```

### After
```
TitaniumSpeedLimits (v2)
  └── rules:
      ├── FirstCut_MillAnnealed: max 45 m/min (confidence: 0.92)
      │   └── source: "25-year expert, session-002"
      ├── SubsequentCut_MillAnnealed: max 55 m/min (confidence: 0.88)
      └── BetaAnnealed: max 60 m/min (confidence: 0.95)
  └── provenance: merged from handbook + expert knowledge
  
TitaniumCoolantRequirement (NEW)
  └── type: Constraint
  └── condition: must use through-spindle coolant
  └── reason: mist causes chip welding
  └── source: "25-year expert, session-002"
  
HeatTreatmentEffect (NEW)
  └── BetaAnnealed hasHigherSpeedTolerance MillAnnealed
  └── confidence: 0.88
```

## Key Takeaways

1. **Knowledge grows organically** from conversations
2. **Expert knowledge is captured** with provenance
3. **Conflicts are resolved** thoughtfully
4. **Future users benefit** from accumulated knowledge
5. **Guardrails evolve** based on new information
6. **Citations remain traceable** to sources

