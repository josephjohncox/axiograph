# Mathematical Foundations

**Diataxis:** Explanation  
**Audience:** contributors

Axiograph is a typed ontology engineering system grounded in category theory,
dependent type theory, HoTT/groupoid intuition, and finite operational
verification. This page states the current design target. It is not a history
of earlier proof prototypes.

The implementation spine is:

```text
canonical .axi
  -> KernelModuleIr
  -> SchemaCategoryIr + TheoryIr + InstanceFunctorIr
  -> KernelSurfaceV1 refs
  -> typed runtime reports
  -> optional Lean verifier
```

## 1. Schemas As Categories

A schema is treated as a finite category presentation:

- object types are category objects,
- relation objects are category objects when the relation has identity or
  roles of its own,
- roles are projection arrows from relation objects to endpoint objects,
- ordinary binary aspects are arrows when no relation-object semantics are
  needed,
- subtype inclusions and context maps are typed arrows,
- path equations are theory obligations over parallel paths.

This keeps n-ary relations, business events, provenance, approvals, and process
steps first-class. The relation-as-object form is canonical internally because
it preserves roles and supports olog-style authoring.

## 2. Instances As Functors

Instance data interprets the schema category in a runtime category of finite
sets, typed facts, or indexed evidence objects. Operationally this is
`InstanceFunctorIr`.

For a schema category `S`, an instance functor `I : S -> Set` maps:

- each object type to a finite set of facts/entities,
- each arrow to a total or partial runtime interpretation with explicit
  admissibility,
- each path equation to a runtime theory obligation,
- each context/world axis to an explicit index, not string metadata.

Query results, CQ checks, coverage reports, and backend projections should cite
compiled refs into this functorial surface rather than relying on display names.

## 3. Functorial Data Migration

Schema evolution is modeled as typed transport along schema/category morphisms.
For a morphism `F : S -> T`, the design uses the standard data-migration
intuition:

- `Delta_F` pulls a `T`-instance back to an `S`-instance,
- `Sigma_F` performs left-adjoint migration such as projection or aggregation,
- `Pi_F` performs right-adjoint migration such as extension along missing
  structure.

Axiograph does not currently claim general adjoint completeness for arbitrary
user ontologies. Runtime migration/rebase reports state the finite transport
basis they checked, which refs transported, which refs failed, and which
obligations remain residual.

## 4. Dependent Contexts

Theory objects are indexed by schema, theory, context, world, lifecycle, anchor,
evidence policy, and slice. A useful judgment form is:

```text
Gamma ; World ; Evidence ; Anchor |- obligation : Kind closed_under Fragment
```

Rust uses this as an operational discipline:

- no obligation is checked without anchors and scope,
- no rule can silently cross context/world boundaries,
- unsupported higher-order cases become addressable residual obligations,
- strong reports state the closure tier and non-claims.

Lean is the stronger proof boundary for selected finite fragments.

## 5. HoTT And Groupoid Intuition

Axiograph uses HoTT/groupoid ideas where they are operationally useful:

- paths represent typed semantic composition,
- path witnesses support query and rewrite certificates,
- equivalence motivates transport across schema/category changes,
- higher paths motivate reconciliation of competing derivations,
- transport explains semantic VCS rebase.

Current non-claims are explicit:

- no full univalence claim,
- no full higher inductive type implementation,
- no global ontology closure,
- no proof that every semantic merge has a best join.

The runtime merge lattice is finite and operational: inclusion, overlap,
dependency, conflict, join candidates, and residual obligations over declared
slices.

## 6. Institutions And Boundary Logics

Axiograph uses institutions as the organizing idea for multiple logics. RDF,
OWL, SHACL, property graphs, TypeDB, TerminusDB, and PathDB are boundary
systems with their own syntax and satisfaction relations. They are not the
kernel.

Interop lowers into or projects out of the canonical IR:

- RDF/OWL imports create candidate schema/theory material,
- SHACL-like validation becomes `ValidationShape` and closed-world obligations,
- backend graph databases receive typed projections with trust caveats,
- PathDB is an execution substrate and storage/debug surface.

## 7. DDD, fDDD, And Ologs

Ologs provide the human modeling surface:

- boxes are object types,
- aspects are arrows,
- relation boxes are relation objects with role projections,
- commuting diagrams are path equations,
- candidate axioms become rewrite/equation/constraint obligations.

DDD/fDDD concepts live mostly in tooling overlays:

- bounded contexts are typed semantic slices,
- context maps are schema/category morphisms,
- aggregates and invariants are theory obligations,
- behavior cases are executable CQs/specifications,
- implementation surfaces are coverage targets, not ontology truth.

This separation keeps `.axi` focused on domain representation while authoring,
coverage, and codegen tools use the ontology.

## 8. Soundness And Completeness Claims

Rust runtime checking can claim:

- `well_typed` under exact compiled refs and anchors,
- `admissible` under the supported runtime fragment,
- `closed` under a declared finite closure tier,
- `complete` only when every in-scope obligation is checked or explicitly
  residual.

It cannot claim full model-theoretic completeness, full HoTT, or global
knowledge closure. Lean certificates can strengthen selected claims, but only
for the fragment encoded in the trusted import closure.

## 9. Design Pressure

The mathematical model should reduce cognitive load, not create decorative
abstraction. The practical standard is:

- every category/type-theory concept must improve authoring, checking, merging,
  coverage, codegen, backend projection, or review;
- every strong claim must name its anchors and fragment;
- every weak or evidence-backed output must stay visibly weak;
- every unresolved semantic mismatch should become a typed hole, residual
  obligation, or resolver handle.

For the research-backed version with references, see
`docs/research/APPLIED_CATEGORY_TYPE_THEORY_FOR_AXIograph.md`.
