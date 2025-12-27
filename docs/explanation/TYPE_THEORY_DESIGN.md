# Type Theory Design for Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

> NOTE (Rust+Lean release): Axiograph’s trusted type-theoretic foundation is now **Lean4 + mathlib**.
> Idris2 was used historically as a prototype proof layer and has been removed from the repo tree.
> Any `idris` snippets below are historical design notes that should be ported/updated to Lean.

## Core Type System

Axiograph uses **Lean 4 + mathlib** as its type-theoretic foundation for the trusted semantics and certificate checking, providing:

- Dependent types
- First-class types
- Totality checking
- Proof irrelevance (where appropriate)
- Mathlib’s category theory / algebra libraries for “best in class” formalizations

This document details the type-theoretic design choices and extensions.

---

## 1. Universe Hierarchy

### Current Approach

Idris uses `Type` with implicit universe polymorphism:

```idris
-- Schema objects live in Type
record Schema where
  Obj : Type
  Gen : Obj -> Obj -> Type
```

### Missing: Explicit Universes

For large eliminations and set-theoretic concerns, we need:

```idris
-- Proposed: Explicit universe levels
record Schema (ℓ : Level) where
  Obj : Type ℓ
  Gen : Obj -> Obj -> Type ℓ
  Eq  : Path Gen a b -> Path Gen a b -> Type ℓ

-- Large schemas containing schemas
record MetaSchema where
  schemas : Type 1  -- Universe of schemas
  mappings : schemas -> schemas -> Type 0
```

**Status:** ⚠️ Implicit only, no `Type ω` or universe polymorphism

---

## 2. Inductive Families

### Well-Founded Induction

Path signatures indexed by length:

```idris
data PathSig : Nat -> Type where
  PathNil  : PathSig 0
  PathCons : StrId -> PathSig n -> PathSig (S n)
```

This ensures:
- Length is tracked at type level
- Operations preserve length invariants
- Termination is guaranteed

### Indexed Inductive Types

Reachability as an inductive family:

```idris
data Reachable : EntityId -> PathSig n -> EntityId -> Type where
  ReachRefl : Reachable e PathNil e
  ReachStep : (rel : Relation) ->
              rel.source = from ->
              rel.relType = r ->
              Reachable rel.target p to ->
              Reachable from (PathCons r p) to
```

**Properties verified by construction:**
- Reflexivity for empty paths
- Step-wise composition
- Source/target consistency

---

## 3. Proof Relevance vs Irrelevance

### Proof-Relevant Data

When proofs carry computational content:

```idris
-- The proof IS the data
data Reachable : EntityId -> PathSig n -> EntityId -> Type where
  -- Constructors contain the actual path taken
```

### Proof-Irrelevant Propositions

When we only care that a proof exists:

```idris
-- Using So for decidable propositions
record Prob where
  value : Double
  valid : So (in01 value)  -- Proof irrelevant, erased at runtime

-- Using erased quantification
erased
validPath : (p : PathSig n) -> ValidPath p
```

### Missing: Full Prop/Set Distinction

HoTT distinguishes:
- **Prop** (h-propositions): At most one proof
- **Set** (h-sets): No higher path structure
- **Groupoid**: 2-paths exist
- **∞-Groupoid**: All levels

```idris
-- Proposed: Truncation levels
data TruncLevel = Prop | Set | Groupoid | Inf

isProp : Type -> Type
isProp A = (x, y : A) -> x = y

isSet : Type -> Type  
isSet A = (x, y : A) -> isProp (x = y)
```

**Status:** ⚠️ Partial (some truncation), needs systematic treatment

---

## 4. Record Types and Copatterns

### Positive Records

Standard record types:

```idris
record Entity where
  constructor MkEntity
  entityId : EntityId
  entityType : StrId
  attrs : List (StrId, StrId)
```

### Missing: Copatterns

For defining coinductive types and infinite structures:

```idris
-- Proposed: Copatterns for streams
codata Stream a where
  head : a
  tail : Stream a

-- Definition by observation
nats : Stream Nat
head nats = 0
tail nats = map S nats
```

**Status:** ❌ Idris 2 has limited codata; no copatterns

---

## 5. Effect System

### Current: IO and Monad Transformers

```idris
loadPathDB : String -> IO (Either Error PathDB)
```

### Missing: Algebraic Effects

More principled effect handling:

```idris
-- Proposed: Effect signature
effect PathDBOps where
  getEntity : EntityId -> Eff (Maybe Entity)
  followRel : EntityId -> StrId -> Eff (List EntityId)

-- Effect handlers
runInMemory : Eff a [PathDBOps] -> PathDB -> (a, PathDB)
runDistributed : Eff a [PathDBOps] -> NetworkConfig -> IO a
```

### Missing: Graded Monads

Track effect accumulation:

```idris
-- Proposed: Confidence-graded computation
data Uncertain : Prob -> Type -> Type where
  Pure : a -> Uncertain 1.0 a
  Bind : Uncertain p a -> (a -> Uncertain q b) -> Uncertain (p * q) b
```

**Status:** ⚠️ Basic monads only, no algebraic effects or grading

---

## 6. Subtyping and Coercion

### Current: Explicit Conversion

```idris
entityToNode : Entity -> Node
entityToNode e = MkNode e.entityId (show e.entityType)
```

### Missing: Subtype Polymorphism

```idris
-- Proposed: Subtype relation
interface (:<) (a : Type) (b : Type) where
  upcast : a -> b

-- Material is a subtype of Entity
Material :< Entity where
  upcast m = MkEntity m.id "Material" m.attrs

-- Contravariant for inputs
validFor : (e : Entity) -> {auto prf : Material :< Entity} -> Bool
```

**Status:** ❌ No subtyping, manual conversions

---

## 7. Linear and Affine Types

### Missing: Resource Tracking

```idris
-- Proposed: Linear types
data Linear : Type -> Type where
  MkLinear : (1 _ : a) -> Linear a

-- Affine (use at most once)
data Affine : Type -> Type where
  MkAffine : (0..1 _ : a) -> Affine a

-- Session types for protocols
data Session : Protocol -> Type where
  Send : (a -> Session s) -> Session (Send a :> s)
  Recv : (a -> Session s) -> Session (Recv a :> s)
  End  : Session Done
```

**Application:** 
- Machine time allocation
- Material consumption in manufacturing
- One-time tokens in security

**Status:** ❌ No linear/affine types in Idris 2

---

## 8. Observational Type Theory

### Missing: Strict Equality with Univalence

Observational Type Theory (OTT) provides:
- Definitional equality for canonical forms
- Propositional equality with UIP (unique identity proofs)
- Function extensionality
- Compatible with classical reasoning

```idris
-- Proposed: Observational equality
data Obs : a -> a -> Type where
  -- For functions, pointwise
  FunObs : ((x : a) -> Obs (f x) (g x)) -> Obs f g
  -- For records, field-wise
  RecObs : Obs r1.field r2.field -> ... -> Obs r1 r2
```

**Status:** ❌ Using intensional equality

---

## 9. Type-Level Computation

### Current: Compile-Time Evaluation

```idris
-- Type-level natural number arithmetic
pathConcat : PathSig m -> PathSig n -> PathSig (m + n)
```

### Missing: Type Families with Overlap

```idris
-- Proposed: Overlapping type families
type family Merge (a : Schema) (b : Schema) : Schema where
  Merge a a = a  -- Overlap: identical schemas
  Merge a b = UnionSchema a b  -- General case
```

**Status:** ⚠️ Interface resolution, no true type families

---

## 10. Reflection and Metaprogramming

### Current: Elaborator Reflection

Idris 2 provides `%macro` and elaborator scripts:

```idris
%macro
deriveShow : (name : Name) -> Elab ()
deriveShow n = do
  -- Generate Show instance
```

### Usage in Axiograph

```idris
-- Auto-generate schema validators
%runElab deriveSchemaValidator "MySchema"

-- Generate migration functions
%runElab deriveMigration "SchemaV1" "SchemaV2"
```

**Status:** ✅ Elaborator reflection available

---

## 11. Proposed Type System Extensions

### 11.1 Quantitative Type Theory (QTT)

Track resource usage at type level:

```idris
-- 0: erased at runtime
-- 1: used exactly once
-- ω: unrestricted

swap : (1 a : Type) -> (1 b : Type) -> (1 x : a) -> (1 y : b) -> (b, a)
swap _ _ x y = (y, x)
```

**Benefit:** Proves linear resource usage

### 11.2 Sized Types

Ensure termination for recursive definitions:

```idris
-- Size-indexed types
data Nat : Size -> Type where
  Z : Nat i
  S : Nat i -> Nat (↑ i)

-- Sized streams
codata Stream : Size -> Type -> Type where
  head : Stream i a -> a
  tail : Stream (↑ i) a -> Stream i a
```

**Benefit:** Coinductive definitions with guaranteed productivity

### 11.3 Self Types

Types that can refer to their own values:

```idris
-- Self type for intrinsic invariants
Self : (A : Type) -> (A -> Type) -> Type
Self A P = (x : A) ** P x

-- Entity that knows its own type
SelfTypedEntity : Type
SelfTypedEntity = Self Entity (\e => ValidType e.entityType)
```

**Benefit:** More expressive invariants

---

## 12. Verified Compilation Target

### Idris → Verified Rust

The compilation pipeline should preserve:

1. **Type Safety:** Rust's ownership = linear subset
2. **Invariants:** Verus annotations for runtime checks
3. **Proofs:** Extract to SMT verification

```
Idris Source
    ↓ type check
Idris Core (TT)
    ↓ erase proofs
Erased Core
    ↓ emit Rust
Verified Rust + Verus Annotations
    ↓ verify
SMT Proof Obligations
    ↓ compile
Native Binary
```

**Status:** ⚠️ Partial (emit Rust, Verus stubs)

---

## 13. Summary of Gaps

| Feature | Importance | Difficulty | Status |
|---------|------------|------------|--------|
| Explicit universes | High | Medium | ⚠️ Implicit |
| Copatterns | High | High | ❌ Missing |
| Algebraic effects | High | High | ❌ Missing |
| Linear types | Medium | High | ❌ Missing |
| Graded monads | Medium | Medium | ❌ Missing |
| Full truncation | Medium | Medium | ⚠️ Partial |
| Subtyping | Low | Medium | ❌ Missing |
| Sized types | Medium | High | ❌ Missing |
| Cubical TT | High | Very High | ❌ Postulated |

---

## 14. Recommended Path Forward

### Phase 1: Strengthen Current Foundation
- Complete `Prop`/`Set` distinction
- Add systematic proof irrelevance
- Improve elaborator scripts for automation

### Phase 2: Add Coinduction
- Implement basic codata with copatterns
- Add productivity checking
- Support for infinite structures

### Phase 3: Effect System
- Design algebraic effect signatures
- Implement effect handlers
- Add graded monad support for confidence tracking

### Phase 4: Advanced Features
- Explore cubical type theory integration
- Add sized types for termination
- Consider QTT for resource tracking

---

## References

1. **Type Theory and Functional Programming** - Simon Thompson
2. **Programming in Martin-Löf's Type Theory** - Nordström, Petersson, Smith
3. **Quantitative Type Theory** - Atkey, 2018
4. **Cubical Agda** - Vezzosi, Mörtberg, Abel
5. **Idris 2: Quantitative Type Theory in Practice** - Brady
