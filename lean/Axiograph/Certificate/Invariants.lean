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

* reachability witnesses (`reachability_v2`, optionally anchored to `PathDBExportV1`)
* normalization / rewrite witnesses (`normalize_path_v2`, `rewrite_derivation_v2`)
* reconciliation decisions (`resolution_v2`)

This file does **not** aim to prove completeness of any algorithm (“all answers”),
only soundness-style invariants (“these witnesses are internally consistent and
mean what they claim”).
-/

namespace Axiograph.Certificate.Invariants

open Axiograph

-- =============================================================================
-- Reachability witnesses
-- =============================================================================

namespace ReachabilityInvariants

open Axiograph.Reachability

/-!
## Internal structure invariants (unanchored)

`verifyReachabilityProofV2` is the internal checker for reachability witnesses.
It does *not* consult any concrete graph; it only checks that the witness is a
well-formed chain and returns the derived summary.

The following theorem records that whenever verification succeeds, the returned
summary agrees with the witness’s own “derived” fields (`start`, `end_`, etc.).
-/

theorem verifyReachabilityProofV2_ok_matches_computations
    (proof : ReachabilityProofV2)
    (result : ReachabilityResultV2) :
    verifyReachabilityProofV2 proof = .ok result →
      result.start = proof.start ∧
      result.end_ = proof.end_ ∧
      result.pathLen = proof.pathLen ∧
      result.confidence = proof.confidence := by
  induction proof generalizing result with
  | reflexive entity =>
      intro h
      simp [verifyReachabilityProofV2] at h
      cases h
      simp [ReachabilityProofV2.start, ReachabilityProofV2.end_, ReachabilityProofV2.pathLen,
        ReachabilityProofV2.confidence]
  | step src relType dst relConfidence relationId? rest ih =>
      intro h
      cases hRest : verifyReachabilityProofV2 rest with
      | error msg =>
          simp [verifyReachabilityProofV2, hRest] at h
      | ok restRes =>
          have hRestInv := ih restRes hRest
          cases hChain : (restRes.start != dst) with
          | true =>
              simp [verifyReachabilityProofV2, hRest, hChain] at h
          | false =>
              simp [verifyReachabilityProofV2, hRest, hChain] at h
              cases h
              rcases hRestInv with ⟨_hStart, hEnd, hLen, hConf⟩
              refine ⟨rfl, ?_, ?_, ?_⟩
              · simpa [ReachabilityProofV2.end_] using hEnd
              · simpa [ReachabilityProofV2.pathLen] using hLen
              · simpa [ReachabilityProofV2.confidence] using congrArg (fun p => Prob.vMult relConfidence p) hConf

/-!
## Anchored structure invariants

`verifyReachabilityProofV2Anchored` additionally requires each witness step to:

* reference a real `relation_id` in a `PathDBExportV1` snapshot (via
  `relation_info`), and
* match the snapshot’s edge metadata (endpoints, rel-type, and confidence).

We record this as an inductive predicate describing a “snapshot-anchored”
reachability chain.
-/

open Axiograph.Axi.PathDBExportV1

inductive AnchoredReachabilityChain
    (relationInfo : Std.HashMap Nat RelationInfoRow) :
    ReachabilityProofV2 → Prop
  | reflexive (entity : Nat) :
      AnchoredReachabilityChain relationInfo (.reflexive entity)
  | step
      (src relType dst : Nat)
      (relConfidence : Prob.VProb)
      (relationId : Nat)
      (row : RelationInfoRow)
      (rest : ReachabilityProofV2)
      (hGet : relationInfo.get? relationId = some row)
      (hRelType : row.relTypeId = relType)
      (hSource : row.source = src)
      (hTarget : row.target = dst)
      (hConfidence : Prob.toNat row.confidence = Prob.toNat relConfidence)
      (hChain : rest.start = dst)
      (hRest : AnchoredReachabilityChain relationInfo rest) :
      AnchoredReachabilityChain relationInfo (.step src relType dst relConfidence (some relationId) rest)

theorem verifyReachabilityProofV2Anchored_ok_matches_computations
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (proof : ReachabilityProofV2)
    (result : ReachabilityResultV2) :
    verifyReachabilityProofV2Anchored relationInfo proof = .ok result →
      result.start = proof.start ∧
      result.end_ = proof.end_ ∧
      result.pathLen = proof.pathLen ∧
      result.confidence = proof.confidence := by
  induction proof generalizing result with
  | reflexive entity =>
      intro h
      simp [verifyReachabilityProofV2Anchored] at h
      cases h
      simp [ReachabilityProofV2.start, ReachabilityProofV2.end_, ReachabilityProofV2.pathLen,
        ReachabilityProofV2.confidence]
  | step src relType dst relConfidence relationId? rest ih =>
      intro h
      cases relationId? with
      | none =>
          simp [verifyReachabilityProofV2Anchored] at h
      | some relationId =>
          cases hRow : relationInfo[relationId]? with
          | none =>
              simp [verifyReachabilityProofV2Anchored, hRow] at h
          | some row =>
              cases hEndpoints : (row.source != src || row.target != dst) with
              | true =>
                  simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints] at h
              | false =>
                  cases hRelType : (row.relTypeId != relType) with
                  | true =>
                      simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType] at h
                  | false =>
                      cases hConfidence : (Prob.toNat row.confidence != Prob.toNat relConfidence) with
                      | true =>
                          simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence] at h
                      | false =>
                          cases hRest : verifyReachabilityProofV2Anchored relationInfo rest with
                          | error msg =>
                              simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence, hRest] at h
                          | ok restRes =>
                              have hRestInv := ih restRes hRest
                              cases hChain : (restRes.start != dst) with
                              | true =>
                                  simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence, hRest, hChain] at h
                              | false =>
                                  simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence, hRest,
                                    hChain] at h
                                  cases h
                                  rcases hRestInv with ⟨_hStart, hEnd, hLen, hConf⟩
                                  refine ⟨rfl, ?_, ?_, ?_⟩
                                  · simpa [ReachabilityProofV2.end_] using hEnd
                                  · simpa [ReachabilityProofV2.pathLen] using hLen
                                  · simpa [ReachabilityProofV2.confidence] using congrArg (fun p => Prob.vMult relConfidence p) hConf

theorem verifyReachabilityProofV2Anchored_ok_implies_anchored_chain
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (proof : ReachabilityProofV2)
    (result : ReachabilityResultV2) :
    verifyReachabilityProofV2Anchored relationInfo proof = .ok result →
      AnchoredReachabilityChain relationInfo proof := by
  induction proof generalizing result with
  | reflexive entity =>
      intro h
      simp [verifyReachabilityProofV2Anchored] at h
      exact AnchoredReachabilityChain.reflexive (relationInfo := relationInfo) entity
  | step src relType dst relConfidence relationId? rest ih =>
      intro h
      cases relationId? with
      | none =>
          simp [verifyReachabilityProofV2Anchored] at h
      | some relationId =>
          cases hRow : relationInfo[relationId]? with
          | none =>
              simp [verifyReachabilityProofV2Anchored, hRow] at h
          | some row =>
              cases hEndpoints : (row.source != src || row.target != dst) with
              | true =>
                  simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints] at h
              | false =>
                  cases hRelType : (row.relTypeId != relType) with
                  | true =>
                      simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType] at h
                  | false =>
                      cases hConfidence : (Prob.toNat row.confidence != Prob.toNat relConfidence) with
                      | true =>
                          simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence] at h
                      | false =>
                          cases hRest : verifyReachabilityProofV2Anchored relationInfo rest with
                          | error msg =>
                              simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence, hRest] at h
                          | ok restRes =>
                              cases hChain : (restRes.start != dst) with
                              | true =>
                                  simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence, hRest,
                                    hChain] at h
                              | false =>
                                  simp [verifyReachabilityProofV2Anchored, hRow, hEndpoints, hRelType, hConfidence, hRest,
                                    hChain] at h
                                  have hRestChain : AnchoredReachabilityChain relationInfo rest :=
                                    ih restRes hRest

                                  -- Convert the checker’s boolean guards into equalities.
                                  have hEndpoints' :
                                      (row.source != src) = false ∧ (row.target != dst) = false := by
                                    exact (Bool.or_eq_false_iff).1 hEndpoints
                                  have hSource : row.source = src :=
                                    (bne_eq_false_iff_eq).1 hEndpoints'.left
                                  have hTarget : row.target = dst :=
                                    (bne_eq_false_iff_eq).1 hEndpoints'.right
                                  have hRelTypeEq : row.relTypeId = relType :=
                                    (bne_eq_false_iff_eq).1 hRelType
                                  have hConfidenceEq : Prob.toNat row.confidence = Prob.toNat relConfidence :=
                                    (bne_eq_false_iff_eq).1 hConfidence

                                  -- Recover the witness-level chaining invariant `rest.start = dst`.
                                  have hRestSummary :=
                                    verifyReachabilityProofV2Anchored_ok_matches_computations
                                      relationInfo rest restRes hRest
                                  have hRestResStartEq : restRes.start = rest.start := hRestSummary.left
                                  have hRestResStartDst : restRes.start = dst :=
                                    (bne_eq_false_iff_eq).1 hChain
                                  have hChainEq : rest.start = dst := by
                                    exact hRestResStartEq.symm.trans hRestResStartDst

                                  exact AnchoredReachabilityChain.step
                                    (relationInfo := relationInfo)
                                    (src := src)
                                    (relType := relType)
                                    (dst := dst)
                                    (relConfidence := relConfidence)
                                    (relationId := relationId)
                                    (row := row)
                                    (rest := rest)
                                    (hGet := hRow)
                                    (hRelType := hRelTypeEq)
                                    (hSource := hSource)
                                    (hTarget := hTarget)
                                    (hConfidence := hConfidenceEq)
                                    (hChain := hChainEq)
                                    (hRest := hRestChain)

end ReachabilityInvariants

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

theorem verifyNormalizePathProofV2_sound_of_derivation
    (proof : NormalizePathProofV2)
    (steps : Array PathRewriteStepV2)
    (hDerivation : proof.derivation? = some steps)
    (result : NormalizePathResultV2)
    (hVerify : verifyNormalizePathProofV2 proof = .ok result)
    {a b : Nat}
    {typedInput : PathExpr a b}
    (hTyped : toTyped proof.input = .ok ⟨a, b, typedInput⟩) :
    ∃ typedNormalized : PathExpr a b,
      toTyped proof.normalized = .ok ⟨a, b, typedNormalized⟩ ∧
      denote typedInput = denote typedNormalized := by
  have hReplay : runDerivation proof.input steps = .ok proof.normalized := by
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
                -- Reduce the optional derivation branch using `hDerivation`, then analyze `runDerivation`.
                cases hRun : runDerivation proof.input steps with
                | error msg =>
                    have : False := by
                        simp [hInputEndpoints, hNormEndpoints, hEndpointsMismatch,
                          hDerivation, hRun] at hVerify
                    exact False.elim this
                | ok derived =>
                    cases hMismatch : (derived != proof.normalized) with
                    | true =>
                        have : False := by
                            simp [hInputEndpoints, hNormEndpoints, hEndpointsMismatch,
                              hDerivation, hRun, hMismatch] at hVerify
                        exact False.elim this
                      | false =>
                          have hEq : derived = proof.normalized :=
                            (bne_eq_false_iff_eq).1 hMismatch
                          simp [hEq]
  exact runDerivation_preserves_denotation proof.input steps proof.normalized hReplay hTyped

/-!
## Soundness of `rewrite_derivation_v2` certificates

This is the generic certificate kind for replayable derivations. The verifier
accepts iff replay succeeds and produces the claimed output; the semantic
soundness follows from the free-groupoid rule soundness.
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

end RewriteInvariants

end Axiograph.Certificate.Invariants
