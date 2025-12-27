# Homotopy Type Theory for Knowledge Graphs

**Diataxis:** Explanation  
**Audience:** contributors

> NOTE (Rust+Lean release): the trusted HoTT/groupoid semantics live in Lean (`lean/Axiograph/HoTT/*`).
> Idris snippets in this document are historical notes from an earlier prototype and should be ported to Lean.

This document explains how Axiograph uses HoTT concepts to enable flexible, mathematically rigorous knowledge representation.

## Why HoTT for Knowledge Graphs?

Traditional knowledge graphs treat relationships as static edges. HoTT gives us:

1. **Paths as relationships**: A relationship between A and B is a *path* from A to B
2. **Higher paths**: Relationships between relationships (how friendships evolve)
3. **Equivalence = Identity**: Equivalent structures are interchangeable
4. **Transport**: Data moves along paths (migration)

## Core Concepts

### 1. Identity Types (Paths)

In HoTT, "equality" is replaced by *paths*:

```idris
data Path : a -> a -> Type where
  Refl : Path x x
```

For knowledge graphs:
- `Path Person Person` represents kinship
- `Path Schema Schema` represents schema evolution
- Paths compose: if `p : Path A B` and `q : Path B C`, then `p @@ q : Path A C`

### 2. Higher Paths (2-Paths, 3-Paths, ...)

A 2-path is a path between paths:

```idris
Path2 : Path x y -> Path x y -> Type
Path2 p q = Path p q
```

**Example**: Two ways to derive "cousin":
- Via mother's side: Parent⁻¹ → Mother → Sibling → Child
- Via father's side: Parent⁻¹ → Father → Sibling → Child

Both paths have the same *degree* (4). The fact that they're "the same kinship" is a 2-path!

### 3. Groupoid Structure

When all paths are invertible, we get a *groupoid*:

```idris
record Groupoid where
  Obj : Type
  Hom : Obj -> Obj -> Type
  inv : Hom a b -> Hom b a
  -- inv (inv p) = p
```

**Applications**:
- Social relationships that can be "undone" (friendship → acquaintance → friendship)
- Economic transactions with reversals
- Schema migrations that can rollback

### 4. Univalence

The key insight: **equivalent structures are identical**.

```idris
postulate
ua : Equiv a b -> Path a b
```

For schemas:
```idris
schemaUnivalence : SchemaEquiv s1 s2 -> Path s1 s2
```

This means:
- If two schemas have the same structure, they're interchangeable
- Queries on equivalent schemas give equivalent results
- Data can be freely transported between equivalent representations

## Practical Examples

### Social Networks: 2-Categorical Structure

```
People (0-cells) ─→ Relationships (1-morphisms) ─→ Evolution (2-morphisms)
```

A social network forms a **2-category**:
- Objects: People
- 1-morphisms: Relationships (friend, colleague, family)
- 2-morphisms: How relationships change

```axi
relation RelationshipPath(
  from: Person, to: Person,
  startRel: Stranger,
  endRel: Friend,
  transform: BecameFriends
)
```

The 2-morphisms let us track *how* relationships evolved, not just their current state.

### Economics: Path Independence

Economic flows form a groupoid when transactions are reversible:

```axi
FlowInverse = {
  (flow=Loans, inverse=LoanRepayment),
  (flow=Savings, inverse=Withdrawal)
}
```

**Path Independence**: Two transaction sequences are equivalent if they result in the same economic state.

```axi
PathEquivalence = {
  -- Borrow → Invest → Earn → Repay ≡ Save → Invest → Earn
  (path1=BorrowInvestRepay, path2=SaveInvestEarn, witness=SameNetWorth)
}
```

This is a conservation law expressed as a 2-path!

### Family: Multiple Derivations

Kinship relations form a rich path structure:

```axi
-- Cousin derived two ways
PathEquivalence = {
  (from=Alice, to=Bob,
   path1=MothersCousinPath,
   path2=FathersCousinPath,
   relType=Cousin)
}
```

Different cultures have different equivalences (different "homotopy theories"):

```axi
-- In Hawaiian kinship, cousins ≡ siblings
CulturalEquivalence = {
  (culture=Hawaiian, rel1=Cousin, rel2=Sibling)
}
```

### Schema Evolution: Transport

Schema changes are paths in the space of schemas:

```
ProductV1 ─AddCategories→ ProductV2 ─NormalizeSKU→ ProductV3
              ↓                         ↑
         MergeCategories              JoinSKU (inverse)
```

**Equivalence**: Two normalizations that preserve information:

```axi
SchemaEquiv = {
  (s1=ProductV3, s2=ProductV3_alt,
   forward=V3toV3alt, backward=V3altToV3,
   proof=IsoProof)
}
```

**Transport**: Data migrates along schema paths:

```axi
MigrateData = {
  (migration=V3toV3alt,
   sourceData=Products_Jan2023,
   targetData=Products_Jan2023_migrated)
}
```

## Mathematical Foundations

### The Homotopy Hypothesis

Types behave like topological spaces:
- Points = Values
- Paths = Equalities
- 2-Paths = Homotopies between paths
- etc.

### n-Truncation Levels

| Level | Name | Meaning |
|-------|------|---------|
| -2 | Contractible | Exactly one element |
| -1 | Proposition | At most one element (all equal) |
| 0 | Set | Equality is propositional |
| 1 | 1-Groupoid | Has non-trivial 2-paths |
| n | n-Groupoid | Has structure up to (n+1)-paths |

Knowledge graphs typically live at level 1-2 (interesting 2-paths, less so for 3-paths).

### Kan Extensions

For schema integration, we use *Kan extensions*:

Given schemas A, B and a functor F : A → B, the **left Kan extension** Lan_F gives the "best approximation" of data from A in schema B.

This is how we formally handle:
- Schema merging
- View definitions
- Lossy migrations

## Implementation Notes

### Lean Modules

| Module | Purpose |
|--------|---------|
| `lean/Axiograph/HoTT/Core.lean` | Paths, transport, equivalences (core vocabulary) |
| `lean/Axiograph/HoTT/KnowledgeGraph.lean` | Knowledge-graph paths + equivalence constructors |
| `lean/Axiograph/HoTT/PathAlgebraProofs.lean` | Groupoid laws + normalization/confidence proofs (scaffold) |
| `lean/Axiograph/HoTT/FreeGroupoid.lean` | Bridge to mathlib free-groupoid denotation |

### Postulates

Lean is not cubical and does not provide univalence in the trusted kernel. We treat “univalence-like” behavior as an explicit, certificate-checked notion of equivalence (e.g. schema equivalences/migrations as data + proofs), not as a foundational axiom.

This is safe for knowledge graph reasoning - we're using HoTT as a *design pattern*, not proving theorems about the foundations.

### Practical Use

1. **Model relationships as paths** → Get composition for free
2. **Track relationship evolution** → Use 2-morphisms
3. **Schema migration** → Use equivalences and transport
4. **Multi-perspective reasoning** → Different groupoid structures for different "theories"

## Further Reading

- *Homotopy Type Theory: Univalent Foundations* (The HoTT Book)
- *Category Theory for Scientists* (Spivak) - CQL background
- *Higher Topos Theory* (Lurie) - ∞-categories
- *Seven Sketches in Compositionality* (Fong & Spivak) - Applied category theory
