-- =============================================================================
-- Axiograph.Util.Dec - explicit decidability witness
-- =============================================================================
--
-- Lean's built-in `Decidable` targets `Prop`; this helper is useful where the
-- certificate layer wants an explicit positive/negative witness as data.

namespace Axiograph

universe u

inductive Dec (α : Sort u) : Type u where
  | yes : α → Dec α
  | no : (α → False) → Dec α

end Axiograph
