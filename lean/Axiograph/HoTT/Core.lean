import Std
import Mathlib.Logic.Equiv.Defs

/-!
# `Axiograph.HoTT.Core`

This module is the HoTT-flavored “core vocabulary” used by Axiograph’s formal
semantics and certificate checker.

In Idris2 we encode HoTT primitives directly (identity types, transport,
equivalences, quotients, sections/retractions, …) and then build the
knowledge-graph path algebra on top.

Lean4’s built-in equality `Eq` lives in `Prop` and is proof-irrelevant, so this
is **not** a full HoTT implementation. Nevertheless:

1. We keep the same surface API as the Idris kernel (names like `Path`,
   `transport`, `QuasiInverse`, …) so the Idris → Lean translation is auditable.
2. The *non-trivial* higher/groupoid structure we care about is encoded
   explicitly in later modules (`Axiograph.HoTT.KnowledgeGraph`,
   `Axiograph.HoTT.PathAlgebraProofs`) via inductive witnesses.
3. For “best in class” foundations, we reuse Lean/mathlib’s standard
   constructions where possible (`Equiv`, `Quot`, `Trunc`, `Subsingleton`, …).
-/

namespace Axiograph.HoTT

universe u v w

-- =============================================================================
-- Identity Types (Paths)
-- =============================================================================

/-| A path between two values (HoTT identity type). In Lean this is `Eq`. -/
abbrev Path {α : Sort u} (x y : α) : Prop := x = y

/-| Reflexivity: every point has a trivial path to itself. -/
theorem pathRefl {α : Sort u} (x : α) : Path x x := rfl

-- Idris-compatibility aliases
abbrev Refl {α : Sort u} (x : α) : Path x x := pathRefl x

/-| Symmetry: if `x = y` then `y = x`. -/
abbrev pathSymm {α : Sort u} {x y : α} : Path x y → Path y x := Eq.symm
abbrev sym {α : Sort u} {x y : α} : Path x y → Path y x := pathSymm

/-| Transitivity: if `x = y` and `y = z` then `x = z`. -/
abbrev pathTrans {α : Sort u} {x y z : α} : Path x y → Path y z → Path x z := Eq.trans
abbrev trans {α : Sort u} {x y z : α} : Path x y → Path y z → Path x z := pathTrans

/-| Infix composition for paths (mirrors Idris `(@@)`). -/
infixl:90 " @@" => pathTrans

/-| Congruence: equality is respected by functions. -/
abbrev cong {α : Sort u} {β : Sort v} (f : α → β) {x y : α} : Path x y → Path (f x) (f y) :=
  congrArg f

/-| Transport along a path: if `x = y` and `P x`, then `P y`. -/
def transport {α : Sort u} (P : α → Sort v) {x y : α} (p : Path x y) (px : P x) : P y := by
  cases p
  exact px

-- =============================================================================
-- Higher Paths (Paths between Paths)
-- =============================================================================

/-| A 2-path: a path between paths. -/
abbrev Path2 {α : Sort u} {x y : α} (p q : Path x y) : Prop := p = q

/-| A 3-path: a path between 2-paths. -/
abbrev Path3 {α : Sort u} {x y : α} {p q : Path x y} (pe1 pe2 : Path2 p q) : Prop := pe1 = pe2

/-| Left whiskering: horizontal composition of 2-paths. -/
def whiskerLeft {α : Sort u} {x y z : α} {p q : Path x y} (r : Path y z) :
    Path2 p q → Path2 (p @@ r) (q @@ r)
  | h => by cases h; rfl

abbrev whiskerL {α : Sort u} {x y z : α} {p q : Path x y} (r : Path y z) :
    Path2 p q → Path2 (p @@ r) (q @@ r) :=
  whiskerLeft (α := α) (x := x) (y := y) (z := z) (p := p) (q := q) r

/-| Right whiskering: horizontal composition of 2-paths. -/
def whiskerRight {α : Sort u} {x y z : α} {q r : Path y z} (p : Path x y) :
    Path2 q r → Path2 (p @@ q) (p @@ r)
  | h => by cases h; rfl

abbrev whiskerR {α : Sort u} {x y z : α} {q r : Path y z} (p : Path x y) :
    Path2 q r → Path2 (p @@ q) (p @@ r) :=
  whiskerRight (α := α) (x := x) (y := y) (z := z) (q := q) (r := r) p

-- =============================================================================
-- Equivalences
-- =============================================================================

/-|
A quasi-inverse for a function.

This is the same record used in the Idris kernel, but in Lean we can also turn
it into a standard `Equiv` (`Equiv α β`).
-/
structure QuasiInverse {α : Sort u} {β : Sort v} (f : α → β) : Type (max u v) where
  inverse : β → α
  left_inverse : (x : α) → inverse (f x) = x
  right_inverse : (y : β) → f (inverse y) = y

/-| Convert a quasi-inverse witness into a Lean equivalence. -/
def quasiInverseToEquiv {α : Sort u} {β : Sort v} (f : α → β) (qi : QuasiInverse f) : Equiv α β :=
  { toFun := f
    invFun := qi.inverse
    left_inv := qi.left_inverse
    right_inv := qi.right_inverse }

/-| Convert a Lean equivalence into a quasi-inverse witness. -/
def equivToQuasiInverse {α : Sort u} {β : Sort v} (e : Equiv α β) : QuasiInverse e.toFun :=
  { inverse := e.invFun
    left_inverse := e.left_inv
    right_inverse := e.right_inv }

/-| Idris-style names for common equivalence operations. -/
abbrev idEquiv (α : Sort u) : Equiv α α := Equiv.refl α
abbrev compEquiv {α : Sort u} {β : Sort v} {γ : Sort w} (e1 : Equiv α β) (e2 : Equiv β γ) : Equiv α γ :=
  e1.trans e2
abbrev invEquiv {α : Sort u} {β : Sort v} (e : Equiv α β) : Equiv β α := e.symm

-- =============================================================================
-- Univalence (Axiomatized)
-- =============================================================================

/-!
Lean4 + mathlib do not assume univalence.

The Idris codebase currently treats univalence as an axiom (`believe_me`), so we
mirror that here as an explicit axiom. We keep it isolated: the current checker
does not *need* univalence to validate certificates.
-/

axiom ua {α β : Type u} : (Equiv α β) → α = β

def idToEquiv {α β : Type u} : α = β → Equiv α β
  | rfl => Equiv.refl α

axiom uaTransport {α β : Type u} (e : Equiv α β) (x : α) :
    transport (fun t => t) (ua e) x = e x

-- =============================================================================
-- Contractibility and Truncation
-- =============================================================================

/-| A type is contractible if it has a chosen center and every element equals it. -/
structure IsContr (α : Sort u) : Type (u + 1) where
  center : α
  contr : (x : α) → center = x

/-| A type is a proposition if all inhabitants are equal (`Subsingleton`). -/
abbrev IsProp (α : Sort u) : Prop := Subsingleton α

/-| A type is a set if all equality proofs are equal (UIP-style). -/
abbrev IsSet (α : Sort u) : Prop := ∀ x y : α, Subsingleton (x = y)

/-| Propositional truncation: keep only “there exists”, forget witnesses.

Lean core does not expose a HoTT-style propositional truncation in `Type` by
default, and the current Axiograph trusted kernel does not rely on truncation.

For now we model propositional truncation as `Nonempty α` (a `Prop`), which is
already proof-irrelevant.
-/
abbrev Trunc (α : Sort u) : Prop := Nonempty α

theorem truncIsProp (α : Sort u) : IsProp (Trunc α) := by
  infer_instance

-- =============================================================================
-- Fibrations and Dependent Paths
-- =============================================================================

/-|
A dependent path (“path over a path”):
if `p : x = y` and `P : α → Sort`, then `PathOver P p px py` states that `px`
transported along `p` equals `py`.
-/
def PathOver {α : Sort u} (P : α → Sort v) {x y : α} (p : x = y) (px : P x) (py : P y) : Prop :=
  transport P p px = py

def transOver {α : Sort u} {P : α → Sort v} {x y z : α} {p₁ : x = y} {p₂ : y = z}
    {px : P x} {py : P y} {pz : P z} :
    PathOver P p₁ px py → PathOver P p₂ py pz → PathOver P (p₁ @@ p₂) px pz := by
  intro h₁ h₂
  cases p₁
  cases p₂
  simpa [PathOver] using Eq.trans h₁ h₂

-- =============================================================================
-- Sections and Retractions
-- =============================================================================

/-|
A section/retraction pair witnesses that `α` embeds into `β` and can be recovered:
`retraction (section x) = x`.
-/
structure Section (α : Sort u) (β : Sort v) : Type (max u v) where
  embed : α → β
  project : β → α
  project_embed : (x : α) → project (embed x) = x

abbrev HasSection (α : Sort u) (β : Sort v) : Type (max u v) := Section α β

def liftAlongSection {α : Sort u} {β : Sort v} (P : α → Sort w) (sec : Section α β) (x : α) (px : P x) :
    P (sec.project (sec.embed x)) :=
  transport P (pathSymm (sec.project_embed x)) px

def compSection {α : Sort u} {β : Sort v} {γ : Sort w} (s1 : Section α β) (s2 : Section β γ) : Section α γ :=
  { embed := fun x => s2.embed (s1.embed x)
    project := fun z => s1.project (s2.project z)
    project_embed := fun x =>
      pathTrans (cong s1.project (s2.project_embed (s1.embed x))) (s1.project_embed x) }

def idSection (α : Sort u) : Section α α :=
  { embed := id
    project := id
    project_embed := fun _ => rfl }

-- =============================================================================
-- Quotients (using Lean's `Quot`)
-- =============================================================================

/-|
`Quotient α r` identifies `r`-related elements.

Lean’s `Quot` works with a `Prop`-valued relation. This matches the usage in the
Idris codebase where quotient “path constructors” are axiomatized.
-/
abbrev Quotient (α : Sort u) (r : α → α → Prop) : Sort u := Quot r

abbrev QInject {α : Sort u} {r : α → α → Prop} (x : α) : Quotient α r :=
  Quot.mk r x

def qpath {α : Sort u} {r : α → α → Prop} (x y : α) (h : r x y) :
    (QInject (r := r) x) = (QInject (r := r) y) :=
  Quot.sound h

def qelim {α : Sort u} {r : α → α → Prop} {β : Sort v} (f : α → β)
    (respect : (x y : α) → r x y → f x = f y) :
    Quotient α r → β :=
  Quot.lift f respect

def qelimProp {α : Sort u} {r : α → α → Prop} (p : Quotient α r → Prop)
    (base : (x : α) → p (QInject (r := r) x)) :
    (q : Quotient α r) → p q :=
  fun q => Quot.inductionOn q base

abbrev SetQuotient (α : Sort u) (r : α → α → Prop) : Sort u := Quotient α r

-- Common specializations used in Axiograph.
abbrev EntityClass (α : Sort u) (sameAs : α → α → Prop) : Sort u := Quotient α sameAs
abbrev PathEquivClass (α : Sort u) (equiv : α → α → Prop) : Sort u := Quotient α equiv

/-|
Choose a representative for each equivalence class *as a function out of the quotient*.

To be well-defined, `choose` must respect the relation.
The Idris version axiomatizes this proof; here we require it explicitly.
-/
def canonical {α : Sort u} {r : α → α → Prop} (choose : α → α)
    (respect : (x y : α) → r x y → choose x = choose y) :
    Quotient α r → α :=
  qelim choose respect

-- =============================================================================
-- Restrictions (subtypes)
-- =============================================================================

/-| A restriction is a value paired with a proof it satisfies a predicate. -/
abbrev Restriction {α : Sort u} (good : α → Prop) : Sort (max 1 u) := { x : α // good x }

def restrictedValue {α : Sort u} {good : α → Prop} (r : Restriction good) : α :=
  r.val

def mkRestriction {α : Sort u} {good : α → Prop} (x : α) (h : good x) : Restriction good :=
  ⟨x, h⟩

abbrev restrictionEmbed {α : Sort u} {good : α → Prop} : Restriction good → α :=
  restrictedValue

def restrictionProject {α : Sort u} {good : α → Prop} [DecidablePred good] (defaultValue : α) (defaultOk : good defaultValue) :
    α → Restriction good
  | x =>
      if h : good x then
        ⟨x, h⟩
      else
        ⟨defaultValue, defaultOk⟩

abbrev KeyQuotient (α : Sort u) (key : α → Sort v) : Sort u :=
  Quotient α (fun x y => key x = key y)

abbrev FunctionalQuotient (α : Sort u) (src : α → Sort v) (tgt : α → Sort w) : Sort u :=
  Quotient α (fun x y => src x = src y ∧ tgt x = tgt y)

end Axiograph.HoTT
