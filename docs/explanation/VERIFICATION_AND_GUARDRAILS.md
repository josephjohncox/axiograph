# Axiograph Verification and Guardrails

**Diataxis:** Explanation  
**Audience:** contributors

## Overview

Axiograph v6 is migrating to a **proof-carrying** architecture:

- **Rust** is the high-performance *untrusted engine* (ingestion, indexing, search, reconciliation, certificate emission).
- **Lean** is the *trusted checker/spec* (mathlib-backed): it defines meaning and verifies certificates.

This document summarizes how verification and guardrails fit together. For the end-to-end “book”, see `docs/explanation/BOOK.md`. For how to run checkers, see `docs/howto/FORMAL_VERIFICATION.md`.

## Verification Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         Axiograph verification stack                          │
├──────────────────────────────────────────────────────────────────────────────┤
│ Trusted: Lean                                                                 │
│  - semantics/spec (paths/groupoid/rewrite meaning, confidence algebra, policy)│
│  - certificate parsers + checkers (fail-closed)                               │
├──────────────────────────────────────────────────────────────────────────────┤
│ Untrusted: Rust runtime (cluster in production)                               │
│  - ingestion, indexing, search, reconciliation                                │
│  - emits: answers + certificates                                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ Hardening (optional, additive)                                                │
│  - Verus/Kani/Miri/fuzzing: engine invariant hardening                        │
└──────────────────────────────────────────────────────────────────────────────┘
```

The key discipline is:

> The engine may be clever; the checker must be small and stable.

Verification effort goes into defining meaning in Lean and checking certificates against it, not into “proving the engine correct”.

## Ways This Can Be Broken / Dumb (and what we do about it)

This is a non-exhaustive list of common failure modes in “proof-carrying” systems.
We treat these as *design constraints*, not as footnotes.

### 1) Conflating “certificate-checked” with “true”

Certificates prove **derivability from the accepted inputs under the formal semantics**.
They do *not* prove that the accepted inputs are correct, complete, or grounded in reality.

Guardrails:
- We keep **provenance** and **confidence** on facts, and we maintain separate planes:
  **evidence/proposals** (untrusted) vs **accepted/canonical** (reviewed).
- Any “certified answer” is always **snapshot-scoped** (“true in snapshot S”), which keeps the
  meaning clear in distributed and evolving settings.

### 2) Building a checker that just re-runs the same algorithm

If the “checker” is basically “run the same optimizer/query again and compare outputs”, we’ve
accidentally moved the trust problem instead of solving it.

Guardrails:
- Certificate formats are designed to be **replayable** and **small-step checkable**
  (e.g. rewrite derivations are rule+position traces, not opaque claims).
- “Recompute-and-compare” is allowed only as a **migration scaffold** (useful early, unsafe as a terminal state).
  The long-term target is: prove rule soundness once (Lean/mathlib), then replay derivations cheaply.

### 3) Not making “unknown vs false” explicit

In an open-world knowledge setting, missing facts are usually **unknown**, not **false**.
Silent closed-world assumptions can cause inconsistency blowups and brittle “negation by failure”.

Guardrails:
- Query results are treated as **witness-based**: “we found a derivation” vs “we did not find one”.
  Absence of a result is not automatically a negative claim.
- Any negative information should be represented explicitly (as policy, constraints, or signed evidence),
  not inferred implicitly from missing edges.
- Shape/validation work (SHACL-like) should be certificate-checked and should preserve “unknown” explicitly.

### 4) Treating inverses/groupoids as factual invertibility

The **free-groupoid** semantics gives us formal inverses of *paths as expressions*.
That does not mean the underlying real-world relation is invertible (“every edge has an inverse in reality”).

Guardrails:
- We distinguish **formal inverse** (in the path expression algebra) from **domain invertibility**
  (which requires explicit axioms or evidence such as an `Inverse` relation).
- Certificates about normalization/equivalence manipulate only the **expression layer**
  unless a domain rule explicitly states invertibility.

### 5) Treating confidence math as calibrated truth-probability

Our “confidence” is a **bounded, algebraic weight** used for ranking and policy.
It should not be interpreted as a calibrated probability of truth unless we have a calibration story.

Guardrails:
- Certificates ensure the **invariants** of confidence combination (bounds, monotonicity where intended),
  not “truth of the world”.
- Any product decisions that require calibrated uncertainty should treat these values as inputs to
  a separate calibration and evaluation pipeline.

## Certificates (Rust → Lean)

The core production guardrail is: high-value results are **untrusted until verified**.

- Rust emits a versioned JSON certificate (reachability, resolution, normalization, etc.).
- Lean parses the certificate strictly and validates it against the formal semantics.

See:

- `docs/reference/CERTIFICATES.md` (formats)
- `docs/howto/FORMAL_VERIFICATION.md` (how to run checks)
- `docs/explanation/BOOK.md` Part IV (certificate design and threat model)

## Shared Binary Format (v2)

PathDB’s binary format is specified in Rust (and hardened with optional Rust
verification tooling). Lean’s trusted core focuses on **certificate checking**
and `.axi` anchors; it does not currently parse `.axpd` bytes directly.

```rust
/// Magic number: "AXPD" in ASCII
pub const MAGIC_NUMBER: u32 = 0x41585044;

/// Feature flags
pub mod feature_flags {
    pub const MODAL_LOGIC: u64 = 1 << 0;
    pub const PROBABILISTIC: u64 = 1 << 1;
    pub const TEMPORAL_LOGIC: u64 = 1 << 2;
    // ...
}

/// Binary header (64 bytes)
#[repr(C, packed)]
pub struct BinaryHeader {
    pub magic: u32,
    pub version: u32,
    pub flags: u64,
    pub string_offset: u64,
    pub entity_offset: u64,
    pub relation_offset: u64,
    pub path_index_offset: u64,
    pub total_size: u64,
    pub checksum: u64,
}
```

### Compatibility Proofs

In Verus, we verify header parsing:

```rust
#[requires(bytes.len() >= Self::SIZE)]
pub fn from_bytes(bytes: &[u8]) -> Option<Self>
```

## Modal and Probabilistic Logic Support

### Modal Logic (PathDB v2)

PathDB v2 supports storing and querying modal frames:

```rust
// Create a Kripke frame
let mut frame = ModalFrame::new_kripke(1);

// Add worlds
frame.add_world(ModalWorld { world_id: 0, true_props, ... });
frame.add_world(ModalWorld { world_id: 1, true_props, ... });

// Add accessibility
frame.add_accessibility(acc_rel, 0, 1);  // w0 can access w1

// Query: □p (necessarily p)
let result = frame.box_worlds(acc_rel, &p_worlds);

// Query: ◇p (possibly p)
let result = frame.diamond_worlds(acc_rel, &p_worlds);
```

### Probabilistic Queries

```rust
// Verified probability type
let p = VerifiedProb::new(0.7);  // Verified to be in [0, 1]

// AxQL/PathDB can apply a confidence threshold during querying.
// (For certified querying, the threshold is recorded in the certificate.)
```

### Query Translation

AxQL/SQL-ish atoms compile into PathDB operations (type indexes, relation
indexes, RPQ traversal, and optional confidence filtering).

## Guardrails for Learning

The guardrails system provides safety nets for inexperienced users:

### Rule Definition

```rust
GuardrailRule {
    id: "MACH-002".to_string(),
    name: "Titanium with high speed".to_string(),
    description: "Titanium requires reduced cutting speeds...",
    severity: Severity::Critical,
    domain: "machining".to_string(),
    violation_pattern: Some(ViolationPattern {
        path: vec!["hasMaterial", "isTitanium"],
        ...
    }),
    forbidden_relations: vec!["hasHighSpeed".to_string()],
    ...
}
```

### Progressive Disclosure

Information is tailored to user experience level:

| Experience | Disclosure Level | Output |
|------------|------------------|--------|
| 0.0-0.2    | Minimal          | "Missing material spec" |
| 0.2-0.4    | Basic            | Explanation + 1 suggestion |
| 0.4-0.6    | Standard         | Full explanation + all suggestions |
| 0.6-0.8    | Detailed         | + Evidence paths + resources |
| 0.8-1.0    | Expert           | + Technical details + proofs |

### Learning Resources

When violations are detected, the system suggests learning materials:

```rust
LearningResource {
    resource_type: LearningResourceType::Concept,
    title: "Titanium Machining Properties",
    description: "Understanding thermal conductivity and work hardening",
    kg_path: Some(vec!["Concept", "TitaniumMachining"]),
    relevance: 0.95,
}
```

## Proof Generation

PathDB and the query runtime generate proof witnesses that can be verified by a trusted checker.

Today, the production direction is:

- Rust emits **JSON certificates** verified by **Lean**.
- Optional Rust verification tooling (Verus/Kani/Miri/fuzzing) hardens invariants, but does not replace Lean certificate checking.

### Rust Proof Structure

```rust
pub enum ReachabilityProof {
    Reflexive { entity: u32 },
    Step {
        from: u32,
        rel_type: u32,
        to: u32,
        rel_confidence: VerifiedProb,
        rest: Box<ReachabilityProof>,
    },
}
```

### Lean checking (certificate-first)

Instead of generating “proof code”, the engine serializes a compact certificate that the Lean checker validates. This scales better operationally and keeps the trusted boundary small.

## Using Verus for Verification

To verify the Rust code with Verus:

```bash
# Install Verus
git clone https://github.com/verus-lang/verus
cd verus
./tools/get-z3.sh
source ./tools/activate

# Verify PathDB
cd /path/to/axiograph_v6/rust
verus crates/axiograph-pathdb/src/verified.rs
```

Key verified properties:

1. **Probability Invariants**: Values always in [0, 1]
2. **Bitmap Bounds**: IDs never exceed max_id
3. **Path Length**: Concatenation preserves lengths
4. **Modal Consistency**: Accessibility references valid worlds

## Best Practices

### For Developers

1. **Semantics-first**: define meaning in Lean; keep it small and stable
2. **Certificate-first**: make high-value operations emit certificates and treat verification as a production gate
3. **Harden untrusted surfaces**: fuzz parsers/FFI; isolate `unsafe`; use Verus/Kani/Miri where it pays off
4. **Make “unknown” explicit**: separate proposal facts from accepted facts; do not conflate evidence with truth

### For Domain Experts

1. **Define guardrails**: capture safety/policy constraints as rules and tests
2. **Write competency questions (CQs)**: treat them as executable query tests (see `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`)
3. **Demand provenance**: every promoted fact needs sources and context

### For Users

1. **Prefer certified answers**: treat uncertified outputs as suggestions, not truth
2. **Use confidence as evidence strength**: not necessarily “probability of truth” (see `docs/explanation/BOOK.md` §13.4)
3. **Inspect provenance**: ask “why do we believe this?” and follow the certificate chain when stakes are high

## References

- [Verus: Verified Rust for Systems](https://github.com/verus-lang/verus)
- [Roaring Bitmaps](https://roaringbitmap.org/)
