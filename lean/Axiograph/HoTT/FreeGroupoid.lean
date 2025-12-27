import Mathlib.CategoryTheory.Groupoid.FreeGroupoid

/-!
# Free groupoid semantics for Axiograph paths (mathlib-backed)

Section 3 of `docs/explanation/BOOK.md` describes “paths + groupoids + rewriting” as a
foundational semantics layer.

Lean already has a high-quality implementation of the **free groupoid on a
quiver** in mathlib:

- `Mathlib.CategoryTheory.Groupoid.FreeGroupoid`

This module provides a small, auditable bridge:

* a *typed* path expression language with inverses (`PathExpr`),
* a denotation function into mathlib’s `Quiver.FreeGroupoid`,
* and the core groupoid equalities we want certificates to rely on.

We keep this separate from the runtime/certificate formats:

* Certificates carry untrusted *data* (`PathExprV2` in `Axiograph.Certificate.Format`).
* The trusted checker can interpret that data into a typed `PathExpr` and then
  reason using mathlib’s groupoid laws.
-/

namespace Axiograph.HoTT

open CategoryTheory

-- =============================================================================
-- A very small “quiver of labeled edges” on `Nat`
-- =============================================================================

/-!
For certificate checking we want a semantics that does **not** depend on any
particular in-memory graph representation.

We therefore use a generic quiver whose:

* objects are entity identifiers (`Nat`), and
* arrows `src ⟶ dst` are just *relation labels* (`Nat`).

This is a “pure syntax” quiver: it says “there is an edge labeled `r` from
`src` to `dst`” for all `src`, `dst`, and `r`. This is enough to model the
*groupoid laws* (identity/associativity/inverses) that are independent of any
domain theory.

Domain-specific rewriting (unit conversions, schema migration, etc.) should be
added on top as explicit rewrite rules/certificates.
-/

instance : Quiver Nat where
  Hom _ _ := Nat

abbrev FreeGroupoidNat : Type := Quiver.FreeGroupoid Nat

def fgObj (entity : Nat) : FreeGroupoidNat :=
  (Quiver.FreeGroupoid.of Nat).obj entity

def fgStep (src : Nat) (relType : Nat) (dst : Nat) : (fgObj src ⟶ fgObj dst) :=
  (Quiver.FreeGroupoid.of Nat).map (X := src) (Y := dst) (relType : Nat)

-- =============================================================================
-- Typed path expressions (free groupoid syntax)
-- =============================================================================

/-!
`PathExpr a b` is a *typed* syntax tree for paths from `a` to `b`.

This is the shape we ultimately want to reason about in Lean: endpoints are
tracked by the type, so ill-formed compositions are not constructible.
-/

inductive PathExpr : Nat → Nat → Type where
  | refl (a : Nat) : PathExpr a a
  | step (src : Nat) (relType : Nat) (dst : Nat) : PathExpr src dst
  | trans {a b c : Nat} (left : PathExpr a b) (right : PathExpr b c) : PathExpr a c
  | inv {a b : Nat} (p : PathExpr a b) : PathExpr b a
  deriving Repr

-- =============================================================================
-- Denotation into mathlib’s free groupoid
-- =============================================================================

/-!
Interpret a `PathExpr` as a morphism in the free groupoid.

Because `FreeGroupoidNat` is a genuine `Groupoid`, we get:

* associativity and identity “for free” from the `Category` instance,
* inverse laws from the `Groupoid` instance.
-/

def denote : {a b : Nat} → PathExpr a b → (fgObj a ⟶ fgObj b)
  | _, _, .refl a => 𝟙 (fgObj a)
  | _, _, .step src relType dst => fgStep src relType dst
  | _, _, .trans left right => denote left ≫ denote right
  | _, _, .inv p => Groupoid.inv (denote p)

-- =============================================================================
-- Core groupoid equalities (used by rewrite/certificate checking)
-- =============================================================================

theorem denote_id_left {a b : Nat} (p : PathExpr a b) :
    denote (.trans (.refl a) p) = denote p := by
  simp [denote]

theorem denote_id_right {a b : Nat} (p : PathExpr a b) :
    denote (.trans p (.refl b)) = denote p := by
  simp [denote]

theorem denote_assoc {a b c d : Nat} (p : PathExpr a b) (q : PathExpr b c) (r : PathExpr c d) :
    denote (.trans (.trans p q) r) = denote (.trans p (.trans q r)) := by
  simp [denote, Category.assoc]

theorem denote_inv_left {a b : Nat} (p : PathExpr a b) :
    denote (.trans p (.inv p)) = denote (.refl a) := by
  -- `simp` uses the groupoid law `p ≫ p⁻¹ = 𝟙`.
  simp [denote]

theorem denote_inv_right {a b : Nat} (p : PathExpr a b) :
    denote (.trans (.inv p) p) = denote (.refl b) := by
  simp [denote]

end Axiograph.HoTT
