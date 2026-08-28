# Applied Category And Type Theory For Axiograph

This note pins the runtime theory-checker design to a narrow, useful
mathematical contract. Axiograph should use category theory, dependent type
theory, and HoTT-inspired path semantics to make ontology engineering
operational, but it should not claim a full HoTT/topos/dependently typed kernel
until that fragment is formally implemented and checked.

## Core Model

The accepted meaning plane is canonical `.axi` plus compiled semantic IR.
External stores and interchange formats are projections of that meaning, not the
kernel.

The categorical core is:

```text
SchemaPresentationIr S
  objects  : object types and relation objects
  arrows   : ordered role projections, subtypes, aspects, and functions

TypedTheoryIr T over S
  constraints, path equations, rewrites, obligations

InstanceModelIr I (intended as I : S -> FinSet)
  carriers        : finite sets of entities/facts
  interpretations : total generator maps checked over the supported fragment
```

Relation-as-object is canonical. A binary or n-ary relation is represented as an
object `R` with projection arrows:

```text
R --role_1--> A_1
R --role_2--> A_2
...
R --role_n--> A_n
```

This keeps attributes, provenance, evidence, context, temporal axes, and
business roles attached to the relation object rather than erased into a loose
edge label. Subtype inclusions are arrows `A_sub -> A_super`, not comments on
names. A well-typed path is a composable arrow sequence in the compiled schema
category, with explicit context/world/temporal side conditions when those axes
are present.

## Functorial Data Migration

For a schema morphism `F : S -> T`, functorial data migration gives three
canonical operations between instance categories:

```text
Delta_F : Inst(T) -> Inst(S)     pullback/reindexing
Sigma_F : Inst(S) -> Inst(T)     left Kan-style migration
Pi_F    : Inst(S) -> Inst(T)     right Kan-style migration
```

Axiograph should treat migration, rebase, and merge as transport problems over
typed schema/theory morphisms:

- Querying a slice under another schema is `Delta_F`-like reindexing.
- Pushing accepted facts and obligations forward is `Sigma_F`-like migration.
- Computing required compatible target structure is `Pi_F`-like completion.

Runtime transport is not proof by default. A transport report must classify each
obligation as preserved, transported, missing-object-image, missing-arrow-image,
opaque/out-of-fragment, or blocked. Lean certification can later prove selected
transport laws for narrow path/rewrite fragments.

## Ologs As Authoring Surface

Ologs are the right human-facing surface for this system because they are
category-theoretic but domain-authorable:

- boxes map to object types,
- aspects map to arrows,
- relation boxes map to relation objects,
- commuting diagrams map to path equations,
- olog alignments map to functors/schema morphisms,
- candidate axioms map to constraints, rewrites, or review-only obligations.

The runtime checker should make olog authoring typed and actionable: failed
diagrams become typed holes or refinement handles, not unstructured prose.

## Dependent Contexts

The checker should use a categories-with-families style discipline for
context-indexed ontology work. The operational analogy is:

```text
Gamma             context of known object/relation/theory ids
Ty(Gamma)         types available under that context
Tm(Gamma, A)      terms/facts/paths/rules of type A under Gamma
Gamma.A           context extension with a new typed assumption
```

Axiograph obligations are indexed by schema, theory, world, evidence policy,
anchor, lifecycle state, and semantic slice:

```text
Gamma ; W ; E ; A |- obligation : Kind closed_under Fragment
```

Meaning:

- `Gamma` is the typed semantic context from compiled IR.
- `W` is the declared world/context universe.
- `E` is the evidence policy.
- `A` is the accepted anchor/ref/slice set.
- `Kind` classifies the obligation as constraint, equation, rewrite, transport,
  CQ/spec, or business invariant.
- `Fragment` is the closure tier and supported runtime fragment.

This is the useful dependent-typing effect: artifacts are not well-scoped unless
their indices are explicit.

## HoTT And Groupoid Semantics

The current sound center is path/groupoid semantics, not full HoTT. A path
witness can show that two typed routes have the same endpoints and are related
by accepted rewrites/equations in the supported fragment:

```text
p : x =_A y
transport(P, p, u) : P(y)
```

For Axiograph, transport means moving facts, obligations, paths, or CQ witnesses
across an accepted equivalence or schema morphism when the runtime/Lean fragment
supports that move.

Non-claims:

- The runtime checker does not prove univalence.
- It does not make arbitrary ontology equivalence decidable.
- It does not prove global ontology closure.
- It does not certify higher-order rewrites unless exported to and accepted by a
  trusted Lean checker for that fragment.

## Institutions And Boundary Logics

Axiograph has to interact with RDF, OWL, SHACL, property graphs, TypeDB, and
TerminusDB without letting any boundary logic become the kernel. Institutions
give the right abstraction: signatures, sentences, models, and satisfaction,
plus a satisfaction condition under change of notation.

The design rule is:

- Import/export formats may supply signatures and candidate sentences.
- Axiograph lowers accepted material into canonical `.axi` and compiled IR.
- Trust contracts record which satisfaction relation is being used.
- Cross-logic claims stay scoped and non-global unless a checked institution
  morphism or certified translation exists.

## RDF And SHACL

RDF-style semantics are useful for open-world interop and exploratory knowledge
integration. Absence of a triple is not falsity. That aligns with evidence-plane
ontology discovery.

SHACL-style validation is useful for closed-world gates: ingestion checks,
promotion, business-rule enforcement, migration readiness, release readiness,
and implementation conformance. Axiograph should import SHACL-like shapes into
typed validation obligations, not treat SHACL as the ontology kernel.

## DDD And fDDD

DDD and functional DDD are operationally important because ontology changes must
track software and business change.

The mapping is:

- bounded context -> typed semantic slice,
- context map -> schema/theory morphism,
- aggregate invariant -> theory obligation,
- domain event -> typed relation object or transition fact,
- behavior case -> executable CQ/spec obligation,
- anti-corruption layer -> explicit translation morphism with trust caveats.

This makes merge and rebase business-aware. A semantic merge should not only say
"two files changed"; it should say which bounded-context obligations, CQs,
aggregates, and implementation surfaces are preserved, weakened, blocked, or
require resolver steps.

## Runtime Soundness And Completeness

Runtime checker soundness means:

```text
If the checker emits `checked` for obligation O under Gamma, W, E, A, Fragment,
then O is well-scoped, endpoint-safe, lifecycle-valid, and admissible in the
declared runtime fragment.
```

It does not mean Lean-certified proof unless the trust contract says so.

Runtime completeness is tiered:

- `finite_fragment`: complete only over finite accepted worlds, terminating
  supported rules, explicit contexts/worlds, and known imports.
- `evidence_weighted`: complete only after thresholding the declared evidence
  world, with optional semiring/lattice propagation when implemented.
- `global_indexed`: complete only over a finite declared VCS/world/slice/import
  universe. It is not arbitrary global ontology truth.

Closure means saturation to a fixpoint for the supported fragment or an explicit
blocking/residual report explaining why closure was not reached.

## References

- David I. Spivak and Robert E. Kent, "Ologs: a categorical framework for knowledge representation", <https://arxiv.org/abs/1102.1889>.
- David I. Spivak, "Functorial Data Migration", <https://arxiv.org/abs/1009.1166>.
- Ryan Wisnesky, David Spivak, et al., "Functorial Data Migration: From Theory to Practice", <https://arxiv.org/abs/1502.05947>.
- Brendan Fong and David I. Spivak, "Seven Sketches in Compositionality", <https://arxiv.org/abs/1803.05316>.
- Simon Castellan, Pierre Clairambault, and Peter Dybjer, "Categories with Families: Unityped, Simply Typed, and Dependently Typed", <https://arxiv.org/abs/1904.00827>.
- Stanford Encyclopedia of Philosophy, "Intuitionistic Type Theory", <https://plato.stanford.edu/entries/type-theory-intuitionistic/>.
- The Univalent Foundations Program, "Homotopy Type Theory: Univalent Foundations of Mathematics", <https://homotopytypetheory.org/book/>.
- Joseph A. Goguen and Rod Burstall, "Institutions: Abstract Model Theory for Specification and Programming", <https://www.lfcs.inf.ed.ac.uk/reports/90/ECS-LFCS-90-106/>.
- W3C, "RDF 1.1 Semantics", <https://www.w3.org/TR/rdf11-mt/>.
- W3C, "Shapes Constraint Language (SHACL)", <https://www.w3.org/TR/shacl/>.
- Martin Fowler, "Bounded Context", <https://martinfowler.com/bliki/BoundedContext.html>.
- Scott Wlaschin, "Domain Modeling Made Functional", <https://pragprog.com/titles/swdddf/domain-modeling-made-functional/>.
