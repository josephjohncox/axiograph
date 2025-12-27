-- =============================================================================
-- Axiograph.Util.Dec - Idris-style `Dec` (Lean helper)
-- =============================================================================
--
-- Idris uses `Dec : Type -> Type` to represent decidability of any type, not just
-- propositions. Lean's built-in `Decidable` targets `Prop`, so we provide a tiny
-- `Dec` mirror to keep Idris→Lean ports auditable.

namespace Axiograph

universe u

inductive Dec (α : Sort u) : Type u where
  | yes : α → Dec α
  | no : (α → False) → Dec α

end Axiograph

