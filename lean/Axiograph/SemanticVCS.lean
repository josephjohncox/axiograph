import Std

/-!
# Semantic VCS, finite slices, and merge/rebase preservation

This module is a Lean-side formalization scaffold for the operational semantic
VCS theory used by Rust.

It deliberately proves only small, finite-slice preservation facts.  It is not
the shipped verifier boundary (`Axiograph.VerifyMain` stays narrow), not a proof
that arbitrary ontologies form a complete lattice, and not a full HoTT or
univalence development.  The intended bridge is:

* Rust builds `SemanticSliceManifestV1`, `SemanticMergePlanV1`, and
  `SemanticRebasePlanV1` over compiled IR ids.
* Lean can later certify narrow fragments by checking that a runtime plan
  denotes one of the conservative operations below.
-/

namespace Axiograph
namespace SemanticVCS

/-- The finite operational ref kinds that merge/rebase plans may mention. -/
inductive SemanticRefKind where
  | commit
  | moduleRef
  | schemaObject
  | relationObject
  | roleProjection
  | subtypeInclusion
  | theoryObligation
  | theorySubject
  | pathExpression
  | rewriteRule
  | transportItem
  | instanceFunctor
  | stableFact
  | contextWorld
  | competencyQuestion
  | behaviorCase
  | implementationSurface
  | evidence
  | backendProjection
  | explicitIrRef
  deriving Repr, BEq, DecidableEq

/--
A stable semantic reference.  The `id` is intentionally opaque here: Rust owns
deterministic id construction; Lean checks relations among already-declared ids.
-/
structure SemanticRef where
  kind : SemanticRefKind
  id : String
  deriving Repr, BEq, DecidableEq

/--
The indices under which slice, merge, and rebase claims are scoped.  Empty
strings are allowed only for scaffold/tests; runtime reports should fill these
from accepted anchors and compiled IR digests.
-/
structure SemanticAnchor where
  acceptedRef : String
  acceptedSnapshotId : String
  kernelIrDigest : String
  contextId : Option String := none
  worldId : Option String := none
  deriving Repr, BEq, DecidableEq

/--
Executable manifests are finite lists of refs.  The proof layer below denotes a
manifest as a predicate so lattice laws are stated without depending on a
particular Rust container representation.
-/
structure SemanticSliceManifest where
  anchor : SemanticAnchor
  refs : List SemanticRef
  deriving Repr, BEq, DecidableEq

/-- A semantic slice as a predicate over refs, indexed by one semantic anchor. -/
structure SemanticSlice where
  anchor : SemanticAnchor
  contains : SemanticRef → Prop

def SemanticSliceManifest.denote (m : SemanticSliceManifest) : SemanticSlice :=
  { anchor := m.anchor, contains := fun r => r ∈ m.refs }

def sameAnchor (a b : SemanticSlice) : Prop :=
  a.anchor = b.anchor

/-- Runtime slice inclusion: every ref in `a` is present in `b`. -/
def refSubset (a b : SemanticSlice) : Prop :=
  ∀ r, a.contains r → b.contains r

def disjoint (a b : SemanticSlice) : Prop :=
  ∀ r, a.contains r → b.contains r → False

def overlaps (a b : SemanticSlice) : Prop :=
  ∃ r, a.contains r ∧ b.contains r

/--
Finite operational join candidate: the union of known refs.  Runtime
materialization still needs gate checks; this is just the denotation of the
candidate result.
-/
def join (a b : SemanticSlice) : SemanticSlice :=
  { anchor := a.anchor, contains := fun r => a.contains r ∨ b.contains r }

/--
Finite operational meet candidate: the intersection of known refs.  This is a
meet over a declared finite support universe, not a claim about global ontology
truth.
-/
def meet (a b : SemanticSlice) : SemanticSlice :=
  { anchor := a.anchor, contains := fun r => a.contains r ∧ b.contains r }

theorem refSubset_refl (s : SemanticSlice) : refSubset s s := by
  intro r h
  exact h

theorem join_contains_left (a b : SemanticSlice) : refSubset a (join a b) := by
  intro r h
  exact Or.inl h

theorem join_contains_right (a b : SemanticSlice) : refSubset b (join a b) := by
  intro r h
  exact Or.inr h

theorem join_least
    (a b c : SemanticSlice)
    (ha : refSubset a c)
    (hb : refSubset b c) :
    refSubset (join a b) c := by
  intro r h
  cases h with
  | inl hl => exact ha r hl
  | inr hr => exact hb r hr

theorem meet_subset_left (a b : SemanticSlice) : refSubset (meet a b) a := by
  intro r h
  exact h.left

theorem meet_subset_right (a b : SemanticSlice) : refSubset (meet a b) b := by
  intro r h
  exact h.right

theorem meet_greatest
    (a b c : SemanticSlice)
    (ha : refSubset c a)
    (hb : refSubset c b) :
    refSubset c (meet a b) := by
  intro r h
  exact And.intro (ha r h) (hb r h)

theorem disjoint_not_overlaps (a b : SemanticSlice) :
    disjoint a b → ¬ overlaps a b := by
  intro hd ho
  rcases ho with ⟨r, ha, hb⟩
  exact hd r ha hb

inductive TrustClass where
  | certifiableFragment
  | runtimeChecked
  | reviewOnly
  | evidenceBacked
  deriving Repr, BEq, DecidableEq

inductive MergeBlockerKind where
  | conflict
  | resolverStep
  | qualityGate
  | competencyGate
  | competencyQuestionRegression
  | trustRegression
  | coverageRegression
  | runtimeTheory
  | residualObligation
  | staleTarget
  | missingSupport
  | previewNotOk
  deriving Repr, BEq, DecidableEq

structure ResolverStep where
  handleId : String
  touchedRefs : List SemanticRef
  required : Bool := true
  deriving Repr, BEq, DecidableEq

structure SemanticMergePlan where
  base : SemanticSlice
  left : SemanticSlice
  right : SemanticSlice
  result : SemanticSlice
  blockers : List MergeBlockerKind
  resolverSteps : List ResolverStep
  residualObligations : List SemanticRef
  trustClass : TrustClass

def SemanticMergePlan.canMaterialize (p : SemanticMergePlan) : Prop :=
  p.blockers = [] ∧ p.resolverSteps = [] ∧ p.residualObligations = []

def SemanticMergePlan.preservesLeft (p : SemanticMergePlan) : Prop :=
  refSubset p.left p.result

def SemanticMergePlan.preservesRight (p : SemanticMergePlan) : Prop :=
  refSubset p.right p.result

/-- The conservative auto-join plan used for disjoint/idempotent runtime slices. -/
def conservativeJoinPlan (base left right : SemanticSlice) : SemanticMergePlan :=
  { base := base
    left := left
    right := right
    result := join left right
    blockers := []
    resolverSteps := []
    residualObligations := []
    trustClass := .runtimeChecked }

theorem conservative_join_preserves_left
    (base left right : SemanticSlice) :
    (conservativeJoinPlan base left right).preservesLeft := by
  exact join_contains_left left right

theorem conservative_join_preserves_right
    (base left right : SemanticSlice) :
    (conservativeJoinPlan base left right).preservesRight := by
  exact join_contains_right left right

theorem conservative_join_can_materialize
    (base left right : SemanticSlice) :
    (conservativeJoinPlan base left right).canMaterialize := by
  exact And.intro rfl (And.intro rfl rfl)

/--
A merge plan packaged with the preservation and materialization proofs expected
from a certified conservative merge fragment.
-/
structure CheckedMergePlan where
  plan : SemanticMergePlan
  canMaterialize : plan.canMaterialize
  preservesLeft : plan.preservesLeft
  preservesRight : plan.preservesRight

def checkedConservativeJoinPlan
    (base left right : SemanticSlice) : CheckedMergePlan :=
  { plan := conservativeJoinPlan base left right
    canMaterialize := conservative_join_can_materialize base left right
    preservesLeft := conservative_join_preserves_left base left right
    preservesRight := conservative_join_preserves_right base left right }

/--
Executable materialization gate for a Lean-readable merge plan.  This mirrors
the Rust fail-closed policy: blockers, resolver steps, or residual obligations
all prevent materialization.
-/
def checkMergePlanMaterialization (p : SemanticMergePlan) : Except String Unit :=
  if _hb : p.blockers = [] then
    if _hr : p.resolverSteps = [] then
      if _ho : p.residualObligations = [] then
        .ok ()
      else
        .error "merge plan has residual obligations"
    else
      .error "merge plan has unresolved resolver steps"
  else
    .error "merge plan has blockers"

theorem checkMergePlanMaterialization_sound
    (p : SemanticMergePlan)
    (h : checkMergePlanMaterialization p = .ok ()) :
    p.canMaterialize := by
  unfold checkMergePlanMaterialization at h
  by_cases hb : p.blockers = []
  · simp [hb] at h
    by_cases hr : p.resolverSteps = []
    · simp [hr] at h
      by_cases ho : p.residualObligations = []
      · exact And.intro hb (And.intro hr ho)
      · simp [ho] at h
    · simp [hr] at h
  · simp [hb] at h

inductive TransportStatus where
  | preserved
  | transported
  | missingObjectImage
  | missingArrowImage
  | opaqueOrOutOfFragment
  | blocked
  deriving Repr, BEq, DecidableEq

def TransportStatus.successful : TransportStatus → Bool
  | .preserved => true
  | .transported => true
  | .missingObjectImage => false
  | .missingArrowImage => false
  | .opaqueOrOutOfFragment => false
  | .blocked => false

structure TransportItem where
  sourceRef : SemanticRef
  targetRef : Option SemanticRef
  status : TransportStatus
  required : Bool := true
  deriving Repr, BEq, DecidableEq

def TransportItem.succeeds (i : TransportItem) : Prop :=
  i.status.successful = true

def hasFailedRequiredTransport : List TransportItem → Bool
  | [] => false
  | item :: rest =>
      (item.required && !item.status.successful) || hasFailedRequiredTransport rest

theorem hasFailedRequiredTransport_false_sound :
    ∀ items,
      hasFailedRequiredTransport items = false →
      ∀ item, item ∈ items → item.required = true → item.succeeds
  | [], _, item, hin, _ => by
      cases hin
  | head :: tail, h, item, hin, hreq => by
      simp [hasFailedRequiredTransport] at h
      simp at hin
      rcases hin with rfl | hinTail
      ·
          simpa [TransportItem.succeeds, hreq] using h.1
      ·
          exact hasFailedRequiredTransport_false_sound tail h.2 item hinTail hreq

structure SemanticRebasePlan where
  source : SemanticSlice
  onto : SemanticSlice
  result : SemanticSlice
  transportItems : List TransportItem
  blockers : List MergeBlockerKind
  resolverSteps : List ResolverStep
  residualObligations : List SemanticRef

def SemanticRebasePlan.canMaterialize (p : SemanticRebasePlan) : Prop :=
  p.blockers = [] ∧ p.resolverSteps = [] ∧ p.residualObligations = []

def SemanticRebasePlan.requiredTransportsSucceed (p : SemanticRebasePlan) : Prop :=
  ∀ item, item ∈ p.transportItems → item.required = true → item.succeeds

/--
The minimal preservation statement expected of a materializable rebase: required
transport items are preserved or transported, and no typed blockers remain.
-/
def SemanticRebasePlan.transportPreservationClaim (p : SemanticRebasePlan) : Prop :=
  p.canMaterialize ∧ p.requiredTransportsSucceed

theorem rebase_preservation_from_parts
    (p : SemanticRebasePlan)
    (hm : p.canMaterialize)
    (ht : p.requiredTransportsSucceed) :
    p.transportPreservationClaim := by
  exact And.intro hm ht

structure CheckedRebasePlan where
  plan : SemanticRebasePlan
  canMaterialize : plan.canMaterialize
  requiredTransportsSucceed : plan.requiredTransportsSucceed
  transportPreservation : plan.transportPreservationClaim

def checkRebasePlanMaterialization (p : SemanticRebasePlan) : Except String Unit :=
  if _hb : p.blockers = [] then
    if _hr : p.resolverSteps = [] then
      if _ho : p.residualObligations = [] then
        if _ht : hasFailedRequiredTransport p.transportItems = false then
          .ok ()
        else
          .error "rebase plan has failed required transports"
      else
        .error "rebase plan has residual obligations"
    else
      .error "rebase plan has unresolved resolver steps"
  else
    .error "rebase plan has blockers"

theorem checkRebasePlanMaterialization_sound
    (p : SemanticRebasePlan)
    (h : checkRebasePlanMaterialization p = .ok ()) :
    p.canMaterialize := by
  unfold checkRebasePlanMaterialization at h
  by_cases hb : p.blockers = []
  · simp [hb] at h
    by_cases hr : p.resolverSteps = []
    · simp [hr] at h
      by_cases ho : p.residualObligations = []
      · simp [ho] at h
        by_cases ht : hasFailedRequiredTransport p.transportItems = false
        · simp [ht] at h
          exact And.intro hb (And.intro hr ho)
        · simp [ht] at h
      · simp [ho] at h
    · simp [hr] at h
  · simp [hb] at h

theorem checkRebasePlanMaterialization_transport_sound
    (p : SemanticRebasePlan)
    (h : checkRebasePlanMaterialization p = .ok ()) :
    p.requiredTransportsSucceed := by
  unfold checkRebasePlanMaterialization at h
  by_cases hb : p.blockers = []
  · simp [hb] at h
    by_cases hr : p.resolverSteps = []
    · simp [hr] at h
      by_cases ho : p.residualObligations = []
      · simp [ho] at h
        by_cases ht : hasFailedRequiredTransport p.transportItems = false
        · simp [ht] at h
          exact hasFailedRequiredTransport_false_sound p.transportItems ht
        · simp [ht] at h
      · simp [ho] at h
    · simp [hr] at h
  · simp [hb] at h

/-- Operations that should eventually reduce to the same typed preservation vocabulary. -/
inductive SemanticOperationKind where
  | query
  | competencyQuestion
  | behaviorCase
  | authoringDelta
  | migration
  | reconciliation
  | merge
  | rebase
  | promotion
  | supersede
  | retract
  | backendProjection
  | embeddingEvidenceLift
  deriving Repr, BEq, DecidableEq

structure OperationPreservationClaim where
  operation : SemanticOperationKind
  input : SemanticSlice
  output : SemanticSlice
  assumptions : List String
  blockers : List MergeBlockerKind

def OperationPreservationClaim.wellScoped (c : OperationPreservationClaim) : Prop :=
  c.input.anchor = c.output.anchor ∨ c.operation = .rebase ∨ c.operation = .migration

def OperationPreservationClaim.preservesInputRefs (c : OperationPreservationClaim) : Prop :=
  refSubset c.input c.output

inductive OperationCheckLevel where
  | runtimeOnly
  | leanScaffold
  | verifierBoundary
  deriving Repr, BEq, DecidableEq

/--
The common target shape for future operation certificates.  A checked operation
must be well-scoped; operations that claim preservation also carry the explicit
ref-subset proof.  This is intentionally separate from `VerifyMain` until a
certificate parser/checker consumes it.
-/
structure CheckedOperationPreservation where
  claim : OperationPreservationClaim
  level : OperationCheckLevel
  wellScoped : claim.wellScoped
  preservesInputRefs : claim.preservesInputRefs

def checkedJoinOperation
    (operation : SemanticOperationKind)
    (input other : SemanticSlice)
    (assumptions : List String := ["finite operational slice join"]) :
    CheckedOperationPreservation :=
  let output := join input other
  let claim : OperationPreservationClaim :=
    { operation := operation
      input := input
      output := output
      assumptions := assumptions
      blockers := [] }
  have hs : claim.wellScoped := by
    unfold OperationPreservationClaim.wellScoped claim output
    simp [join]
  have hp : claim.preservesInputRefs := by
    unfold OperationPreservationClaim.preservesInputRefs claim output
    exact join_contains_left input other
  { claim := claim
    level := .leanScaffold
    wellScoped := hs
    preservesInputRefs := hp }

end SemanticVCS
end Axiograph
