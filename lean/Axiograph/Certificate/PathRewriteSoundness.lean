import Axiograph.Certificate.Check
import Axiograph.HoTT.FreeGroupoid

/-!
# `Axiograph.Certificate.PathRewriteSoundness`

This file proves **semantic soundness** of the *groupoid/path rewrite rules* used in:

* `normalize_path_v2` (optional derivation replay), and
* `rewrite_derivation_v2`.

The untrusted Rust engine (and certificate emitters) are allowed to be “clever”:
they can normalize, reassociate, cancel inverses, and so on. The trusted Lean
checker replays those steps as *syntax manipulation*.

To ensure that replaying those steps is meaningful, we show that each local rule
preserves denotation in mathlib’s **free groupoid semantics**
(`Mathlib.CategoryTheory.Groupoid.FreeGroupoid`), via the bridge in
`Axiograph.HoTT.FreeGroupoid`.

## Important: well-typedness

Certificate paths (`PathExprV2`) are *untyped* trees. A rewrite rule can succeed
even when endpoints do not line up (because the matcher is purely syntactic).

Therefore, soundness statements are phrased as:

> If the input expression is well-typed (`toTyped` succeeds),
> and the rewrite step succeeds, then the output is well-typed and denotes the
> same morphism in the free groupoid.
-/

namespace Axiograph.Certificate.PathRewriteSoundness

open CategoryTheory
open Axiograph
open Axiograph.HoTT

-- =============================================================================
-- 1) Interpret `PathExprV2` into the typed `HoTT.PathExpr`
-- =============================================================================

abbrev TypedExpr := Axiograph.HoTT.PathExpr
abbrev TypedSigma : Type := Σ a b : Nat, TypedExpr a b

/--
Interpret an untyped certificate path expression into a *typed* expression.

The returned `TypedExpr a b` tracks endpoints in its type, so ill-formed
compositions are rejected at the boundary (with an error string).

This function is intentionally small and explicit: it is part of the trusted
surface area that connects certificates to mathlib semantics.
-/
def toTyped : PathExprV2 → Except String TypedSigma
  | .reflexive entity =>
      .ok ⟨entity, entity, .refl entity⟩
  | .step src relType dst =>
      .ok ⟨src, dst, .step src relType dst⟩
  | .inv path =>
      match toTyped path with
      | .error msg => .error msg
      | .ok ⟨a, b, p⟩ => .ok ⟨b, a, .inv p⟩
  | .trans left right =>
      match toTyped left with
      | .error msg => .error msg
      | .ok ⟨a, b, p⟩ =>
          match toTyped right with
          | .error msg => .error msg
          | .ok ⟨b2, c, q⟩ =>
              if h : b = b2 then
                match h with
                | rfl => .ok ⟨a, c, .trans p q⟩
              else
                .error s!"invalid trans endpoints: left.end={b} right.start={b2}"

def denoteV2 (expr : PathExprV2) : Except String (Σ a b : Nat, (fgObj a ⟶ fgObj b)) := do
  let ⟨a, b, p⟩ ← toTyped expr
  pure ⟨a, b, Axiograph.HoTT.denote p⟩

private def denoteTypedSigma : TypedSigma → Σ a b : Nat, (fgObj a ⟶ fgObj b)
  | ⟨a, b, p⟩ => ⟨a, b, Axiograph.HoTT.denote p⟩

-- =============================================================================
-- Small helper lemmas for `toTyped`
-- =============================================================================

theorem toTyped_trans_ok {left right : PathExprV2} {typed : TypedSigma} :
    toTyped (.trans left right) = .ok typed →
    ∃ a b c, ∃ (p : TypedExpr a b) (q : TypedExpr b c),
      typed = ⟨a, c, .trans p q⟩ ∧
      toTyped left = .ok ⟨a, b, p⟩ ∧
      toTyped right = .ok ⟨b, c, q⟩ := by
  intro h
  cases hLeft : toTyped left with
  | error msg =>
      simp [toTyped, hLeft] at h
  | ok leftTyped =>
      rcases leftTyped with ⟨a, b, p⟩
      cases hRight : toTyped right with
      | error msg =>
          simp [toTyped, hLeft, hRight] at h
      | ok rightTyped =>
          rcases rightTyped with ⟨b2, c, q⟩
          by_cases hb : b = b2
          · cases hb
            simp [toTyped, hLeft, hRight] at h
            cases h
            refine ⟨a, b, c, p, q, rfl, ?_, ?_⟩ <;> simp
          · simp [toTyped, hLeft, hRight, hb] at h

theorem toTyped_inv_ok {path : PathExprV2} {typed : TypedSigma} :
    toTyped (.inv path) = .ok typed →
    ∃ a b, ∃ (p : TypedExpr a b),
      typed = ⟨b, a, .inv p⟩ ∧
      toTyped path = .ok ⟨a, b, p⟩ := by
  intro h
  cases hPath : toTyped path with
  | error msg =>
      simp [toTyped, hPath] at h
  | ok pathTyped =>
      rcases pathTyped with ⟨a, b, p⟩
      simp [toTyped, hPath] at h
      cases h
      refine ⟨a, b, p, rfl, ?_⟩
      simp

theorem toTyped_trans_mk {left right : PathExprV2}
    {a b c : Nat} {p : TypedExpr a b} {q : TypedExpr b c}
    (hLeft : toTyped left = .ok ⟨a, b, p⟩)
    (hRight : toTyped right = .ok ⟨b, c, q⟩) :
    toTyped (.trans left right) = .ok ⟨a, c, .trans p q⟩ := by
  simp [toTyped, hLeft, hRight]

theorem toTyped_inv_mk {path : PathExprV2}
    {a b : Nat} {p : TypedExpr a b}
    (hPath : toTyped path = .ok ⟨a, b, p⟩) :
    toTyped (.inv path) = .ok ⟨b, a, .inv p⟩ := by
  simp [toTyped, hPath]

theorem toTyped_reflexive_endpoints {entity a b : Nat} {p : TypedExpr a b} :
    toTyped (.reflexive entity) = .ok ⟨a, b, p⟩ →
    a = entity ∧ b = entity := by
  intro h
  have h' := h
  simp [toTyped] at h'
  have ha : a = entity := (And.left h').symm
  cases ha
  have heq : (⟨entity, PathExpr.refl entity⟩ : Σ x : Nat, TypedExpr entity x) =
      (⟨b, p⟩ : Σ x : Nat, TypedExpr entity x) := by
    simpa using (eq_of_heq (And.right h'))
  have hb : entity = b := congrArg Sigma.fst heq
  exact ⟨rfl, hb.symm⟩

theorem toTyped_step_endpoints {src relType dst a b : Nat} {p : TypedExpr a b} :
    toTyped (.step src relType dst) = .ok ⟨a, b, p⟩ →
    a = src ∧ b = dst := by
  intro h
  have h' := h
  simp [toTyped] at h'
  have ha : a = src := h'.1.symm
  cases ha
  have heq : (⟨dst, PathExpr.step src relType dst⟩ : Σ x : Nat, TypedExpr src x) =
      (⟨b, p⟩ : Σ x : Nat, TypedExpr src x) := by
    simpa using (eq_of_heq h'.2)
  have hb : dst = b := congrArg Sigma.fst heq
  exact ⟨rfl, hb.symm⟩

-- =============================================================================
-- 2) Soundness of root rewrite rules
-- =============================================================================

theorem applyRule_preserves_denote
    (rule : PathRewriteRuleV2)
    {expr expr' : PathExprV2}
    (hApply : PathNormalization.applyRule rule expr = .ok expr')
    {a b : Nat} {typed : TypedExpr a b}
    (hTyped : toTyped expr = .ok ⟨a, b, typed⟩) :
    ∃ typed' : TypedExpr a b,
      toTyped expr' = .ok ⟨a, b, typed'⟩ ∧
      Axiograph.HoTT.denote typed = Axiograph.HoTT.denote typed' := by
  cases rule with
  | idLeft =>
      cases expr with
      | trans left right =>
          cases left with
          | reflexive entity =>
              -- `applyRule` succeeded, so `expr' = right`.
              have hOk : (.ok right : Except String PathExprV2) = .ok expr' := by
                simpa [PathNormalization.applyRule] using hApply
              have hExpr : expr' = right := by
                injection hOk with h
                exact h.symm
              subst expr'

              -- If the input is well-typed, then the right subexpression is well-typed and has
              -- the same denotation (because the left subexpression denotes the identity).
              rcases toTyped_trans_ok (left := .reflexive entity) (right := right)
                  (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                ⟨a0, b0, c0, p, q, hEq, hLeft, hRight⟩
              cases hEq

              -- From `toTyped (reflexive entity) = ok ⟨a, b0, p⟩` we get `b0 = a` (and `a = entity`).
              have hEndpoints := toTyped_reflexive_endpoints (entity := entity) (a := a) (b := b0) (p := p) hLeft
              have hb0a : b0 = a := hEndpoints.right.trans hEndpoints.left.symm
              cases hb0a

              -- Now `q : TypedExpr a b`, so we can take it as the witness.
              have hp : p = .refl a := by
                have ha : a = entity := hEndpoints.left
                cases ha
                have : PathExpr.refl a = p := by
                  simpa [toTyped] using hLeft
                exact this.symm
              refine ⟨q, (by simpa using hRight), ?_⟩
              -- `typed = trans (refl a) q` after rewriting `p`.
              simp [hp, Axiograph.HoTT.denote]
          | _ =>
              simp [PathNormalization.applyRule] at hApply
      | _ =>
          simp [PathNormalization.applyRule] at hApply
  | idRight =>
      cases expr with
      | trans left right =>
          cases right with
          | reflexive entity =>
              -- `applyRule` succeeded, so `expr' = left`.
              have hOk : (.ok left : Except String PathExprV2) = .ok expr' := by
                simpa [PathNormalization.applyRule] using hApply
              have hExpr : expr' = left := by
                injection hOk with h
                exact h.symm
              subst expr'

              -- If the input is well-typed, then the left subexpression is well-typed and has
              -- the same denotation (because the right subexpression denotes the identity).
              rcases toTyped_trans_ok (left := left) (right := .reflexive entity)
                  (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                ⟨a0, b0, c0, p, q, hEq, hLeft, hRight⟩
              cases hEq
              -- From `toTyped (reflexive entity) = ok ⟨b0, b, q⟩` we get `b0 = b`.
              have hEndpoints := toTyped_reflexive_endpoints (entity := entity) (a := b0) (b := b) (p := q) hRight
              have hb0b : b0 = b := hEndpoints.left.trans hEndpoints.right.symm
              cases hb0b

              have hq : q = .refl b := by
                have hb : b = entity := hEndpoints.right
                cases hb
                have : PathExpr.refl b = q := by
                  simpa [toTyped] using hRight
                exact this.symm
              refine ⟨p, (by simpa using hLeft), ?_⟩
              simp [hq, Axiograph.HoTT.denote]
          | _ =>
              simp [PathNormalization.applyRule] at hApply
      | _ =>
          simp [PathNormalization.applyRule] at hApply
  | assocRight =>
      cases expr with
      | trans left r =>
          cases left with
          | trans p q =>
              have h := hApply
              simp [PathNormalization.applyRule] at h
              cases h
              rcases toTyped_trans_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                ⟨_, _, _, leftT, rT, hEq, hLeft, hR⟩
              cases hEq
              rcases toTyped_trans_ok (typed := ⟨_, _, leftT⟩) hLeft with
                ⟨_, _, _, pT, qT, hEq2, hP, hQ⟩
              cases hEq2
              have hQR : toTyped (.trans q r) = .ok ⟨_, _, .trans qT rT⟩ :=
                toTyped_trans_mk (left := q) (right := r) hQ hR
              have hOut : toTyped (.trans p (.trans q r)) = .ok ⟨a, b, .trans pT (.trans qT rT)⟩ :=
                toTyped_trans_mk (left := p) (right := .trans q r) hP hQR
              refine ⟨.trans pT (.trans qT rT), hOut, ?_⟩
              simp [Axiograph.HoTT.denote, Category.assoc]
          | _ =>
              simp [PathNormalization.applyRule] at hApply
      | _ =>
          simp [PathNormalization.applyRule] at hApply
  | invRefl =>
      cases expr with
      | inv path =>
          cases path with
          | reflexive entity =>
              -- `applyRule` succeeded, so `expr' = reflexive entity`.
              have hOk : (.ok (.reflexive entity) : Except String PathExprV2) = .ok expr' := by
                simpa [PathNormalization.applyRule] using hApply
              have hExpr : expr' = .reflexive entity := by
                injection hOk with h
                exact h.symm
              subst expr'

              -- Decompose the typing of `inv (reflexive entity)`.
              rcases toTyped_inv_ok (path := (.reflexive entity)) (typed := ⟨a, b, typed⟩)
                  (by simpa using hTyped) with
                ⟨a0, b0, p, hEq, hPath⟩
              cases hEq

              -- The inner `reflexive` path forces `a = b` and `p = refl a`.
              have hEndpoints := toTyped_reflexive_endpoints (entity := entity) (a := b) (b := a) (p := p) hPath
              have ha : a = entity := hEndpoints.right
              have hb : b = entity := hEndpoints.left
              cases ha
              cases hb
              have hp : p = .refl a := by
                have : PathExpr.refl a = p := by
                  simpa [toTyped] using hPath
                exact this.symm

              refine ⟨.refl a, by simp [toTyped], ?_⟩
              -- Denotation: `(refl)⁻¹ = refl`.
              simp [hp, Axiograph.HoTT.denote]
          | _ =>
              simp [PathNormalization.applyRule] at hApply
      | _ =>
          simp [PathNormalization.applyRule] at hApply
  | invInv =>
      cases expr with
      | inv p1 =>
          cases p1 with
          | inv p =>
              have h := hApply
              simp [PathNormalization.applyRule] at h
              cases h
              rcases toTyped_inv_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                ⟨_, _, p0, hEq, hPath⟩
              rcases toTyped_inv_ok (typed := ⟨_, _, p0⟩) hPath with
                ⟨_, _, pT, hEq2, hP⟩
              cases hEq2
              cases hEq
              refine ⟨pT, by simpa using hP, ?_⟩
              simp [Axiograph.HoTT.denote]
          | _ =>
              simp [PathNormalization.applyRule] at hApply
      | _ =>
          simp [PathNormalization.applyRule] at hApply
  | invTrans =>
      cases expr with
      | inv p1 =>
          cases p1 with
          | trans p q =>
              have h := hApply
              simp [PathNormalization.applyRule] at h
              cases h
              rcases toTyped_inv_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                ⟨_, _, p0, hEq, hPath⟩
              rcases toTyped_trans_ok (typed := ⟨_, _, p0⟩) hPath with
                ⟨_, _, _, pT, qT, hEq2, hP, hQ⟩
              cases hEq2
              cases hEq
              have hInvQ : toTyped (.inv q) = .ok ⟨_, _, .inv qT⟩ :=
                toTyped_inv_mk (path := q) hQ
              have hInvP : toTyped (.inv p) = .ok ⟨_, _, .inv pT⟩ :=
                toTyped_inv_mk (path := p) hP
              have hOut : toTyped (.trans (.inv q) (.inv p)) = .ok ⟨a, b, .trans (.inv qT) (.inv pT)⟩ :=
                toTyped_trans_mk (left := .inv q) (right := .inv p) hInvQ hInvP
              refine ⟨.trans (.inv qT) (.inv pT), hOut, ?_⟩
              simp [Axiograph.HoTT.denote]
          | _ =>
              simp [PathNormalization.applyRule] at hApply
      | _ =>
          simp [PathNormalization.applyRule] at hApply
  | cancelHead =>
      cases expr with
      | trans atomA rhs =>
          cases rhs with
          | trans atomB rest =>
              -- Pattern: `trans a (trans b rest)` ↦ `rest`
              cases hCond :
                (PathNormalization.isAtom atomA && PathNormalization.isAtom atomB &&
                  PathNormalization.atomsAreInverse atomA atomB) <;>
                (simp [PathNormalization.applyRule, hCond] at hApply)
              -- success branch
              cases hApply
              rcases toTyped_trans_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                ⟨start, mid, end_, aT, rhsT, hEq, hA, hRhs⟩
              cases hEq
              rcases toTyped_trans_ok (typed := ⟨mid, b, rhsT⟩) hRhs with
                ⟨mid2, mid3, end2, bT, restT, hEq2, hB, hRest⟩
              cases hEq2

              -- If `atomA` and `atomB` are inverse atoms, their composition returns to the
              -- original start vertex, so `mid3 = a`.
              have hMid3 : mid3 = a := by
                cases atomA with
                | step src relType dst =>
                    cases atomB with
                    | inv path =>
                        cases path with
                        | step src2 relType2 dst2 =>
                            -- `atomsAreInverse` forces the underlying atoms to match.
                            have hCond' := hCond
                            simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse, Bool.and_eq_true] at hCond'
                            have hSrc : src = src2 := hCond'.1.1
                            have hA' := hA
                            simp [toTyped] at hA'
                            have hSrcA : src = a := hA'.1
                            -- `toTyped (inv (step ...))` flips endpoints, so `mid3 = src2`.
                            rcases toTyped_inv_ok (path := (.step src2 relType2 dst2))
                                (typed := (⟨mid, mid3, bT⟩ : TypedSigma)) (by simpa using hB) with
                              ⟨a0, b0, p0, hEq0, hStep0⟩
                            cases hEq0
                            have hStep0' := hStep0
                            simp [toTyped] at hStep0'
                            -- Combine: `mid3 = src2 = src = a`.
                            exact (hStep0'.1.symm).trans (hSrc.symm.trans hSrcA)
                        | _ =>
                            -- Not an atom: contradicts the success condition.
                            have hCond' := hCond
                            simp [PathNormalization.atomsAreInverse] at hCond'
                    | _ =>
                        have hCond' := hCond
                        simp [PathNormalization.atomsAreInverse] at hCond'
                | inv inner =>
                    cases atomB with
                    | step src relType dst =>
                        cases inner with
                        | step src2 relType2 dst2 =>
                            have hCond' := hCond
                            simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse, Bool.and_eq_true] at hCond'
                            have hDst : dst2 = dst := hCond'.2

                            -- From the inverse typing, `a = dst2`.
                            rcases toTyped_inv_ok (path := (.step src2 relType2 dst2))
                                (typed := (⟨a, mid, aT⟩ : TypedSigma)) (by simpa using hA) with
                              ⟨a0, b0, p0, hEq0, hStep0⟩
                            have hStepEnds :
                                a0 = src2 ∧ b0 = dst2 :=
                              toTyped_step_endpoints (src := src2) (relType := relType2) (dst := dst2)
                                (a := a0) (b := b0) (p := p0) hStep0
                            have ha : a = dst2 := by
                              -- From `⟨a, mid, aT⟩ = ⟨b0, a0, inv p0⟩` we get `a = b0`,
                              -- and from the underlying step typing we get `b0 = dst2`.
                              exact (congrArg Sigma.fst hEq0).trans hStepEnds.2

                            -- From the forward typing, `mid3 = dst`.
                            have hMid3Dst : mid3 = dst :=
                              (toTyped_step_endpoints (src := src) (relType := relType) (dst := dst)
                                (a := mid) (b := mid3) (p := bT) hB).2
                            exact hMid3Dst.trans (ha.trans hDst).symm
                        | _ =>
                            have hCond' := hCond
                            simp [PathNormalization.atomsAreInverse] at hCond'
                    | _ =>
                        have hCond' := hCond
                        simp [PathNormalization.atomsAreInverse] at hCond'
                | reflexive _ =>
                    simp [PathNormalization.isAtom] at hCond
                | trans _ _ =>
                    simp [PathNormalization.isAtom] at hCond
              cases hMid3

              refine ⟨restT, (by simpa using hRest), ?_⟩
              -- Reduce to the two inverse-atom cases.
              cases atomA with
              | step src relType dst =>
                  cases atomB with
                  | inv path =>
                      -- This is the `step.inv` case: `step ; inv(step)` cancels to the identity.
                      cases path with
                      | step src2 relType2 dst2 =>
                          have hCond' := hCond
                          simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse, Bool.and_eq_true] at hCond'
                          have hSrc : src = src2 := hCond'.1.1
                          have hRel : relType = relType2 := hCond'.1.2
                          have hDst : dst = dst2 := hCond'.2

                          -- Rewrite `src`/`dst` so the step matches the typed endpoints.
                          have hAEnds :=
                            toTyped_step_endpoints (src := src) (relType := relType) (dst := dst)
                              (a := a) (b := mid) (p := aT) hA
                          have hSrcA : src = a := hAEnds.1.symm
                          have hDstA : dst = mid := hAEnds.2.symm
                          cases hSrcA
                          cases hDstA

                          -- Rewrite the inverse-atom parameters to match the forward one.
                          cases hSrc
                          cases hRel
                          cases hDst

                          -- `toTyped (step ...)` forces `aT = step ...`.
                          have hATyped : (⟨a, mid, PathExpr.step a relType mid⟩ : TypedSigma) = ⟨a, mid, aT⟩ := by
                            have :
                                Except.ok (ε := String) (α := TypedSigma) (⟨a, mid, PathExpr.step a relType mid⟩ : TypedSigma) =
                                  Except.ok (ε := String) (α := TypedSigma) ⟨a, mid, aT⟩ := by
                              simpa [toTyped] using hA
                            injection this
                          cases hATyped

                          -- `toTyped (inv (step ...))` forces `bT = inv (step ...)`.
                          have hBTyped :
                              (⟨mid, a, PathExpr.inv (PathExpr.step a relType mid)⟩ : TypedSigma) = ⟨mid, a, bT⟩ := by
                            have :
                                Except.ok (ε := String) (α := TypedSigma)
                                    (⟨mid, a, PathExpr.inv (PathExpr.step a relType mid)⟩ : TypedSigma) =
                                  Except.ok (ε := String) (α := TypedSigma) ⟨mid, a, bT⟩ := by
                              simpa [toTyped] using hB
                            injection this
                          cases hBTyped

                          -- Now the cancellation is the groupoid law `p ≫ p⁻¹ = 𝟙`.
                          simp [Axiograph.HoTT.denote]
                      | _ =>
                          simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse] at hCond
                  | _ =>
                      simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse] at hCond
              | inv path =>
                  cases atomB with
                  | step src relType dst =>
                      -- This is the `inv.step` case: `inv(step) ; step` cancels to the identity.
                      cases path with
                      | step src2 relType2 dst2 =>
                          have hCond' := hCond
                          simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse, Bool.and_eq_true] at hCond'
                          have hSrc : src2 = src := hCond'.1.1
                          have hRel : relType2 = relType := hCond'.1.2
                          have hDst : dst2 = dst := hCond'.2

                          -- Rewrite `src`/`dst` so the forward step matches the typed endpoints.
                          have hBEnds :=
                            toTyped_step_endpoints (src := src) (relType := relType) (dst := dst)
                              (a := mid) (b := a) (p := bT) hB
                          have hSrcB : src = mid := hBEnds.1.symm
                          have hDstB : dst = a := hBEnds.2.symm
                          cases hSrcB
                          cases hDstB

                          -- Rewrite the inverse-atom parameters to match the forward one.
                          cases hSrc
                          cases hRel
                          cases hDst

                          -- `toTyped (inv (step ...))` forces `aT = inv (step ...)`.
                          have hATyped :
                              (⟨a, mid, PathExpr.inv (PathExpr.step mid relType a)⟩ : TypedSigma) = ⟨a, mid, aT⟩ := by
                            have :
                                Except.ok (ε := String) (α := TypedSigma)
                                    (⟨a, mid, PathExpr.inv (PathExpr.step mid relType a)⟩ : TypedSigma) =
                                  Except.ok (ε := String) (α := TypedSigma) ⟨a, mid, aT⟩ := by
                              simpa [toTyped] using hA
                            injection this
                          cases hATyped

                          -- `toTyped (step ...)` forces `bT = step ...`.
                          have hBTyped : (⟨mid, a, PathExpr.step mid relType a⟩ : TypedSigma) = ⟨mid, a, bT⟩ := by
                            have :
                                Except.ok (ε := String) (α := TypedSigma) (⟨mid, a, PathExpr.step mid relType a⟩ : TypedSigma) =
                                  Except.ok (ε := String) (α := TypedSigma) ⟨mid, a, bT⟩ := by
                              simpa [toTyped] using hB
                            injection this
                          cases hBTyped

                          simp [Axiograph.HoTT.denote]
                      | _ =>
                          simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse] at hCond
                  | _ =>
                      simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse] at hCond
              | reflexive _ =>
                  simp [PathNormalization.isAtom] at hCond
              | trans _ _ =>
                  simp [PathNormalization.isAtom] at hCond
          | reflexive entity2 =>
              -- `rhs` is not an atom, so the rule cannot apply.
              simp [PathNormalization.applyRule, PathNormalization.isAtom] at hApply
          | step src relType dst =>
              -- Pattern: `trans a b` ↦ `reflexive start`
              cases hCond :
                (PathNormalization.isAtom atomA && PathNormalization.isAtom (.step src relType dst) &&
                  PathNormalization.atomsAreInverse atomA (.step src relType dst)) with
              | false =>
                  simp [PathNormalization.applyRule, hCond] at hApply
              | true =>
                  -- In the success case we must also have `atomStartEntity atomA = some start`.
                  cases hStartVal : PathNormalization.atomStartEntity atomA with
                  | none =>
                      simp [PathNormalization.applyRule, hCond, hStartVal] at hApply
                  | some start =>
                      simp [PathNormalization.applyRule, hCond, hStartVal] at hApply
                      cases hApply

                      rcases toTyped_trans_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                        ⟨_, mid, _, aT, bT, hEq, hA, hB⟩
                      cases hEq

                      -- From the right step typing we recover `b = dst`.
                      have hBEnds :=
                        toTyped_step_endpoints (src := src) (relType := relType) (dst := dst)
                          (a := mid) (b := b) (p := bT) hB
                      have hbDst : b = dst := hBEnds.2

                      -- Since the right atom is a forward step, the only successful cancellation is
                      -- the `inv(step) ; step` case.
                      cases atomA with
                      | reflexive _ =>
                          simp [PathNormalization.isAtom] at hCond
                      | trans _ _ =>
                          simp [PathNormalization.isAtom] at hCond
                      | step _ _ _ =>
                          -- `atomsAreInverse (step ...) (step ...)` is always false.
                          simp [PathNormalization.atomsAreInverse] at hCond
                      | inv inner =>
                          cases inner with
                          | step src2 relType2 dst2 =>
                              have hCond' := hCond
                              simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse, Bool.and_eq_true] at hCond'
                              have hSrc : src2 = src := hCond'.1.1
                              have hRel : relType2 = relType := hCond'.1.2
                              have hDst : dst2 = dst := hCond'.2

                              -- From `atomStartEntity (inv (step ...)) = some start` we get `start = dst2`.
                              have hStartEq : start = dst2 := by
                                -- `atomStartEntity` for an inverse step is the original destination.
                                have :
                                    (some dst2 : Option Nat) = some start := by
                                  simpa [PathNormalization.atomStartEntity] using hStartVal
                                injection this with hEq
                                exact hEq.symm

                              -- From the inverse typing, `a = dst2`.
                              rcases toTyped_inv_ok (path := (.step src2 relType2 dst2))
                                  (typed := (⟨a, mid, aT⟩ : TypedSigma)) (by simpa using hA) with
                                ⟨a0, b0, p0, hEq0, hStep0⟩
                              have hStepEnds :
                                  a0 = src2 ∧ b0 = dst2 :=
                                toTyped_step_endpoints (src := src2) (relType := relType2) (dst := dst2)
                                  (a := a0) (b := b0) (p := p0) hStep0
                              have haDst2 : a = dst2 := by
                                exact (congrArg Sigma.fst hEq0).trans hStepEnds.2

                              have hb : b = a := by
                                exact hbDst.trans (hDst.symm.trans haDst2.symm)
                              have hStartA : start = a := by
                                exact hStartEq.trans haDst2.symm
                              cases hb
                              cases hStartA

                              -- In this case, `expr' = reflexive a`.
                              refine ⟨.refl a, (by simp [toTyped]), ?_⟩
                              -- The denotation is the groupoid law `p⁻¹ ≫ p = 𝟙`.
                              -- Rewrite the forward-step parameters to match the typed endpoints.
                              -- Rewrite inverse-atom parameters to match the right step.
                              cases hSrc
                              cases hRel
                              cases hDst

                              -- From the right step typing we recover `mid = src` and `a = dst`,
                              -- and we rewrite so the inverse atom matches the typed endpoints.
                              have hBEnds :=
                                toTyped_step_endpoints (src := src) (relType := relType) (dst := dst)
                                  (a := mid) (b := a) (p := bT) (by simpa using hB)
                              have hMidSrc : mid = src := hBEnds.1
                              have hADst : a = dst := hBEnds.2
                              cases hMidSrc
                              cases hADst

                              -- `toTyped (inv (step ...))` forces `aT = inv (step ...)` and the endpoints.
                              have hATyped :
                                  (⟨a, src, PathExpr.inv (PathExpr.step src relType a)⟩ : TypedSigma) = ⟨a, src, aT⟩ := by
                                have :
                                    Except.ok (ε := String) (α := TypedSigma)
                                        (⟨a, src, PathExpr.inv (PathExpr.step src relType a)⟩ : TypedSigma) =
                                      Except.ok (ε := String) (α := TypedSigma) ⟨a, src, aT⟩ := by
                                  simpa [toTyped] using hA
                                injection this
                              cases hATyped

                              -- `toTyped (step ...)` forces `bT = step ...` and the endpoints.
                              have hBTyped :
                                  (⟨src, a, PathExpr.step src relType a⟩ : TypedSigma) = ⟨src, a, bT⟩ := by
                                have :
                                    Except.ok (ε := String) (α := TypedSigma)
                                        (⟨src, a, PathExpr.step src relType a⟩ : TypedSigma) =
                                      Except.ok (ε := String) (α := TypedSigma) ⟨src, a, bT⟩ := by
                                  simpa [toTyped] using hB
                                injection this
                              cases hBTyped

                              -- Now we are exactly in the groupoid cancellation case.
                              simp [Axiograph.HoTT.denote]
                          | _ =>
                              -- Not an atom: contradicts the success condition.
                              simp [PathNormalization.isAtom] at hCond
          | inv path =>
              -- Pattern: `trans a b` ↦ `reflexive start`
              cases hCond :
                (PathNormalization.isAtom atomA && PathNormalization.isAtom (.inv path) &&
                  PathNormalization.atomsAreInverse atomA (.inv path)) with
              | false =>
                  simp [PathNormalization.applyRule, hCond] at hApply
              | true =>
                  cases hStartVal : PathNormalization.atomStartEntity atomA with
                  | none =>
                      simp [PathNormalization.applyRule, hCond, hStartVal] at hApply
                  | some start =>
                      simp [PathNormalization.applyRule, hCond, hStartVal] at hApply
                      cases hApply

                      rcases toTyped_trans_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                        ⟨_, mid, _, aT, bT, hEq, hA, hB⟩
                      cases hEq

                      -- Since the right atom is an inverse, the only successful cancellation is
                      -- the `step ; inv(step)` case.
                      cases atomA with
                      | reflexive _ =>
                          simp [PathNormalization.isAtom] at hCond
                      | trans _ _ =>
                          simp [PathNormalization.isAtom] at hCond
                      | inv _ =>
                          -- `atomsAreInverse (inv ...) (inv ...)` is always false.
                          simp [PathNormalization.atomsAreInverse] at hCond
                      | step src relType dst =>
                          cases path with
                          | step src2 relType2 dst2 =>
                              have hCond' := hCond
                              simp [PathNormalization.isAtom, PathNormalization.atomsAreInverse, Bool.and_eq_true] at hCond'
                              have hSrc : src = src2 := hCond'.1.1
                              have hRel : relType = relType2 := hCond'.1.2
                              have hDst : dst = dst2 := hCond'.2

                              have hStartEq : start = src := by
                                have :
                                    (some src : Option Nat) = some start := by
                                  simpa [PathNormalization.atomStartEntity] using hStartVal
                                injection this with hEq
                                exact hEq.symm

                              -- `toTyped (step ...)` forces the typed endpoints and expression.
                              have hATyped : (⟨src, dst, PathExpr.step src relType dst⟩ : TypedSigma) = ⟨a, mid, aT⟩ := by
                                have :
                                    Except.ok (ε := String) (α := TypedSigma) (⟨src, dst, PathExpr.step src relType dst⟩ : TypedSigma) =
                                      Except.ok (ε := String) (α := TypedSigma) ⟨a, mid, aT⟩ := by
                                  simpa [toTyped] using hA
                                injection this
                              -- Keep `hATyped` for endpoint extraction; we rewrite the typed term later.

                              -- `toTyped (inv (step ...))` forces the typed endpoints and expression.
                              have hBTyped :
                                  (⟨dst2, src2, PathExpr.inv (PathExpr.step src2 relType2 dst2)⟩ : TypedSigma) = ⟨mid, b, bT⟩ := by
                                have :
                                    Except.ok (ε := String) (α := TypedSigma)
                                        (⟨dst2, src2, PathExpr.inv (PathExpr.step src2 relType2 dst2)⟩ : TypedSigma) =
                                      Except.ok (ε := String) (α := TypedSigma) ⟨mid, b, bT⟩ := by
                                  simpa [toTyped] using hB
                                injection this
                              -- Keep `hBTyped` for endpoint extraction; we rewrite the typed term later.

                              -- Rewrite the inverse-atom parameters to match the forward one.
                              cases hSrc
                              cases hRel
                              cases hDst
                              -- Rewrite the endpoint witnesses and typed atoms.
                              cases hATyped
                              cases hBTyped
                              cases hStartEq

                              refine ⟨.refl _, (by simp [toTyped]), ?_⟩
                              -- Now we are exactly in the groupoid cancellation case.
                              simp [Axiograph.HoTT.denote]
                          | _ =>
                              -- Not an atom: contradicts the success condition.
                              simp [PathNormalization.isAtom] at hCond
      | _ =>
          simp [PathNormalization.applyRule] at hApply

-- =============================================================================
-- 3) Congruence: applying a rule at a position preserves denotation
-- =============================================================================

theorem applyAt_preserves_denote
    (pos : List Nat)
    (rule : PathRewriteRuleV2)
    {expr expr' : PathExprV2}
    (hApply : PathNormalization.applyAt pos rule expr = .ok expr')
    {a b : Nat} {typed : TypedExpr a b}
    (hTyped : toTyped expr = .ok ⟨a, b, typed⟩) :
    ∃ typed' : TypedExpr a b,
      toTyped expr' = .ok ⟨a, b, typed'⟩ ∧
      Axiograph.HoTT.denote typed = Axiograph.HoTT.denote typed' := by
  induction pos generalizing expr expr' a b typed with
  | nil =>
      have hRule : PathNormalization.applyRule rule expr = .ok expr' := by
        simpa [PathNormalization.applyAt] using hApply
      exact applyRule_preserves_denote rule hRule hTyped
  | cons head rest ih =>
      cases head with
      | zero =>
          cases expr with
          | trans left right =>
              cases hRec : PathNormalization.applyAt rest rule left with
              | error msg =>
                  simp [PathNormalization.applyAt, hRec] at hApply
              | ok left' =>
                  simp [PathNormalization.applyAt, hRec] at hApply
                  cases hApply
                  rcases toTyped_trans_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                    ⟨a0, b0, c0, p, q, hEq, hL, hR⟩
                  cases hEq
                  rcases ih (expr := left) (expr' := left') (a := a) (b := b0) (typed := p)
                    (by simpa using hRec) hL with ⟨p', hLeftTyped, hDenoteLeft⟩
                  have hOut : toTyped (.trans left' right) = .ok ⟨a, b, .trans p' q⟩ :=
                    toTyped_trans_mk (left := left') (right := right) hLeftTyped hR
                  refine ⟨.trans p' q, hOut, ?_⟩
                  simp [Axiograph.HoTT.denote, hDenoteLeft]
          | _ =>
              simp [PathNormalization.applyAt] at hApply
      | succ head' =>
          cases head' with
          | zero =>
              cases expr with
              | trans left right =>
                  cases hRec : PathNormalization.applyAt rest rule right with
                  | error msg =>
                      simp [PathNormalization.applyAt, hRec] at hApply
                  | ok right' =>
                      simp [PathNormalization.applyAt, hRec] at hApply
                      cases hApply
                      rcases toTyped_trans_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                        ⟨a0, b0, c0, p, q, hEq, hL, hR⟩
                      cases hEq
                      rcases ih (expr := right) (expr' := right') (a := b0) (b := b) (typed := q)
                        (by simpa using hRec) hR with ⟨q', hRightTyped, hDenoteRight⟩
                      have hOut : toTyped (.trans left right') = .ok ⟨a, b, .trans p q'⟩ :=
                        toTyped_trans_mk (left := left) (right := right') hL hRightTyped
                      refine ⟨.trans p q', hOut, ?_⟩
                      simp [Axiograph.HoTT.denote, hDenoteRight]
              | _ =>
                  simp [PathNormalization.applyAt] at hApply
          | succ head'' =>
              cases head'' with
              | zero =>
                  cases expr with
                  | inv path =>
                      cases hRec : PathNormalization.applyAt rest rule path with
                      | error msg =>
                          simp [PathNormalization.applyAt, hRec] at hApply
                      | ok path' =>
                          simp [PathNormalization.applyAt, hRec] at hApply
                          cases hApply
                          rcases toTyped_inv_ok (typed := ⟨a, b, typed⟩) (by simpa using hTyped) with
                            ⟨a0, b0, p, hEq, hPathTyped⟩
                          cases hEq
                          rcases ih (expr := path) (expr' := path') (a := b) (b := a) (typed := p)
                            (by simpa using hRec) hPathTyped with ⟨p', hPathTyped', hDenotePath⟩
                          have hOut : toTyped (.inv path') = .ok ⟨a, b, .inv p'⟩ :=
                            toTyped_inv_mk (path := path') hPathTyped'
                          refine ⟨.inv p', hOut, ?_⟩
                          simp [Axiograph.HoTT.denote, hDenotePath]
                  | _ =>
                      simp [PathNormalization.applyAt] at hApply
              | succ _ =>
                  simp [PathNormalization.applyAt] at hApply

end Axiograph.Certificate.PathRewriteSoundness
