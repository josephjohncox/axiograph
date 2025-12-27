import Axiograph.Prob.Verified
import Mathlib.Analysis.SpecialFunctions.Log.Basic
import Mathlib.Data.Real.Basic

/-!
# `Axiograph.Prob.KL`

This module defines a **KL-divergence** (Kullback–Leibler) style score for
finite, fixed-point probability distributions.

Important design note:

*This is not part of the trusted certificate checker.*

KL divergence involves transcendental functions (`log`). Even if we define it in
Lean for semantic clarity, we should keep it out of the small trusted executable
until we have a clear, constructive “bounded check” story.

Instead, we use KL (and related scores like Jensen–Shannon) as:

- untrusted analysis tooling (drift detection across contexts/snapshots),
- reconciliation/learning heuristics,
- ranking and exploration signals.

The certified core remains: derivability + explicit witnesses.
-/

namespace Axiograph.Prob

open scoped BigOperators

noncomputable section

/-| Convert fixed-point `VProb` into a real number in `[0, 1]`. -/
def toReal (p : VProb) : ℝ :=
  (toRat p : ℝ)

/-|
Smoothing for divergence-style analytics.

We clamp values below `ε` up to `ε` to avoid `log 0` and division by zero in
practical analytics. This changes the value of KL; treat it as a heuristic
score, not a certified truth claim.
-/
def smooth (ε : ℝ) (x : ℝ) : ℝ :=
  max ε x

/-|
KL divergence score (smoothed), over `n` outcomes.

`KL(p || q) = Σᵢ pᵢ * log(pᵢ / qᵢ)`

We smooth both `p` and `q` by `ε` (see `smooth`).
-/
def klDiv (ε : ℝ) {n : Nat} (p q : Fin n → VProb) : ℝ :=
  ∑ i : Fin n,
    let pi := smooth ε (toReal (p i))
    let qi := smooth ε (toReal (q i))
    pi * Real.log (pi / qi)

end

end Axiograph.Prob
