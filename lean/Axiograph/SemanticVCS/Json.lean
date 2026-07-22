import Lean
import Axiograph.SemanticVCS

/-!
JSON-facing parsers for the finite semantic VCS scaffold.

These parsers are intentionally small and strict.  They are not imported by the
trusted verifier executable yet; they define the Lean-readable payload shape
that Rust should target when exporting merge/rebase plan certificates.
-/

namespace Axiograph
namespace SemanticVCS
namespace Json

open Lean

def parseStringArray (j : Lean.Json) : Except String (List String) := do
  let arr ← j.getArr?
  let mut out : Array String := #[]
  for item in arr do
    out := out.push (← item.getStr?)
  pure out.toList

def parseArray {α : Type} (j : Lean.Json) (f : Lean.Json → Except String α) :
    Except String (List α) := do
  let arr ← j.getArr?
  let mut out : Array α := #[]
  for item in arr do
    out := out.push (← f item)
  pure out.toList

def optionalField (j : Lean.Json) (field : String) : Except String (Option Lean.Json) :=
  match j.getObjVal? field with
  | .ok value => pure (some value)
  | .error _ => pure none

def parseOptionalString (j : Lean.Json) (field : String) : Except String (Option String) := do
  match (← optionalField j field) with
  | none => pure none
  | some value => pure (some (← value.getStr?))

def parseOptionalBoolDefault
    (j : Lean.Json)
    (field : String)
    (default : Bool) : Except String Bool := do
  match (← optionalField j field) with
  | none => pure default
  | some value => fromJson? value

def parseSemanticRefKind (s : String) : Except String SemanticRefKind := do
  match s with
  | "commit" => pure .commit
  | "module_ref" => pure .moduleRef
  | "schema_object" => pure .schemaObject
  | "relation_object" => pure .relationObject
  | "role_projection" => pure .roleProjection
  | "subtype_inclusion" => pure .subtypeInclusion
  | "theory_obligation" => pure .theoryObligation
  | "theory_subject" => pure .theorySubject
  | "path_expression" => pure .pathExpression
  | "rewrite_rule" => pure .rewriteRule
  | "transport_item" => pure .transportItem
  | "instance_functor" => pure .instanceFunctor
  | "stable_fact" => pure .stableFact
  | "context_world" => pure .contextWorld
  | "competency_question" => pure .competencyQuestion
  | "behavior_case" => pure .behaviorCase
  | "implementation_surface" => pure .implementationSurface
  | "evidence" => pure .evidence
  | "backend_projection" => pure .backendProjection
  | "explicit_ir_ref" => pure .explicitIrRef
  | other => throw s!"unknown semantic ref kind `{other}`"

def parseSemanticRef (j : Lean.Json) : Except String SemanticRef := do
  let kind ← parseSemanticRefKind (← (← j.getObjVal? "kind").getStr?)
  let id ← (← j.getObjVal? "id").getStr?
  pure { kind, id }

def parseSemanticAnchor (j : Lean.Json) : Except String SemanticAnchor := do
  let acceptedRef ← (← j.getObjVal? "accepted_ref").getStr?
  let acceptedSnapshotId ← (← j.getObjVal? "accepted_snapshot_id").getStr?
  let kernelIrDigest ← (← j.getObjVal? "kernel_ir_digest").getStr?
  let contextId ← parseOptionalString j "context_id"
  let worldId ← parseOptionalString j "world_id"
  pure { acceptedRef, acceptedSnapshotId, kernelIrDigest, contextId, worldId }

def parseSemanticSliceManifest (j : Lean.Json) : Except String SemanticSliceManifest := do
  let anchor ← parseSemanticAnchor (← j.getObjVal? "anchor")
  let refs ← parseArray (← j.getObjVal? "refs") parseSemanticRef
  pure { anchor, refs }

def parseTrustClass (s : String) : Except String TrustClass := do
  match s with
  | "certifiable_fragment" => pure .certifiableFragment
  | "runtime_checked" => pure .runtimeChecked
  | "review_only" => pure .reviewOnly
  | "evidence_backed" => pure .evidenceBacked
  | other => throw s!"unknown trust class `{other}`"

def parseMergeBlockerKind (s : String) : Except String MergeBlockerKind := do
  match s with
  | "conflict" => pure .conflict
  | "resolver_step" => pure .resolverStep
  | "quality_gate" => pure .qualityGate
  | "competency_gate" => pure .competencyGate
  | "competency_question_regression" => pure .competencyQuestionRegression
  | "trust_regression" => pure .trustRegression
  | "coverage_regression" => pure .coverageRegression
  | "runtime_theory" => pure .runtimeTheory
  | "residual_obligation" => pure .residualObligation
  | "stale_target" => pure .staleTarget
  | "missing_support" => pure .missingSupport
  | "preview_not_ok" => pure .previewNotOk
  | other => throw s!"unknown merge blocker kind `{other}`"

def parseResolverStep (j : Lean.Json) : Except String ResolverStep := do
  let handleId ← (← j.getObjVal? "handle_id").getStr?
  let touchedRefs ← parseArray (← j.getObjVal? "touched_refs") parseSemanticRef
  let required ← parseOptionalBoolDefault j "required" true
  pure { handleId, touchedRefs, required }

def parseSemanticMergePlan (j : Lean.Json) : Except String SemanticMergePlan := do
  let baseManifest ← parseSemanticSliceManifest (← j.getObjVal? "base")
  let leftManifest ← parseSemanticSliceManifest (← j.getObjVal? "left")
  let rightManifest ← parseSemanticSliceManifest (← j.getObjVal? "right")
  let resultManifest ← parseSemanticSliceManifest (← j.getObjVal? "result")
  let blockers ← parseArray (← j.getObjVal? "blockers") (fun item => do
    parseMergeBlockerKind (← item.getStr?))
  let resolverSteps ← parseArray (← j.getObjVal? "resolver_steps") parseResolverStep
  let residualObligations ←
    parseArray (← j.getObjVal? "residual_obligations") parseSemanticRef
  let trustClass ← parseTrustClass (← (← j.getObjVal? "trust_class").getStr?)
  pure
    { base := baseManifest.denote
      left := leftManifest.denote
      right := rightManifest.denote
      result := resultManifest.denote
      blockers
      resolverSteps
      residualObligations
      trustClass }

def parseTransportStatus (s : String) : Except String TransportStatus := do
  match s with
  | "preserved" => pure .preserved
  | "transported" => pure .transported
  | "missing_object_image" => pure .missingObjectImage
  | "missing_arrow_image" => pure .missingArrowImage
  | "opaque_or_out_of_fragment" => pure .opaqueOrOutOfFragment
  | "blocked" => pure .blocked
  | other => throw s!"unknown transport status `{other}`"

def parseOptionalSemanticRef (j : Lean.Json) (field : String) :
    Except String (Option SemanticRef) := do
  match (← optionalField j field) with
  | none => pure none
  | some value => pure (some (← parseSemanticRef value))

def parseTransportItem (j : Lean.Json) : Except String TransportItem := do
  let sourceRef ← parseSemanticRef (← j.getObjVal? "source_ref")
  let targetRef ← parseOptionalSemanticRef j "target_ref"
  let status ← parseTransportStatus (← (← j.getObjVal? "status").getStr?)
  let required ← parseOptionalBoolDefault j "required" true
  pure { sourceRef, targetRef, status, required }

def parseSemanticRebasePlan (j : Lean.Json) : Except String SemanticRebasePlan := do
  let sourceManifest ← parseSemanticSliceManifest (← j.getObjVal? "source")
  let ontoManifest ← parseSemanticSliceManifest (← j.getObjVal? "onto")
  let resultManifest ← parseSemanticSliceManifest (← j.getObjVal? "result")
  let transportItems ← parseArray (← j.getObjVal? "transport_items") parseTransportItem
  let blockers ← parseArray (← j.getObjVal? "blockers") (fun item => do
    parseMergeBlockerKind (← item.getStr?))
  let resolverSteps ← parseArray (← j.getObjVal? "resolver_steps") parseResolverStep
  let residualObligations ←
    parseArray (← j.getObjVal? "residual_obligations") parseSemanticRef
  pure
    { source := sourceManifest.denote
      onto := ontoManifest.denote
      result := resultManifest.denote
      transportItems
      blockers
      resolverSteps
      residualObligations }

def refsSubsetBool (left right : List SemanticRef) : Bool :=
  left.all fun ref => right.contains ref

def refsSameSetBool (left right : List SemanticRef) : Bool :=
  refsSubsetBool left right && refsSubsetBool right left

def sameLineage (before after : SemanticAnchor) : Bool :=
  before.acceptedRef == after.acceptedRef &&
    before.contextId == after.contextId &&
    before.worldId == after.worldId

def transportTarget (item : TransportItem) : Option SemanticRef :=
  match item.status with
  | .preserved => some (item.targetRef.getD item.sourceRef)
  | .transported => item.targetRef
  | _ => none

def successfulTransportTargets : List TransportItem → List SemanticRef
  | [] => []
  | item :: rest =>
      match transportTarget item with
      | some target => target :: successfulTransportTargets rest
      | none => successfulTransportTargets rest

def transportsWellScoped
    (source result : SemanticSliceManifest)
    (items : List TransportItem) : Bool :=
  let everySourceCovered := source.refs.all fun ref =>
    items.any fun item => item.sourceRef == ref
  let everyItemValid := items.all fun item =>
    source.refs.contains item.sourceRef &&
      if item.status.successful then
        match transportTarget item with
        | some target => result.refs.contains target
        | none => false
      else
        true
  everySourceCovered && everyItemValid

def checkMergePlanJson (j : Lean.Json) : Except String Unit := do
  let base ← parseSemanticSliceManifest (← j.getObjVal? "base")
  let left ← parseSemanticSliceManifest (← j.getObjVal? "left")
  let right ← parseSemanticSliceManifest (← j.getObjVal? "right")
  let result ← parseSemanticSliceManifest (← j.getObjVal? "result")
  unless sameLineage base.anchor result.anchor do
    throw "merge result is not anchored to the base branch lineage"
  unless refsSameSetBool result.refs (left.refs ++ right.refs) do
    throw "merge plan result is not the exact finite join of left and right refs"
  checkMergePlanMaterialization (← parseSemanticMergePlan j)

def checkRebasePlanJson (j : Lean.Json) : Except String Unit := do
  let source ← parseSemanticSliceManifest (← j.getObjVal? "source")
  let onto ← parseSemanticSliceManifest (← j.getObjVal? "onto")
  let result ← parseSemanticSliceManifest (← j.getObjVal? "result")
  let items ← parseArray (← j.getObjVal? "transport_items") parseTransportItem
  unless sameLineage source.anchor result.anchor do
    throw "rebase result is not anchored to the source branch lineage"
  unless refsSubsetBool onto.refs result.refs do
    throw "rebase result does not preserve all onto refs"
  unless transportsWellScoped source result items do
    throw "rebase transport items do not cover source refs with targets in the result"
  unless refsSameSetBool result.refs (onto.refs ++ successfulTransportTargets items) do
    throw "rebase result contains refs outside the onto slice and successful transport targets"
  checkRebasePlanMaterialization (← parseSemanticRebasePlan j)

end Json
end SemanticVCS
end Axiograph
