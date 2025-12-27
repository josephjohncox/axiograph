import Mathlib.Algebra.BigOperators.Group.Finset.Basic
import Mathlib.Data.Nat.Basic
import Mathlib.Data.Fintype.Basic
import Mathlib.Data.Fintype.Card
import Mathlib.Data.Rat.Init

/-!
# `Axiograph.Prob.Verified`

This module provides **verified probabilities** and confidence-combination rules.

We intentionally avoid floating point arithmetic in the trusted checker.
Instead, a probability is represented in **fixed-point** form:

* `Precision = 1_000_000`
* a probability is `numerator / Precision` where `numerator ∈ [0, Precision]`

This mirrors `idris/Axiograph/Prob/Verified.idr`, but uses Lean/mathlib lemmas to
avoid unsafe casts (`believe_me`) and to keep proofs readable.
-/

namespace Axiograph.Prob

open scoped BigOperators

-- =============================================================================
-- Fixed-Point Probability
-- =============================================================================

/-| The fixed-point denominator. `numerator = Precision` means probability 1. -/
def Precision : Nat := 1_000_000

/-|
Verified probability: the bound `0 ≤ numerator ≤ Precision` is enforced by `Fin`.

This keeps certificates deterministic and makes arithmetic proofs feasible.
-/
structure VProb where
  numerator : Fin (Precision + 1)
deriving Repr, DecidableEq

abbrev MkVProb := VProb.mk

/-| View a probability as its fixed-point numerator. -/
def toNat (p : VProb) : Nat :=
  p.numerator.val

abbrev toFixedPoint : VProb → Nat := toNat

/-| Convert to `Float` for display/logging only (never for proofs). -/
def toFloat (p : VProb) : Float :=
  (Float.ofNat (toNat p)) / (Float.ofNat Precision)

/-| Convert to `ℚ` for reasoning/debugging (exact). -/
def toRat (p : VProb) : ℚ :=
  (toNat p : ℚ) / (Precision : ℚ)

/-| Numerators are always bounded by `Precision` (by construction). -/
theorem numeratorBounded (p : VProb) : toNat p ≤ Precision :=
  Nat.le_of_lt_succ p.numerator.is_lt

/-| Smart constructor from a bounded numerator. -/
def ofNat (n : Nat) (h : n ≤ Precision) : VProb :=
  MkVProb ⟨n, Nat.lt_succ_of_le h⟩

/-| Clamp a raw numerator into `[0, Precision]` (useful when parsing untrusted input). -/
def ofNatClamped (n : Nat) : VProb :=
  ofNat (Nat.min n Precision) (Nat.min_le_right _ _)

def vZero : VProb :=
  ofNat 0 (Nat.zero_le _)

def vOne : VProb :=
  ofNat Precision le_rfl

-- =============================================================================
-- Verified Operations
-- =============================================================================

/-|
Multiplication of probabilities (fixed-point):

`(a/P) * (b/P) = (a*b/P) / P`  (integer division rounds down).

The result is still within `[0, P]`, and we prove that bound.
-/
def vMult (p1 p2 : VProb) : VProb :=
  let leftNumerator := toNat p1
  let rightNumerator := toNat p2
  let scaledNumerator := (leftNumerator * rightNumerator) / Precision
  have leftBound : leftNumerator ≤ Precision := numeratorBounded p1
  have rightBound : rightNumerator ≤ Precision := numeratorBounded p2
  have productBound : leftNumerator * rightNumerator ≤ Precision * Precision := by
    exact Nat.mul_le_mul leftBound rightBound
  have scaledBound : scaledNumerator ≤ Precision :=
    Nat.div_le_of_le_mul (m := leftNumerator * rightNumerator) (k := Precision) (n := Precision) productBound
  ofNat scaledNumerator scaledBound

/-| Safe multiplication that also returns the (trivial) bound witness. -/
def vMultSafe (leftProbability rightProbability : VProb) :
    { result : VProb // toNat result ≤ Precision } :=
  let result := vMult leftProbability rightProbability
  ⟨result, numeratorBounded result⟩

/-| Complement: `1 - p`. -/
def vComplement (p : VProb) : VProb :=
  let n := toNat p
  have hn : Precision - n ≤ Precision := Nat.sub_le _ _
  ofNat (Precision - n) hn

-- =============================================================================
-- Discrete Distributions (Verified)
-- =============================================================================

/-| Sum the numerators of a finite family of probabilities. -/
def sumNumerators {n : Nat} (ps : Fin n → VProb) : Nat :=
  ∑ i : Fin n, toNat (ps i)

/-|
A discrete distribution over `n` outcomes.

We keep the sum constraint as an inequality because fixed-point rounding makes
exact equality awkward (and not needed for most invariants).
-/
structure VDist (n : Nat) where
  probs : Fin n → VProb
  sumValid : sumNumerators probs ≤ Precision + 1

/-|
Uniform distribution.

Each entry is `Precision / n`, so the total numerator sum is at most `Precision`
(and therefore ≤ `Precision + 1`).
-/
def uniform (n : Nat) (_hn : 0 < n) : VDist n :=
  let probVal := Precision / n
  have probBound : probVal ≤ Precision := Nat.div_le_self _ _
  let single : VProb := ofNat probVal probBound
  have sumBound : sumNumerators (n := n) (fun _ => single) ≤ Precision + 1 := by
    classical
    -- `∑ i : Fin n, c = n * c`
    have hsum : sumNumerators (n := n) (fun _ => single) = n * toNat single := by
      simp [sumNumerators]
    have hle : sumNumerators (n := n) (fun _ => single) ≤ Precision := by
      -- `n * (Precision / n) ≤ Precision`
      -- and `toNat single = Precision / n` by construction.
      have : n * toNat single ≤ Precision := by
        -- `toNat single = Precision / n` by construction.
        simpa [single, probVal, toNat] using Nat.mul_div_le Precision n
      simpa [hsum] using this
    exact Nat.le_trans hle (Nat.le_succ Precision)
  { probs := fun _ => single, sumValid := sumBound }

-- =============================================================================
-- Evidence Strength
-- =============================================================================

inductive EvidenceStrength where
  | strong       -- ≥ 0.8
  | moderate     -- ≥ 0.5
  | weak         -- ≥ 0.2
  | negligible
deriving Repr, DecidableEq

def strongThreshold : Nat := 800_000
def moderateThreshold : Nat := 500_000
def weakThreshold : Nat := 200_000

def classifyStrength (probability : VProb) : EvidenceStrength :=
  let numerator := toNat probability
  if strongThreshold ≤ numerator then
    .strong
  else if moderateThreshold ≤ numerator then
    .moderate
  else if weakThreshold ≤ numerator then
    .weak
  else
    .negligible

theorem strongMeansHigh (probability : VProb) :
    classifyStrength probability = .strong → strongThreshold ≤ toNat probability := by
  intro classificationIsStrong
  by_cases hStrong : strongThreshold ≤ toNat probability
  · exact hStrong
  · -- In the `else` branch, `classifyStrength p` cannot be `.strong`.
    have : False := by
      by_cases hModerate : moderateThreshold ≤ toNat probability
      ·
        have h' := classificationIsStrong
        simp [classifyStrength, hStrong, hModerate] at h'
      · by_cases hWeak : weakThreshold ≤ toNat probability
        ·
          have h' := classificationIsStrong
          simp [classifyStrength, hStrong, hModerate, hWeak] at h'
        ·
          have h' := classificationIsStrong
          simp [classifyStrength, hStrong, hModerate, hWeak] at h'
    exact False.elim this

-- =============================================================================
-- Bayesian Update
-- =============================================================================

structure BayesUpdate where
  prior : VProb
  likelihoodTrue : VProb   -- P(E|H)
  likelihoodFalse : VProb  -- P(E|¬H)
deriving Repr, DecidableEq

/-|
Bayesian posterior in fixed-point arithmetic.

We compute:
* `numerator   = P(E|H) * P(H)`
* `denominator = P(E|H) * P(H) + P(E|¬H) * P(¬H)`
and return `numerator / denominator`.

We return the prior if the denominator is 0.
-/
def bayesPosterior (bu : BayesUpdate) : VProb :=
  let priorNumerator := toNat bu.prior
  let likelihoodTrueNumerator := toNat bu.likelihoodTrue
  let likelihoodFalseNumerator := toNat bu.likelihoodFalse
  let numerator := likelihoodTrueNumerator * priorNumerator
  let priorComplementNumerator := Precision - priorNumerator
  let denominator := numerator + (likelihoodFalseNumerator * priorComplementNumerator)
  if hDenomZero : denominator = 0 then
    bu.prior
  else
    let scaled := (numerator * Precision) / denominator
    have hscaled : scaled ≤ Precision := by
      have hDenGe : numerator ≤ denominator := by
        -- `numerator ≤ numerator + (...)`
        exact Nat.le_add_right _ _
      have hMul : numerator * Precision ≤ denominator * Precision :=
        Nat.mul_le_mul_right Precision hDenGe
      -- `(numerator * Precision) / denominator ≤ Precision`
      exact Nat.div_le_of_le_mul (m := numerator * Precision) (k := denominator) (n := Precision) (by
        simpa [Nat.mul_assoc] using hMul)
    ofNat scaled hscaled

theorem posteriorValid (bu : BayesUpdate) : toNat (bayesPosterior bu) ≤ Precision :=
  numeratorBounded _

-- =============================================================================
-- Confidence Combination
-- =============================================================================

def combineIndependent : VProb → VProb → VProb :=
  vMult

/-|
Combining independent probabilities cannot increase confidence:
`p1 * p2 ≤ p1` because `p2 ≤ 1`.
-/
theorem combineReduces (leftProbability rightProbability : VProb) :
    toNat (combineIndependent leftProbability rightProbability) ≤ toNat leftProbability := by
  -- Show: (a*b)/P ≤ a  using `b ≤ P`.
  let leftNumerator := toNat leftProbability
  let rightNumerator := toNat rightProbability
  have rightBound : rightNumerator ≤ Precision := numeratorBounded rightProbability
  have productBound : leftNumerator * rightNumerator ≤ Precision * leftNumerator := by
    -- `a*b ≤ a*P = P*a`
    have : leftNumerator * rightNumerator ≤ leftNumerator * Precision :=
      Nat.mul_le_mul_left leftNumerator rightBound
    simpa [Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc] using this
  -- Now apply `Nat.div_le_of_le_mul`.
  have : (leftNumerator * rightNumerator) / Precision ≤ leftNumerator :=
    Nat.div_le_of_le_mul
      (m := leftNumerator * rightNumerator)
      (k := Precision)
      (n := leftNumerator)
      productBound
  simpa [combineIndependent, vMult, leftNumerator, rightNumerator] using this

/-| Symmetric version of `combineReduces`: the product is also ≤ the second factor. -/
theorem combineReducesRight (leftProbability rightProbability : VProb) :
    toNat (combineIndependent leftProbability rightProbability) ≤ toNat rightProbability := by
  let leftNumerator := toNat leftProbability
  let rightNumerator := toNat rightProbability
  have leftBound : leftNumerator ≤ Precision := numeratorBounded leftProbability
  have productBound : leftNumerator * rightNumerator ≤ Precision * rightNumerator := by
    -- Multiply `a ≤ Precision` by `b`.
    simpa [Nat.mul_comm, Nat.mul_left_comm, Nat.mul_assoc] using Nat.mul_le_mul_right rightNumerator leftBound
  have : (leftNumerator * rightNumerator) / Precision ≤ rightNumerator :=
    Nat.div_le_of_le_mul
      (m := leftNumerator * rightNumerator)
      (k := Precision)
      (n := rightNumerator)
      productBound
  simpa [combineIndependent, vMult, leftNumerator, rightNumerator] using this

/-| Average two probabilities (fixed-point). -/
def average (p1 p2 : VProb) : VProb :=
  let sum := toNat p1 + toNat p2
  let avg := sum / 2
  have havg : avg ≤ Precision := by
    -- `(a+b)/2 ≤ (P+P)/2 = P`
    have ha : toNat p1 ≤ Precision := numeratorBounded p1
    have hb : toNat p2 ≤ Precision := numeratorBounded p2
    have hsum : sum ≤ Precision + Precision := Nat.add_le_add ha hb
    have hAvgUpper : avg ≤ (Precision + Precision) / 2 := Nat.div_le_div_right hsum
    have hUpper : (Precision + Precision) / 2 ≤ Precision := by
      have : (Precision + Precision) / 2 = Precision := by
        -- `(P + P) / 2 = (P * 2) / 2 = P`
        rw [← Nat.mul_two Precision]
        exact Nat.mul_div_right Precision (by decide : 0 < (2 : Nat))
      exact le_of_eq this
    exact Nat.le_trans hAvgUpper hUpper
  ofNat avg havg

-- =============================================================================
-- Decidable Equality / Ordering
-- =============================================================================

def decEqVProb (p1 p2 : VProb) : Decidable (p1 = p2) :=
  by infer_instance

def decLTE (p1 p2 : VProb) : Decidable (toNat p1 ≤ toNat p2) :=
  inferInstance

-- =============================================================================
-- Path Confidence
-- =============================================================================

def pathConfidence : List VProb → VProb
  | [] => vOne
  | p :: ps => vMult p (pathConfidence ps)

theorem pathConfidenceDecreases (firstStepConfidence : VProb) (rest : List VProb) :
    toNat (pathConfidence (firstStepConfidence :: rest)) ≤ toNat (pathConfidence rest) := by
  simpa [pathConfidence] using combineReducesRight firstStepConfidence (pathConfidence rest)

theorem emptyPathConfidence : pathConfidence ([] : List VProb) = vOne :=
  rfl

-- =============================================================================
-- Conflict Resolution (Decision Procedure)
-- =============================================================================

inductive Resolution where
  | chooseFirst
  | chooseSecond
  | merge (w1 w2 : VProb)
  | needReview
deriving Repr, DecidableEq

def decideResolution (firstConfidence secondConfidence threshold : VProb) : Resolution :=
  let n1 := toNat firstConfidence
  let n2 := toNat secondConfidence
  let gap := if n1 ≥ n2 then n1 - n2 else n2 - n1
  let threshN := toNat threshold
  if gap ≥ threshN then
    if n1 ≥ n2 then .chooseFirst else .chooseSecond
  else if gap ≥ (threshN / 2) then
    .merge firstConfidence secondConfidence
  else
    .needReview

theorem resolutionConsistent (firstConfidence secondConfidence threshold : VProb) :
    decideResolution firstConfidence secondConfidence threshold = .chooseFirst →
      toNat secondConfidence ≤ toNat firstConfidence := by
  intro decisionIsChooseFirst
  -- If `firstConfidence ≥ secondConfidence`, we are done (this is exactly the goal).
  by_cases firstAtLeastSecond : toNat firstConfidence ≥ toNat secondConfidence
  · simpa using firstAtLeastSecond
  ·
    -- Otherwise, `chooseFirst` is impossible: derive a contradiction by case-splitting
    -- the remaining `if` conditions.
    have contradiction : False := by
      have h := decisionIsChooseFirst
      unfold decideResolution at h
      simp [firstAtLeastSecond] at h
      -- The result can only be `chooseSecond`, `merge`, or `needReview` in this branch.
      by_cases gapLarge : toNat threshold ≤ toNat secondConfidence - toNat firstConfidence
      · simp [gapLarge] at h
      · by_cases gapMedium :
            toNat threshold / 2 ≤ toNat secondConfidence - toNat firstConfidence
        · simp [gapLarge, gapMedium] at h
        · simp [gapLarge, gapMedium] at h
    exact False.elim contradiction

-- =============================================================================
-- Source Credibility
-- =============================================================================

structure TrackRecord where
  correct : Nat
  incorrect : Nat
deriving Repr, DecidableEq

def computeCredibility (tr : TrackRecord) : VProb :=
  let total := tr.correct + tr.incorrect
  if h : total = 0 then
    -- default 0.5 when no data
    ofNat 500_000 (by decide)
  else
    let ratio := (tr.correct * Precision) / total
    have hratio : ratio ≤ Precision := by
      -- `correct ≤ total`, so `correct*P ≤ total*P`, so division by `total` gives ≤ P
      have hct : tr.correct ≤ total := Nat.le_add_right _ _
      have hmul : tr.correct * Precision ≤ total * Precision := Nat.mul_le_mul_right Precision hct
      exact Nat.div_le_of_le_mul (m := tr.correct * Precision) (k := total) (n := Precision) (by
        simpa [Nat.mul_assoc] using hmul)
    ofNat ratio hratio

-- TODO: prove monotonicity with rounding; Idris currently axiomatizes this.
axiom moreCorrectBetter (tr : TrackRecord) :
    toNat (computeCredibility tr) ≤ toNat (computeCredibility { correct := tr.correct + 1, incorrect := tr.incorrect })

-- =============================================================================
-- Runtime Assertions (for untrusted inputs)
-- =============================================================================

def assertValid (p : VProb) : Bool :=
  toNat p ≤ Precision

def assertSumsToOne (ps : List VProb) : Bool :=
  let sum := ps.foldl (fun acc p => acc + toNat p) 0
  (Precision - 1000 ≤ sum) && (sum ≤ Precision + 1000)

-- =============================================================================
-- External Format Helpers
-- =============================================================================

def fromFixedPoint (n : Nat) : Option VProb :=
  if h : n ≤ Precision then
    some (ofNat n h)
  else
    none

theorem roundtripPreserves (p : VProb) : fromFixedPoint (toFixedPoint p) = some p := by
  -- `toFixedPoint` is the `Fin.val`; the proof component is unique (Prop).
  cases p with
  | mk num =>
    have h : num.val ≤ Precision := Nat.le_of_lt_succ num.is_lt
    simp [fromFixedPoint, toFixedPoint, toNat, ofNat, h, MkVProb]

end Axiograph.Prob
