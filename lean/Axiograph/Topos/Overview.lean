import Mathlib.CategoryTheory.FintypeCat
import Mathlib.CategoryTheory.Sites.Grothendieck
import Mathlib.CategoryTheory.Sites.Sheaf

/-!
# `Axiograph.Topos.Overview`

This module is an **explanation-level** semantic scaffold: it pins down (in
mathlib terms) how Topos Theory can organize Axiograph’s semantics.

It is intentionally **not** imported by `Axiograph.VerifyMain` (the trusted
checker executable). The checker must stay small and avoid heavy transitive
dependencies that can make platform linking fragile.

See also:
- `docs/explanation/TOPOS_THEORY.md` (repo-level explanation)

## 1. Schemas as categories

Treat a canonical `.axi` schema as presenting a small category `C`:

- object types are objects of `C`,
- relations are *edge-objects* with projection arrows to field types,
- subtyping declarations are inclusion arrows.

This matches PathDB’s storage model: relation tuples are reified as “fact nodes”
and each field becomes an edge `fact -field-> value`.

## 2. Instances as functors into finite sets

An instance is a functor:

`I : C ⥤ FintypeCat`

so each schema object is interpreted as a finite set and each arrow as a
function.

## 3. Contexts/worlds as (pre)sheaves

Let `W` be a category (often a preorder) of contexts. A context-indexed knowledge
state is naturally a presheaf:

`K : Wᵒᵖ ⥤ (C ⥤ FintypeCat)`

and a choice of Grothendieck topology on `W` yields a sheaf semantics and
modal operators.

Mathlib provides the standard foundations for sites and sheaves:

- `Mathlib.CategoryTheory.Sites.Grothendieck`
- `Mathlib.CategoryTheory.Sites.Sheaf`

In future work, we can connect this semantic story to:

- `.axi` context scoping (`@context`, `axi_fact_in_context`),
- policy/world filters in AxQL/REPL,
- and certificate scoping (“derivable in context w under snapshot S”).
-/

namespace Axiograph.Topos

open CategoryTheory

universe u v uW vW

-- =============================================================================
-- Core semantic shapes
-- =============================================================================

/-|
An Axiograph schema interpreted as a category.

This file does not (yet) build categories directly from `.axi` ASTs; it provides
the canonical mathlib types that such a translation should target.
-/
abbrev SchemaCat : Type (u + 1) :=
  Type u

/-|
Instance semantics for a schema-category `C`:

`C ⥤ FintypeCat`
-/
abbrev Instance (C : Type u) [Category.{v} C] :=
  C ⥤ FintypeCat

/-|
Context-indexed knowledge state:

`Wᵒᵖ ⥤ (C ⥤ FintypeCat)`

Read as: for each world/context, we have an instance, and restriction maps
describe how knowledge behaves under context refinement/weakening.
-/
abbrev Knowledge (W : Type uW) [Category.{vW} W] (C : Type u) [Category.{v} C] :=
  Wᵒᵖ ⥤ (C ⥤ FintypeCat)

-- =============================================================================
-- Modalities via sites/sheaves (API touchpoints)
-- =============================================================================

/-|
A Grothendieck topology on a context category `W`.

Choosing such a `J` is one standard way to induce a subtopos of sheaves and
modal operators on predicates.
-/
abbrev ContextTopology (W : Type uW) [Category.{vW} W] :=
  GrothendieckTopology W

/-|
Sheaves of types over a site `(W, J)`.

This is one way to make “unknown vs false” explicit: the internal logic of a
sheaf topos is intuitionistic, so excluded middle is not assumed by default.
-/
abbrev WorldSheaves (W : Type uW) [Category.{vW} W] (J : GrothendieckTopology W) :=
  Sheaf J (Type uW)

end Axiograph.Topos
