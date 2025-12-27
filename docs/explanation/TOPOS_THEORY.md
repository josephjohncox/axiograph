# Topos Theory in Axiograph (Semantics Roadmap)

**Diataxis:** Explanation  
**Audience:** contributors

This document explains how **topos-theoretic semantics** can unify Axiograph’s:

- schemas/types (meta-plane),
- instances/facts (data-plane),
- contexts/worlds (provenance, time, authority, conversation),
- modalities (□/◇, epistemic/deontic),
- and (eventually) probabilistic/approximate reasoning.

It is **explanation-level** (Diataxis): it informs design, but it is not the
trusted kernel.

## 0) Why Topos Theory here?

Axiograph already commits to:

> *Untrusted engines compute; a small trusted checker verifies.*

Topos theory helps by giving a stable, mathlib-backed semantic “spine” for:

- **typed knowledge** without collapsing into closed-world assumptions,
- **“unknown vs false”** as a first-class distinction (intuitionistic logic),
- **contexts/worlds** as a principled indexing of truth,
- and **data migration** (`Δ_F, Σ_F, Π_F`) as canonical categorical constructions.

## 1) Schemas as categories (CQL-style)

Treat a canonical `.axi` schema as presenting a **small category** `C`.

Practical/implementable presentation (fits PathDB today):

- Each **object type** in the schema is an object of `C`.
- Each **relation** is treated as an **edge-object** (an object of `C`),
  with **projection morphisms** to each field’s type:

```
  Flow : Obj
  from : Flow ⟶ Agent
  to   : Flow ⟶ Agent
  ...
```

- Each **subtype declaration** `Sub < Sup` contributes an inclusion arrow:

```
  incl : Sub ⟶ Sup
```

This aligns with the runtime representation:

- PathDB imports n-ary tuples by **reifying** them as fact nodes.
- Field access is an edge `fact -field-> value`.

So the category presentation is not abstract: it is already the *shape* of the
stored graph.

## 2) Instances as functors into finite sets

An instance is a functor:

- `I : C ⥤ FintypeCat` (or `C ⥤ Type` if you want unbounded semantics).

In other words:

- each schema object becomes a finite set of entities,
- each projection arrow becomes a total function selecting that field value.

This is the semantic story that makes:

- schema-directed typechecking,
- schema-directed AxQL elaboration,
- and schema-directed join planning

all “one theory”.

**Mathlib reference:** `Mathlib.CategoryTheory.FintypeCat`.

## 3) Contexts/worlds as (pre)sheaf semantics

Axiograph uses “contexts/worlds” for:

- provenance (source/authority),
- time (“as of t”),
- conversation and policy scopes,
- and staged acceptance (candidate/evidence → accepted).

Topos view:

- Let `W` be a category/poset of contexts (`w' ⟶ w` means “w' refines w”).
- A context-indexed knowledge state is a presheaf:

```
  K : Wᵒᵖ ⥤ (Instances over C)
```

Intuition:

- a fact may be known in a refined context but not globally,
- restriction maps say what information is visible when you weaken/forget context.

### “Unknown vs false”

In a topos (and in presheaf/sheaf topoi), the internal logic is
**intuitionistic** by default:

- you do not get excluded middle for free,
- so “not provable” is not the same as “false”.

This is exactly the distinction we want for knowledge graphs.

**Mathlib reference:** `Mathlib.CategoryTheory.Sites.Grothendieck`,
`Mathlib.CategoryTheory.Sites.Sheaf`.

## 4) Modalities as subtopoi / topologies

Modal operators (□/◇) can be made precise by choosing a topology:

- a **Grothendieck topology** or **Lawvere–Tierney topology** induces a subtopos,
- which induces modal operators on predicates (“truth values”).

Design-level mapping:

- □ (“necessarily”): holds in all refinements / all admissible worlds.
- ◇ (“possibly”): holds in some refinement / some admissible world.

This connects directly to your existing goals:

- epistemic/deontic layers as first-class,
- policy as a world filter,
- and certificate-scoped derivability: “derivable **in context w** under snapshot S”.

## 5) Probabilistic / approximate reasoning (where KL fits)

Topos theory does not force you into a single “probability semantics”.
Two practical tracks for Axiograph:

### 5.1 Markov/stochastic semantics (runtime-friendly)

Treat probabilistic updates as **stochastic maps** (Markov kernels) between
finite sets. Use KL divergence as an optimization/selection criterion during:

- reconciliation (choose a minimally-disruptive merge),
- learning/schema induction (choose the model that best explains evidence),
- drift detection (what changed between contexts/snapshots?).

This is best kept in the **untrusted tooling layer** first:

- emit a report (KL/JS scores),
- keep the certified core about derivability + explicit witnesses,
- later add *bounded* certificates (e.g. “KL ≤ B”) if needed.

### 5.2 “Probability in a topos” (semantics/reference)

For semantics-level documentation and future proof work, mathlib contains the
**Giry monad** (probability distributions on measurable spaces).

This can serve as a reference point even if PathDB stays finite/constructive.

**Mathlib reference:** `Mathlib.MeasureTheory.Measure.GiryMonad`.

## 6) What this means for implementation (near-term)

Near-term changes that make the topos story operational:

1. Keep `.axi` schemas/theories canonical, but ensure the semantics include:
   - relations as edge-objects + projections (already true in PathDB import),
   - explicit contexts (optional-but-suggested).
2. Make context scoping usable everywhere:
   - indexes keyed by context (already present),
   - AxQL/REPL ergonomics (`in {…}` / `ctx use …`) (already present),
   - add tooling to measure “context drift” using KL/JS (planned/implemented in CLI).
3. Keep the trusted checker small:
   - certificate checking stays about definitional well-formedness and replayable derivations,
   - probability analytics stay outside the checker unless we can bound them constructively.

## 7) Pointers in this repo

- Context scoping at runtime:
  - `rust/crates/axiograph-pathdb/src/lib.rs` (`fact_nodes_by_context*`)
  - `rust/crates/axiograph-cli/src/axql.rs` (`in {…}` scoping)
  - `rust/crates/axiograph-cli/src/repl.rs` (`ctx` command)
- Lean probability core (fixed-point): `lean/Axiograph/Prob/Verified.lean`
- Certificate checker entrypoint: `lean/Axiograph/VerifyMain.lean`
