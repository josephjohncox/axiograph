# Mathematical Foundations of Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

> NOTE (Rust+Lean release): Lean4 + mathlib is the trusted checker/spec layer.
> Idris2 snippets in this document are historical notes from an earlier prototype
> and should be read as design sketches to port to Lean.

## Overview

Axiograph is built on a layered mathematical foundation that unifies:

1. **Category Theory** — Schemas as categories, instances as functors
2. **Dependent Type Theory** — Proofs as programs, specifications as types
3. **Homotopy Type Theory** — Paths as equivalences, transport for migration
4. **Modal Logics** — Necessity, knowledge, obligation
5. **Probabilistic Logic** — Uncertainty, evidence, belief update
6. **Temporal Logic** — Time, intervals, change

This document provides the formal foundations and identifies gaps.

---

## 1. Categorical Semantics

### 1.1 Schemas as Finitely Presented Categories

A **schema** $\mathcal{S}$ is a finitely presented category:

$$\mathcal{S} = \langle \text{Obj}, \text{Gen}, \text{Eq} \rangle$$

where:
- $\text{Obj}$ is a finite set of objects (entity types)
- $\text{Gen}$ is a finite set of generating morphisms (relation types)
- $\text{Eq}$ is a finite set of path equations

**Idris Encoding:**

```idris
record Schema where
  constructor MkSchema
  Obj : Type
  Gen : Obj -> Obj -> Type
  Eq  : {a,b : Obj} -> Path Gen a b -> Path Gen a b -> Type
```

### 1.2 Instances as Functors

An **instance** $I : \mathcal{S} \to \mathbf{Set}$ is a functor from the schema to the category of sets:

- Objects map to sets: $I(A) = $ set of entities of type $A$
- Morphisms map to functions: $I(f : A \to B) : I(A) \to I(B)$
- Path equations become commuting diagrams

**Categorical Laws:**
- Identity: $I(\text{id}_A) = \text{id}_{I(A)}$
- Composition: $I(g \circ f) = I(g) \circ I(f)$

### 1.3 Schema Morphisms as Functors

A **schema morphism** $F : \mathcal{S}_1 \to \mathcal{S}_2$ is a functor that:
- Maps objects to objects
- Maps morphisms to paths (composites of morphisms)
- Preserves path equations

This enables **data migration**:
- **Pullback** $\Delta_F : \text{Inst}(\mathcal{S}_2) \to \text{Inst}(\mathcal{S}_1)$ (query)
- **Left adjoint** $\Sigma_F$ (projection/aggregation)
- **Right adjoint** $\Pi_F$ (extension/universal)

### 1.4 The Category of Schemas

Schemas and their morphisms form a 2-category:
- 0-cells: Schemas
- 1-cells: Functors between schemas
- 2-cells: Natural transformations between functors

**Current Implementation:** ✅ Basic schemas and functors
**Missing:** ⚠️ Full 2-categorical structure, natural transformations

---

## 2. Dependent Type Theory

### 2.1 Types as Propositions

Following the Curry-Howard correspondence:

| Logic | Types |
|-------|-------|
| Proposition $P$ | Type $P$ |
| Proof of $P$ | Term $p : P$ |
| $P \land Q$ | $P \times Q$ (pair) |
| $P \lor Q$ | $P + Q$ (sum) |
| $P \Rightarrow Q$ | $P \to Q$ (function) |
| $\forall x. P(x)$ | $(x : A) \to P(x)$ (dependent function) |
| $\exists x. P(x)$ | $(x : A) \times P(x)$ (dependent pair) |

### 2.2 Dependent Types for Constraints

Constraints are encoded as dependent types:

```idris
-- A probability value with proof of validity
record Prob where
  constructor MkProb
  value : Double
  valid : So (value >= 0.0 && value <= 1.0)

-- A path with proof of its length
data PathSig : Nat -> Type where
  PathNil  : PathSig 0
  PathCons : StrId -> PathSig n -> PathSig (S n)

-- Proof that an entity is reachable
data Reachable : EntityId -> PathSig n -> EntityId -> Type where
  ReachRefl : Reachable e PathNil e
  ReachStep : (rel : Relation) -> ... -> Reachable from (PathCons r p) to
```

### 2.3 Refinement Types

Refinement types add predicates to base types:

$$\{ x : A \mid P(x) \}$$

In Axiograph:
```idris
-- Entity with type constraint
ValidEntity : (schema : Schema) -> (e : Entity) -> Type
ValidEntity s e = Elem e.entityType s.Obj

-- Relation with source/target type checking
ValidRelation : (schema : Schema) -> (r : Relation) -> Type
ValidRelation s r = (ValidEntity s r.source, ValidEntity s r.target)
```

**Current Implementation:** ✅ Basic dependent types, `Prob` type
**Missing:** ⚠️ Full refinement type inference, liquid types integration

---

## 3. Homotopy Type Theory (HoTT)

### 3.1 Paths as Identity

In HoTT, identity is not just reflexivity but a full space of paths:

$$\text{Id}_A(x, y) \equiv (x =_A y)$$

**Path Operations:**
- **Reflexivity:** $\text{refl}_x : x = x$
- **Symmetry:** $p^{-1} : y = x$ when $p : x = y$
- **Transitivity:** $p \cdot q : x = z$ when $p : x = y$ and $q : y = z$
- **Congruence:** $\text{ap}_f(p) : f(x) = f(y)$ when $p : x = y$

### 3.2 Transport

Given a type family $P : A \to \mathcal{U}$ and path $p : x = y$:

$$\text{transport}^P(p) : P(x) \to P(y)$$

**Application:** Migrating data along schema equivalences.

```idris
-- Transport properties along schema equivalence
transport : (P : Schema -> Type) -> Equiv s1 s2 -> P s1 -> P s2
```

### 3.3 Equivalence

An **equivalence** between types is a function with a quasi-inverse:

```idris
record Equiv (A B : Type) where
  f : A -> B
  g : B -> A
  eta : (x : A) -> g (f x) = x
  eps : (y : B) -> f (g y) = y
```

### 3.4 Univalence Axiom

The key HoTT principle:

$$(A \simeq B) \simeq (A = B)$$

Equivalent types are equal. This means:
- Schema equivalences become identities
- Data migration is just substitution

**Current Implementation:** ✅ Paths, transport, equivalences (postulated univalence)
**Missing:** ⚠️ Cubical implementation (computational univalence)

### 3.5 Higher Inductive Types

HITs allow path constructors:

```idris
-- Quotient type (set of equivalence classes)
data Quotient : (A : Type) -> (R : A -> A -> Type) -> Type where
  Class : A -> Quotient A R
  Quot  : R x y -> Class x = Class y  -- path constructor
```

**Application:** Defining equivalence classes of entities.

**Current Implementation:** ⚠️ Limited
**Missing:** ❌ Full HIT support, set truncation

---

## 4. Modal Logics

### 4.1 Kripke Semantics

A **Kripke frame** is $(W, R)$ where:
- $W$ is a set of worlds
- $R \subseteq W \times W$ is an accessibility relation

A **Kripke model** adds a valuation $V : W \to \text{Prop} \to \mathbf{2}$.

**Modal Operators:**
- $\Box \phi$ (necessity): $w \models \Box \phi$ iff $\forall v. wRv \Rightarrow v \models \phi$
- $\Diamond \phi$ (possibility): $w \models \Diamond \phi$ iff $\exists v. wRv \land v \models \phi$

### 4.2 Frame Correspondence

| Axiom | Frame Property |
|-------|----------------|
| **T**: $\Box p \to p$ | Reflexive |
| **4**: $\Box p \to \Box\Box p$ | Transitive |
| **B**: $p \to \Box\Diamond p$ | Symmetric |
| **D**: $\Box p \to \Diamond p$ | Serial |
| **5**: $\Diamond p \to \Box\Diamond p$ | Euclidean |

### 4.3 Epistemic Logic

Multi-agent knowledge with operators $K_a$ for each agent $a$:

- $K_a \phi$: Agent $a$ knows $\phi$
- $B_a \phi$: Agent $a$ believes $\phi$
- $C_G \phi$: Common knowledge among group $G$

**Common Knowledge Fixpoint:**
$$C_G \phi = E_G \phi \land E_G(E_G \phi) \land E_G(E_G(E_G \phi)) \land \cdots$$

where $E_G \phi = \bigwedge_{a \in G} K_a \phi$.

### 4.4 Deontic Logic

Normative modalities:
- $O\phi$ (obligatory): $\phi$ ought to be the case
- $P\phi$ (permitted): $\phi$ is allowed
- $F\phi$ (forbidden): $\phi$ is prohibited, $F\phi \equiv O\neg\phi$

**Standard Deontic Logic (SDL):**
- $O(\phi \to \psi) \to (O\phi \to O\psi)$
- $O\phi \to P\phi$ (ought implies may)
- $\neg(O\phi \land O\neg\phi)$ (no conflicts)

**Current Implementation:** ✅ Kripke frames, epistemic, deontic
**Missing:** ⚠️ Dynamic epistemic logic, deontic paradoxes handling

---

## 5. Probabilistic Logic

### 5.1 Probability Theory

The `Prob` type represents probabilities in $[0, 1]$:

```idris
record Prob where
  value : Double
  valid : So (0.0 <= value && value <= 1.0)
```

**Probability Algebra:**
- Complement: $P(\neg A) = 1 - P(A)$
- Independent AND: $P(A \land B) = P(A) \cdot P(B)$
- Independent OR: $P(A \lor B) = P(A) + P(B) - P(A) \cdot P(B)$

### 5.2 Bayesian Reasoning

**Bayes' Theorem:**
$$P(H|E) = \frac{P(E|H) \cdot P(H)}{P(E)}$$

**Evidence Update:**
```idris
bayesUpdate : (priorH : Prob) -> (likelihoodEgivenH : Double) -> 
              (priorE : Prob) -> Prob
```

### 5.3 Uncertain Facts

```idris
record Uncertain (a : Type) where
  fact : a
  confidence : Prob
```

**Composition:**
- Functor: $\text{map } f \text{ (Uncertain } x\text{)} = \text{Uncertain } (f x)$
- Applicative: Combine uncertainties via independent conjunction

### 5.4 Markov Logic Networks (Future)

Combine first-order logic with probabilities:

$$P(\mathbf{x}) = \frac{1}{Z} \exp\left(\sum_i w_i \cdot n_i(\mathbf{x})\right)$$

where $w_i$ are weights and $n_i$ count satisfied ground clauses.

**Current Implementation:** ✅ Prob type, Bayesian update, uncertain facts
**Missing:** ❌ Markov logic networks, probabilistic inference

---

## 6. Temporal Logic

### 6.1 Interval Temporal Logic

Time intervals with Allen's relations:

```idris
data AllenRelation = Before | Meets | Overlaps | Starts | During | Finishes | Equal
```

**Allen's 13 relations** form a complete classification of how two intervals can relate.

### 6.2 Linear Temporal Logic (LTL)

Operators over infinite traces:
- $\bigcirc \phi$ (next): $\phi$ holds at next state
- $\square \phi$ (always): $\phi$ holds at all future states
- $\Diamond \phi$ (eventually): $\phi$ holds at some future state
- $\phi \mathcal{U} \psi$ (until): $\phi$ holds until $\psi$ becomes true

### 6.3 Temporal Knowledge Graphs

Triples with time: $(s, p, o, [t_1, t_2])$

**Temporal Queries:**
- "What was X at time T?"
- "When did X change?"
- "What is the history of X?"

**Current Implementation:** ✅ Intervals, Allen relations, basic LTL
**Missing:** ⚠️ CTL*, timed automata, metric temporal logic

---

## 7. What's Missing: Advanced Topics

### 7.1 Coalgebra and Codata

**Coalgebra** is the dual of algebra, essential for:
- Infinite structures (streams, lazy data)
- Behavioral equivalence (bisimulation)
- State machines and processes

```idris
-- Coalgebraic stream (infinite sequence)
codata Stream a where
  head : Stream a -> a
  tail : Stream a -> Stream a

-- Final coalgebra gives greatest fixpoint
```

**Gap:** ❌ No formal coalgebraic treatment, limited codata support

### 7.2 Topos Theory

A **topos** is a category that behaves like $\mathbf{Set}$:
- Has limits and colimits
- Has exponentials (function types)
- Has a subobject classifier $\Omega$

**Benefits:**
- Internal logic (intuitionistic)
- Sheaves for contextual truth
- Geometric morphisms for data migration

**Gap:** ❌ No topos-theoretic foundation

### 7.3 Sheaf Semantics

**Sheaves** model context-dependent truth:

- Presheaf: $\mathcal{C}^{\text{op}} \to \mathbf{Set}$
- Sheaf: Presheaf satisfying gluing conditions

**Application:** Multi-perspective ontologies where truth varies by context.

**Gap:** ❌ No sheaf implementation

### 7.4 Double Categories

**Double categories** have:
- Horizontal 1-cells
- Vertical 1-cells
- 2-cells filling squares

**Application:** 
- Spans for relational data
- Cospans for processes
- Profunctors for heterogeneous relations

**Gap:** ❌ No double category support

### 7.5 Enriched Categories

Categories enriched over a monoidal category $\mathcal{V}$:
- Hom-sets become $\mathcal{V}$-objects
- Enables weighted/quantitative relations

**Examples:**
- $\mathbf{2}$-enriched: Preorders
- $[0,\infty]$-enriched: Metric spaces (Lawvere)
- $\mathbb{R}_{\geq 0}$-enriched: Weighted graphs

**Application:** Confidence-weighted relations, similarity measures.

**Gap:** ⚠️ Limited to ad-hoc confidence values, no formal enrichment

### 7.6 Linear Logic

**Linear logic** treats propositions as resources:
- $A \otimes B$ (tensor): Both $A$ and $B$
- $A \multimap B$ (lollipop): Consume $A$ to produce $B$
- $!A$ (bang): Unlimited copies of $A$

**Application:** 
- Resource tracking (machine time, materials)
- Session types for protocols

**Gap:** ❌ No linear logic support

### 7.7 Graded Monads

Monads indexed by a monoid:
$$\text{return} : A \to M_1 A$$
$$\text{bind} : M_m A \to (A \to M_n B) \to M_{m \cdot n} B$$

**Application:** Effect tracking, resource usage, confidence composition.

**Gap:** ❌ No graded monad implementation

### 7.8 Cubical Type Theory

Computational interpretation of univalence using:
- Path types as functions from an interval $I$
- Kan operations for composition
- Glue types for equivalences

**Benefits:**
- Univalence computes
- Higher inductive types
- No axioms needed

**Gap:** ❌ Using Book HoTT (postulated), not cubical

---

## 8. Formal Connections

### 8.1 Logic Integration

```
┌─────────────────────────────────────────────────────────────────────┐
│                    LOGIC HIERARCHY                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Dependent Type Theory                                              │
│         │                                                           │
│         ├── Homotopy Type Theory (paths, equivalence)               │
│         │         │                                                 │
│         │         └── ∞-Groupoids (higher paths)                    │
│         │                                                           │
│         ├── Modal Type Theory                                       │
│         │         │                                                 │
│         │         ├── Necessity/Possibility                         │
│         │         ├── Knowledge/Belief                              │
│         │         └── Obligation/Permission                         │
│         │                                                           │
│         ├── Probabilistic Type Theory                               │
│         │         │                                                 │
│         │         └── Bayesian inference, uncertain types           │
│         │                                                           │
│         └── Temporal Type Theory                                    │
│                   │                                                 │
│                   └── LTL, intervals, traces                        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 8.2 Semantic Integration

All modalities interpreted via Kripke-style semantics:
- Worlds indexed by type (time, agent, context)
- Accessibility varies by modality
- Truth relativized to world

**Uniform Framework:**
```idris
-- Generic modal operator over a frame
data Modal : (W : Type) -> (R : W -> W -> Type) -> (P : W -> Type) -> W -> Type where
  Box : ((w' : W) -> R w w' -> P w') -> Modal W R P w
  Diamond : (w' : W ** (R w w', P w')) -> Modal W R P w
```

---

## 9. Implementation Status

| Component | Theory | Implementation | Notes |
|-----------|--------|----------------|-------|
| Category Theory | ★★★★★ | ★★★☆☆ | Basic categories, missing 2-cells |
| Dependent Types | ★★★★★ | ★★★★☆ | Idris 2 provides strong support |
| HoTT | ★★★★★ | ★★★☆☆ | Postulated univalence, no cubical |
| Modal Logic | ★★★★☆ | ★★★★☆ | Kripke, epistemic, deontic |
| Probabilistic | ★★★☆☆ | ★★★☆☆ | Basic Bayesian, no MLN |
| Temporal | ★★★☆☆ | ★★☆☆☆ | Allen intervals, basic LTL |
| Coalgebra | ★★★★☆ | ★☆☆☆☆ | Minimal codata support |
| Topos/Sheaves | ★★★★★ | ☆☆☆☆☆ | Not implemented |
| Linear Logic | ★★★★☆ | ☆☆☆☆☆ | Not implemented |
| Enriched Cat | ★★★★☆ | ★☆☆☆☆ | Ad-hoc confidence only |

---

## 10. Recommended Extensions

### Priority 1: Categorical Completeness
- Natural transformations between functors
- 2-categorical structure for schemas
- Adjunctions for data migration

### Priority 2: Computational HoTT
- Move to cubical type theory (Cubical Agda/Idris)
- Computational univalence
- Full HIT support

### Priority 3: Enriched Categories
- Formal $[0,1]$-enrichment for confidence
- Quantitative relations
- Metric preservation in queries

### Priority 4: Coalgebra
- Coinductive types for streams
- Bisimulation for behavioral equivalence
- Final coalgebras for observations

### Priority 5: Advanced Logics
- Linear logic for resources
- Separation logic for state
- Graded monads for effects

---

## References

1. **Category Theory for Programmers** - Bartosz Milewski
2. **Homotopy Type Theory: Univalent Foundations** - The Univalent Foundations Program
3. **Modal Logic** - Blackburn, de Rijke, Venema
4. **Probabilistic Graphical Models** - Koller, Friedman
5. **Practical Foundations for Programming Languages** - Robert Harper
6. **Seven Sketches in Compositionality** - Fong, Spivak
7. **Category Theory for the Sciences** - Spivak
