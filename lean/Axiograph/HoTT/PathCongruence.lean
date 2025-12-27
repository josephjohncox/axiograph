import Axiograph.HoTT.FreeGroupoid

/-!
# `Axiograph.HoTT.PathCongruence`

This module records “obvious but essential” congruence principles for Axiograph’s
path semantics.

In the Idris2 prototype, these appeared as explicit constructors/lemmas for the
path-equivalence (2-cell) layer: equivalence must be stable under:

* composition (whiskering left/right), and
* inversion.

In Lean we take the denotational route:

* `PathExpr a b` is interpreted as a morphism in mathlib’s
  `Quiver.FreeGroupoid` (see `Axiograph.HoTT.FreeGroupoid`), and
* path equivalence is defined as *equality of denotations*.

Then congruence follows from standard category/groupoid laws in mathlib.

This is the “best in class” way to port the intended semantics while keeping
the trusted surface small and readable.

On the Rust side, these same principles are implemented operationally by
building larger `PathEquivProofV2` certificates from smaller ones (see
`axiograph_pathdb::ProofProducingOptimizer.path_equiv_congr_*_v2`).
-/

namespace Axiograph.HoTT

open CategoryTheory

/-| Denotational path equivalence: two expressions are equivalent when they
denote the same morphism in the free groupoid. -/
def PathExprEquiv {a b : Nat} (p q : PathExpr a b) : Prop :=
  denote p = denote q

theorem pathExprEquiv_refl {a b : Nat} (p : PathExpr a b) : PathExprEquiv p p := by
  rfl

theorem pathExprEquiv_symm {a b : Nat} {p q : PathExpr a b} :
    PathExprEquiv p q → PathExprEquiv q p := by
  intro h
  simpa [PathExprEquiv] using h.symm

theorem pathExprEquiv_trans {a b : Nat} {p q r : PathExpr a b} :
    PathExprEquiv p q → PathExprEquiv q r → PathExprEquiv p r := by
  intro h₁ h₂
  simpa [PathExprEquiv] using Eq.trans h₁ h₂

/-| Left whiskering (post-composition): if `p ≈ q`, then `p · r ≈ q · r`. -/
theorem pathExprEquiv_congr_right {a b c : Nat} {p q : PathExpr a b} (r : PathExpr b c) :
    PathExprEquiv p q → PathExprEquiv (.trans p r) (.trans q r) := by
  intro h
  -- post-compose both sides with `denote r`
  simpa [PathExprEquiv, denote] using congrArg (fun f => f ≫ denote r) h

/-| Right whiskering (pre-composition): if `p ≈ q`, then `r · p ≈ r · q`. -/
theorem pathExprEquiv_congr_left {a b c : Nat} (r : PathExpr a b) {p q : PathExpr b c} :
    PathExprEquiv p q → PathExprEquiv (.trans r p) (.trans r q) := by
  intro h
  -- pre-compose both sides with `denote r`
  simpa [PathExprEquiv, denote] using congrArg (fun f => denote r ≫ f) h

/-| Congruence under inversion: if `p ≈ q`, then `p⁻¹ ≈ q⁻¹`. -/
theorem pathExprEquiv_congr_inv {a b : Nat} {p q : PathExpr a b} :
    PathExprEquiv p q → PathExprEquiv (.inv p) (.inv q) := by
  intro h
  -- `Groupoid.inv` respects equality.
  simpa [PathExprEquiv, denote] using congrArg Groupoid.inv h

end Axiograph.HoTT
