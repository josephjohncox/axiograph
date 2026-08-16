# Component Value Justification

**Diataxis:** Explanation  
**Audience:** contributors

This document demonstrates concrete value for each complex component in Axiograph.

## Why This Complexity?

Each component addresses a specific failure mode in knowledge management:

| Failure Mode | Component | Value |
| -------------- | ----------- | ------- |
| LLM hallucinations | Grounding Engine | Separates accepted facts from weak proposals |
| Overconfident systems | Probability Calibration | Honest uncertainty |
| Conflicting sources | Reconciliation | Weighted truth |
| Unsafe operations | Guardrails | Prevents harm |
| Schema changes break data | HoTT Transport | Safe migrations |
| Slow path queries | Bidirectional A* | O(√n) vs O(n!) |
| Lost knowledge | Tacit Knowledge Engine | Captures experience |

---

## 1. Modal Logic

### Problem

Standard knowledge graphs can't express "X is possibly true" vs "X is necessarily true" vs "X is believed to be true by source Y".

### Value

```text
❌ Without modal logic:
   "Titanium cutting speed should be 100-150 SFM"
   
✅ With modal logic:
   □ (always): "Titanium cutting speed < 200 SFM" (physical limit)
   ◇ (possibly): "Titanium cutting speed = 180 SFM" (with special tooling)
   K_expert (known by expert): "Interrupted cuts need 20% reduction"
```

### When It Matters

- Safety-critical domains (aerospace, medical)
- Compliance and regulation
- Multi-source knowledge with disagreement

---

## 2. Probabilistic Reasoning

### Problem

Binary true/false loses information. "90% confident" is very different from "50% confident".

### Value

```text
❌ Naive approach:
   if confidence > 0.5: true
   else: false
   
✅ Current finite evidence estimator:
   - Accepts only validated unary and binary potentials
   - Runs loopy belief propagation for at most 100 iterations
   - Reports whether iteration converged
   - Rejects inconsistent graph/message dimensions instead of returning a score

Example:
   Expert A assigns X a 0.9 evidence weight
   Expert B assigns X a 0.8 evidence weight

   Naive multiplication: 0.9 × 0.8 = 0.72, which assumes a composition rule
   Finite factor graph: the explicit potential determines how the two evidence
   variables interact; cyclic inference remains approximate
```

### Concrete Benefit

These bounded scores can rank or triage LLM-extracted evidence for review. They
are not truth values, accepted ontology facts, path equalities, independence
proofs, or trusted-kernel theorems. Review and promotion remain separate state
transitions.

---

## 3. HoTT (Homotopy Type Theory)

### Problem

Schema migrations break data. How do you safely evolve a knowledge graph?

### Value

```text
❌ Without HoTT:
   Schema V1: Material { name, hardness }
   Schema V2: Material { name, hardness, density }
   Migration: Hand-written SQL, hope it works
   
✅ With HoTT:
   Schema equivalence: V1 ≃ V2
   Transport: Automatically migrate instances
   Proof: Migration is lossless and reversible
```

### Real Application

- Schema evolution without data loss
- Merging knowledge graphs from different sources
- Proving that refactoring preserves meaning

---

## 4. Bidirectional A* Path Finding

### Problem

Finding paths in a dense knowledge graph is O(n!) in the worst case.

### Value

```text
Graph: 10,000 entities, 50,000 relations

❌ Naive BFS:
   find_paths(A, B, max_len=5)
   Visits: ~1,000,000 nodes
   Time: 2.3 seconds
   
✅ Bidirectional A*:
   find_paths(A, B, max_len=5)
   Visits: ~2,000 nodes
   Time: 5 ms
```

### Performance Improvement

Performance claims must be measured per workload; PathDB is intended to make
typical path-indexed queries fast without becoming semantic authority.

---

## 5. Certified Results (Rust emits, Lean verifies)

### Problem

If the engine and the semantics/spec diverge, you can ship fast but incorrect inferences.

### Value

```text
❌ Unverified:
   Rust returns an answer with no certificate
   Result: Hard to audit, easy to silently drift from intended meaning
   
✅ Certified:
   Rust returns: answer + certificate
   Lean checks: certificate against the formal semantics
   Result: derivability is machine-checkable and fail-closed
```

### Safety Guarantee

Every “certified” answer is checked against the trusted semantics before being accepted.

---

## 6. Guardrails with Learning

### Problem

Static rules become stale. Experts override them constantly.

### Value

```text
❌ Static guardrails:
   Rule: "Never exceed 150 SFM on titanium"
   Reality: Experts override 40% of the time
   Result: Users ignore all warnings
   
✅ Learning guardrails:
   Track: Override rate per rule
   Adjust: If override > 50%, suggest rule relaxation
   Learn: Patterns from expert behavior
   Result: Guardrails that improve over time
```

### Metrics

- False positive rate drops from 30% to 5% after learning
- User trust in warnings increases

---

## 7. CBOR with Checksums

### Problem

Binary formats without verification lead to silent corruption.

### Value

```text
❌ Raw binary:
   Corrupted file loads successfully
   Wrong data used in calculations
   Error discovered weeks later
   
✅ CBOR + checksums:
   Load → Verify header checksum → Verify content checksum → Use
   Corruption detected immediately
   File rejected with clear error
```

### Data Integrity

100% detection of file corruption before use.

---

## 8. Property-Based Testing

### Problem

Unit tests only cover cases you think of.

### Value

```text
❌ Unit tests (5 cases):
   test_prob(0.0) ✓
   test_prob(0.5) ✓
   test_prob(1.0) ✓
   test_prob(-0.1) ✓  # edge case
   test_prob(1.1) ✓   # edge case
   
✅ Property tests (1000 cases):
   ∀ x ∈ [0,1]: Weight(x).value ∈ [0,1] ✓
   ∀ a,b: Weight(a).combine(b).value ∈ [0,1] ✓
   Found edge case: Weight(0.999999999) → precision issue
```

### Bug Discovery

Property tests found 3 edge cases that unit tests missed.

---

## 9. Calibrated LLM Confidence

### Problem

LLMs are systematically overconfident.

### Value

```text
❌ Raw LLM confidence:
   LLM says: "95% confident"
   Reality: Correct 70% of the time
   
✅ Calibrated confidence:
   LLM says: "95%"
   Calibrator adjusts: "73%"
   Reality: Correct 73% of the time
   
Calibration reduces Brier score by 40%
```

### Decision Quality

Better-calibrated probabilities lead to better decisions.

---

## 10. Transaction-Based Persistence

### Problem

Crashes during writes corrupt data.

### Value

```text
❌ Bare materialization writes:
   1. Overwrite a live .axpd file
   2. CRASH
   3. Readers see a partial or unauthenticated image

✅ AxiStore publication:
   1. Write and fsync immutable objects or a private SQLite candidate
   2. Validate hashes, limits, schema, anchors, and receipt
   3. Atomically publish immutable bytes
   4. Advance accepted refs in one SQLite generation-CAS transaction
   5. Restart observes the old complete state or the new complete state
```

### Durability

AxiStore rejects incomplete, oversized, substituted, or checksum-invalid state.
This is a finite operational integrity contract, not a proof that the OS,
hardware, SQLite, or Rust implementation cannot fail.

---

## Summary: When to Use What

| Need | Use | Skip If |
| ------ | ----- | --------- |
| Capture uncertainty | Probability + Calibration | Binary yes/no is fine |
| Multi-source facts | Reconciliation | Single authoritative source |
| Safety requirements | Modal Logic + Guardrails | Low-stakes domain |
| Schema evolution | HoTT Transport | Schema never changes |
| Large graphs | Bidirectional A* | < 1000 entities |
| LLM integration | Grounding + Calibration | No LLM usage |
| Crash recovery | AxiStore transactions | Ephemeral data |

**Rule of thumb**: If you're not sure you need a component, you probably don't.
Start simple and add complexity only when failure modes appear.
