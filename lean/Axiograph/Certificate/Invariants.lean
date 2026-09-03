import Axiograph.Certificate.PathRewriteSoundness

/-!
# `Axiograph.Certificate.Invariants`

This module proves **invariants** about the runtime witness structures that
Rust emits as certificates.

These theorems are intended to support the project’s “untrusted engine,
trusted checker” architecture:

- Rust is allowed to compute results using complex algorithms and optimizations.
- Lean checks small certificates and we prove that “checker accepts” implies
  meaningful semantic properties.

Scope (today)
-------------

We focus on the core witness kinds that appear in end-to-end flows:

* normalization and path-equivalence witnesses (`normalize_path_v2`,
  `path_equiv_v2`)
* internal builtin `RewriteDerivationProofV2` replay helpers used by path equivalence
* reconciliation decisions (`resolution_v2`)

Anchored domain-rule steps in `rewrite_derivation_v3` are outside these
free-groupoid soundness theorems.

This file does **not** aim to prove completeness of any algorithm (“all answers”),
only soundness-style invariants (“these witnesses are internally consistent and
mean what they claim”).
-/

namespace Axiograph.Certificate.Invariants

open Axiograph

-- =============================================================================
-- Reconciliation (resolution) witnesses
-- =============================================================================

namespace ResolutionInvariants

open Axiograph.Resolution

theorem verifyResolutionProofV2_ok_matches_decision
    (proof : ResolutionProofV2)
    (result : ResolutionResultV2) :
    verifyResolutionProofV2 proof = .ok result →
      result.decision = Prob.decideResolution proof.firstConfidence proof.secondConfidence proof.threshold := by
  intro h
  -- Unfold the verifier and split on its mismatch guard.
  unfold verifyResolutionProofV2 at h
  set expected :=
    Prob.decideResolution proof.firstConfidence proof.secondConfidence proof.threshold with hExpected
  cases hMismatch : (expected != proof.decision) with
  | true =>
      -- In the mismatch branch, the verifier returns `.error`, so `.ok` is impossible.
      have : False := by
        simp [hMismatch] at h
      exact False.elim this
  | false =>
      simp [hMismatch] at h
      cases h
      simp [hExpected]

/-!
If a `resolution_v2` certificate verifies and the decision is `choose_first`,
then the chosen confidence is at least as large as the other one.

This is a *semantic invariant* of the decision procedure (`decideResolution`),
and it becomes a certificate invariant because Lean re-computes the decision.
-/

theorem verifiedResolution_chooseFirst_implies_second_le_first
    (proof : ResolutionProofV2)
    (result : ResolutionResultV2)
    (hVerify : verifyResolutionProofV2 proof = .ok result)
    (hDecision : result.decision = .chooseFirst) :
    Prob.toNat proof.secondConfidence ≤ Prob.toNat proof.firstConfidence := by
  have hExpected :=
    verifyResolutionProofV2_ok_matches_decision proof result hVerify
  have : Prob.decideResolution proof.firstConfidence proof.secondConfidence proof.threshold = .chooseFirst := by
    simpa [hDecision] using hExpected.symm
  exact Prob.resolutionConsistent proof.firstConfidence proof.secondConfidence proof.threshold this

end ResolutionInvariants

-- =============================================================================
-- Rewrite / normalization witnesses
-- =============================================================================

namespace RewriteInvariants

open Axiograph.PathNormalization
open Axiograph.RewriteDerivation
open Axiograph.PathEquivalence
open Axiograph.Certificate.PathRewriteSoundness
open Axiograph.HoTT

/-!
## Soundness of replayable rewrite derivations

The key “meaning” statement for rewrite certificates is:

> If the input expression is well-typed (`toTyped` succeeds),
> and the checker accepts a derivation from `input` to `output`,
> then the denotations of `input` and `output` are equal in the free groupoid.

This is the core result we want to reuse for:
* normalization certificates (`normalize_path_v2`),
* equivalence certificates (`path_equiv_v2`), and
* future domain rewrite / reconciliation derivations.
-/

theorem runDerivationCore_preserves_denotation
    (start end_ : Nat)
    (steps : List PathRewriteStepV2)
    (input output : PathExprV2)
    (hRun : runDerivationCore start end_ input steps = .ok output)
    {a b : Nat}
    {typedInput : PathExpr a b}
    (hTyped : toTyped input = .ok ⟨a, b, typedInput⟩) :
    ∃ typedOutput : PathExpr a b,
      toTyped output = .ok ⟨a, b, typedOutput⟩ ∧
      denote typedInput = denote typedOutput := by
  induction steps generalizing input output a b typedInput with
  | nil =>
      simp [runDerivationCore] at hRun
      cases hRun
      exact ⟨typedInput, hTyped, rfl⟩
  | cons step rest ih =>
      cases hApply : applyAt step.pos.toList step.rule input with
      | error msg =>
          have : False := by
            simp [runDerivationCore, hApply] at hRun
          exact False.elim this
      | ok next =>
          -- Rewrite-rule soundness gives: `denote input = denote next`.
          rcases applyAt_preserves_denote
              (pos := step.pos.toList)
              (rule := step.rule)
              (expr := input)
              (expr' := next)
              (hApply := hApply)
              (typed := typedInput)
              (hTyped := hTyped) with
            ⟨typedNext, hTypedNext, hDenoteStep⟩

          -- Unfold `runDerivationCore` with the successful `applyAt`.
          cases hEndpoints : endpoints next with
          | error msg =>
              have : False := by
                simp [runDerivationCore, hApply, hEndpoints] at hRun
              exact False.elim this
          | ok endpointsNext =>
              rcases endpointsNext with ⟨nextStart, nextEnd⟩
              by_cases hBad : (nextStart != start || nextEnd != end_)
              ·
                have : False := by
                  simp [runDerivationCore, hApply, hEndpoints, hBad] at hRun
                exact False.elim this
              ·
                -- Successful replay means the tail replay succeeded.
                have hTail : runDerivationCore start end_ next rest = .ok output := by
                  simpa [runDerivationCore, hApply, hEndpoints, hBad] using hRun
                rcases ih (input := next) (output := output) (a := a) (b := b)
                    (typedInput := typedNext) hTail hTypedNext with
                  ⟨typedOutput, hTypedOutput, hDenoteTail⟩
                refine ⟨typedOutput, hTypedOutput, ?_⟩
                exact Eq.trans hDenoteStep hDenoteTail

theorem runDerivation_preserves_denotation
    (input : PathExprV2)
    (steps : Array PathRewriteStepV2)
    (output : PathExprV2)
    (hRun : runDerivation input steps = .ok output)
    {a b : Nat}
    {typedInput : PathExpr a b}
    (hTyped : toTyped input = .ok ⟨a, b, typedInput⟩) :
    ∃ typedOutput : PathExpr a b,
      toTyped output = .ok ⟨a, b, typedOutput⟩ ∧
      denote typedInput = denote typedOutput := by
  cases hEndpoints : endpoints input with
  | error msg =>
      have : False := by
        simp [runDerivation, hEndpoints] at hRun
      exact False.elim this
  | ok endpointsInput =>
      rcases endpointsInput with ⟨start, end_⟩
      have hCore : runDerivationCore start end_ input steps.toList = .ok output := by
        simpa [runDerivation, hEndpoints] using hRun
      exact runDerivationCore_preserves_denotation start end_ steps.toList input output hCore hTyped

/-!
## Derivation-backed normalization certificates

When `normalize_path_v2` carries an explicit derivation, and the verifier accepts,
we get a semantic statement “input and normalized denote the same morphism” by
reducing to `runDerivation_preserves_denotation`.
-/

theorem verifyNormalizePathProofV2_sound
    (proof : NormalizePathProofV2)
    (result : NormalizePathResultV2)
    (hVerify : verifyNormalizePathProofV2 proof = .ok result)
    {a b : Nat}
    {typedInput : PathExpr a b}
    (hTyped : toTyped proof.input = .ok ⟨a, b, typedInput⟩) :
    ∃ typedNormalized : PathExpr a b,
      toTyped proof.normalized = .ok ⟨a, b, typedNormalized⟩ ∧
      denote typedInput = denote typedNormalized := by
  have hReplay : runDerivation proof.input proof.derivation = .ok proof.normalized := by
    unfold verifyNormalizePathProofV2 at hVerify
    cases hInputEndpoints : endpoints proof.input with
    | error msg =>
        have : False := by
          simp [hInputEndpoints] at hVerify
        exact False.elim this
    | ok inputEnds =>
        rcases inputEnds with ⟨inputStart, inputEnd⟩
        cases hNormEndpoints : endpoints proof.normalized with
        | error msg =>
            have : False := by
              simp [hInputEndpoints, hNormEndpoints] at hVerify
            exact False.elim this
        | ok normEnds =>
            rcases normEnds with ⟨normStart, normEnd⟩
            cases hEndpointsMismatch : (inputStart != normStart || inputEnd != normEnd) with
            | true =>
                have : False := by
                  simp [hInputEndpoints, hNormEndpoints, hEndpointsMismatch] at hVerify
                exact False.elim this
            | false =>
                cases hRun : runDerivation proof.input proof.derivation with
                | error msg =>
                    have : False := by
                      simp [hInputEndpoints, hNormEndpoints, hEndpointsMismatch, hRun] at hVerify
                    exact False.elim this
                | ok derived =>
                    cases hMismatch : (derived != proof.normalized) with
                    | true =>
                        have : False := by
                          simp [hInputEndpoints, hNormEndpoints, hEndpointsMismatch,
                            hRun, hMismatch] at hVerify
                        exact False.elim this
                    | false =>
                        have hEq : derived = proof.normalized :=
                          (bne_eq_false_iff_eq).1 hMismatch
                        exact congrArg Except.ok hEq
  exact runDerivation_preserves_denotation proof.input proof.derivation proof.normalized hReplay hTyped

/-!
## Soundness of internal builtin replay payloads

`RewriteDerivationProofV2` is the builtin payload reused by normalization and
path-equivalence checking. The verifier accepts iff replay succeeds and
produces the claimed output; semantic soundness follows from the free-groupoid
rule soundness. This theorem does not cover V3 anchored domain rules.
-/

theorem verifyRewriteDerivationProofV2_sound
    (proof : RewriteDerivationProofV2)
    (result : RewriteDerivationResultV2)
    (hVerify : verifyRewriteDerivationProofV2 proof = .ok result)
    {a b : Nat}
    {typedInput : PathExpr a b}
    (hTyped : toTyped proof.input = .ok ⟨a, b, typedInput⟩) :
    ∃ typedOutput : PathExpr a b,
      toTyped proof.output = .ok ⟨a, b, typedOutput⟩ ∧
      denote typedInput = denote typedOutput := by
  have hReplay : runDerivation proof.input proof.derivation = .ok proof.output := by
    unfold verifyRewriteDerivationProofV2 at hVerify
    cases hInputEndpoints : endpoints proof.input with
    | error msg =>
        have : False := by
            simp [hInputEndpoints] at hVerify
        exact False.elim this
    | ok inputEnds =>
        rcases inputEnds with ⟨inputStart, inputEnd⟩
        cases hOutEndpoints : endpoints proof.output with
        | error msg =>
            have : False := by
                simp [hInputEndpoints, hOutEndpoints] at hVerify
            exact False.elim this
        | ok outEnds =>
            rcases outEnds with ⟨outStart, outEnd⟩
            cases hEndpointsMismatch : (inputStart != outStart || inputEnd != outEnd) with
            | true =>
                have : False := by
                    simp [hInputEndpoints, hOutEndpoints, hEndpointsMismatch] at hVerify
                exact False.elim this
            | false =>
                cases hRun : runDerivation proof.input proof.derivation with
                | error msg =>
                    have : False := by
                        simp [hInputEndpoints, hOutEndpoints, hEndpointsMismatch,
                          hRun] at hVerify
                    exact False.elim this
                | ok derived =>
                    cases hMismatch : (derived != proof.output) with
                    | true =>
                        have : False := by
                            simp [hInputEndpoints, hOutEndpoints, hEndpointsMismatch,
                              hRun, hMismatch] at hVerify
                        exact False.elim this
                      | false =>
                          have hEq : derived = proof.output :=
                            (bne_eq_false_iff_eq).1 hMismatch
                          simp [hEq]
  exact runDerivation_preserves_denotation proof.input proof.derivation proof.output hReplay hTyped

/-!
## Soundness of accepted path-equivalence certificates

`verifyPathEquivProofV2` checks two mandatory rewrite traces to one shared normal
form. Each accepted trace preserves free-groupoid denotation, so acceptance
implies denotational equality of the endpoint-indexed inputs. Confidence is not
part of this equality statement.
-/
theorem verifyPathEquivProofV2_sound
    (proof : PathEquivProofV2)
    (result : PathEquivResultV2)
    (hVerify : verifyPathEquivProofV2 proof = .ok result)
    {a b : Nat}
    {typedLeft typedRight : PathExpr a b}
    (hTypedLeft : toTyped proof.left = .ok ⟨a, b, typedLeft⟩)
    (hTypedRight : toTyped proof.right = .ok ⟨a, b, typedRight⟩) :
    denote typedLeft = denote typedRight := by
  cases hLeftReplay : verifyRewriteDerivationProofV2
      (Axiograph.PathEquivalence.leftRewriteProof proof) with
  | error msg =>
      have impossible : (Except.error msg : Except String PathEquivResultV2) = .ok result := by
        simp [verifyPathEquivProofV2, hLeftReplay] at hVerify
      cases impossible
  | ok leftResult =>
      cases hRightReplay : verifyRewriteDerivationProofV2
          (Axiograph.PathEquivalence.rightRewriteProof proof) with
      | error msg =>
          have impossible : (Except.error msg : Except String PathEquivResultV2) = .ok result := by
            simp [verifyPathEquivProofV2, hLeftReplay, hRightReplay] at hVerify
          cases impossible
      | ok rightResult =>
          rcases verifyRewriteDerivationProofV2_sound
              (Axiograph.PathEquivalence.leftRewriteProof proof)
              leftResult hLeftReplay
              (by
                simpa [Axiograph.PathEquivalence.PathEquivProofV2.leftRewriteProof] using hTypedLeft) with
            ⟨typedNormalizedLeft, hTypedNormalizedLeft, hDenoteLeft⟩
          rcases verifyRewriteDerivationProofV2_sound
              (Axiograph.PathEquivalence.rightRewriteProof proof)
              rightResult hRightReplay
              (by
                simpa [Axiograph.PathEquivalence.PathEquivProofV2.rightRewriteProof] using hTypedRight) with
            ⟨typedNormalizedRight, hTypedNormalizedRight, hDenoteRight⟩
          have hNormalizedType : typedNormalizedLeft = typedNormalizedRight := by
            have hOk :
                (Except.ok (ε := String)
                  (⟨a, b, typedNormalizedLeft⟩ : TypedSigma)) =
                .ok (⟨a, b, typedNormalizedRight⟩ : TypedSigma) :=
              hTypedNormalizedLeft.symm.trans hTypedNormalizedRight
            have hSigma :
                (⟨a, b, typedNormalizedLeft⟩ : TypedSigma) =
                (⟨a, b, typedNormalizedRight⟩ : TypedSigma) :=
              Except.ok.inj hOk
            cases hSigma
            rfl
          calc
            denote typedLeft = denote typedNormalizedLeft := hDenoteLeft
            _ = denote typedNormalizedRight := by rw [hNormalizedType]
            _ = denote typedRight := hDenoteRight.symm

end RewriteInvariants

end Axiograph.Certificate.Invariants
