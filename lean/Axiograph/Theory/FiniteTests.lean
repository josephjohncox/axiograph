import Axiograph.Theory.Finite

namespace Axiograph.Theory.FiniteTests

open Axiograph.Theory.Finite

private def person : Fin 3 := ⟨0, by decide⟩
private def parentFact : Fin 3 := ⟨1, by decide⟩
private def context : Fin 3 := ⟨2, by decide⟩

private def validCore : PresentationCore := {
  objectNames := #["Person", "ParentFact", "Context"]
  arrows := #[
    { name := "child", source := parentFact, target := person, kind := .projection },
    { name := "parent", source := parentFact, target := person, kind := .projection },
    { name := "mentor", source := person, target := person, kind := .function }
  ]
}

private def parentRelation : RelationObjectDecl validCore := {
  name := "Parent"
  object := parentFact
  roles := #[
    { name := "child", target := person, projection := ⟨0, by decide⟩,
      declaredOrder := 0, kind := .data },
    { name := "parent", target := person, projection := ⟨1, by decide⟩,
      declaredOrder := 1, kind := .data }
  ]
}

private def childTypedRole : TypedRole validCore := {
  name := "child"
  kind := .data
  declaredOrder := 0
  relationObject := parentFact
  target := person
  projection := .generator ⟨0, by decide⟩
}

private def validPresentation : Presentation := {
  core := validCore
  relations := #[parentRelation]
  equations := #[{
    name := "mentor_left_identity"
    lhs := .trans (.identity 0) (.generator 2)
    rhs := .generator 2
  }]
}

private def badProjectionRelation : RelationObjectDecl validCore := {
  name := "BadParent"
  object := parentFact
  roles := #[
    -- `child` targets Person, not Context. Validation must reject this role.
    { name := "ctx", target := context, projection := ⟨0, by decide⟩,
      declaredOrder := 0, kind := .context }
  ]
}

private def badProjectionPresentation : Presentation := {
  core := validCore
  relations := #[badProjectionRelation]
  equations := #[]
}

private def badRoleOrderRelation : RelationObjectDecl validCore := {
  name := "BadOrder"
  object := parentFact
  roles := #[
    { name := "child", target := person, projection := ⟨0, by decide⟩,
      declaredOrder := 1, kind := .data },
    { name := "parent", target := person, projection := ⟨1, by decide⟩,
      declaredOrder := 0, kind := .data }
  ]
}

private def badRoleOrderPresentation : Presentation := {
  core := validCore
  relations := #[badRoleOrderRelation]
  equations := #[]
}

private def badEquationPresentation : Presentation := {
  core := validCore
  relations := #[parentRelation]
  equations := #[{
    name := "non_composable"
    lhs := .trans (.generator 0) (.generator 0)
    rhs := .generator 0
  }]
}

private def tinyModel : FiniteInterpretation validCore := {
  carrierSize := fun _ => 1
  arrowMap := fun _ _ => ⟨0, by decide⟩
}

private def allVisible : ContextFamily tinyModel := {
  contextNames := #["accepted", "review"]
  visible := fun _ _ _ => true
}

private def ensure (condition : Bool) (message : String) : IO Unit :=
  if condition then pure () else throw (IO.userError message)

private def assertValidPresentation : IO Unit := do
  match validatePresentation validPresentation with
  | .ok _ => pure ()
  | .error errors => throw (IO.userError s!"valid presentation rejected: {repr errors}")
  match compileEquations validPresentation with
  | .ok equations => do
      ensure (equations.size == 1)
        "valid presentation did not compile its typed equation"
  | .error err => throw (IO.userError s!"valid equation rejected: {repr err}")

private def assertCanonicalAxiDerivation : IO Unit := do
  let axi := "module Family\n\nschema Family:\n  object Person\n  object Context\n  relation Parent(child: Person, parent: Person, ctx: Context @context)\n  relation Review(parent_fact: relation(Parent) @evidence)\n  function manager: Person -> Person\n\ntheory FamilyTheory on Family:\n  equation manager_idempotent:\n    manager;manager = manager\n"
  let module ←
    match Axiograph.Axi.SchemaV1.parseSchemaV1 axi with
    | .ok module => pure module
    | .error err => throw (IO.userError s!"canonical .axi fixture rejected: {repr err}")
  let some schema := module.schemas[0]?
    | throw (IO.userError "canonical .axi fixture omitted schema")
  let presentation ←
    match compileAxiSchemaPresentation module schema with
    | .ok presentation => pure presentation
    | .error errors => throw (IO.userError s!"category projection rejected: {repr errors}")
  ensure (presentation.core.objectNames == #["Person", "Context", "Parent", "Review"])
    "canonical derivation did not reify every relation as a category object"
  ensure (presentation.core.arrows.map (·.name) ==
      #["Parent.child", "Parent.parent", "Parent.ctx", "Review.parent_fact", "manager"])
    "canonical derivation did not emit ordered projections and explicit generators"
  let some parent := presentation.relations[0]?
    | throw (IO.userError "canonical derivation lost Parent relation object")
  let some review := presentation.relations[1]?
    | throw (IO.userError "canonical derivation lost Review relation object")
  ensure (parent.roles.map (·.declaredOrder) == #[0, 1, 2])
    "canonical derivation changed declared Parent role order"
  ensure (match review.roles[0]? with
    | some role => role.target.val == 2
    | none => false)
    "relation-valued role did not target the Parent relation object"
  let manifest ←
    match categoryKernelPresentationV3 presentation with
    | .ok manifest => pure manifest
    | .error errors => throw (IO.userError s!"category manifest rejected: {repr errors}")
  ensure (manifest.identityObjects == #[0, 1, 2, 3])
    "canonical category manifest omitted explicit identities"
  ensure (manifest.equations.size == 1)
    "canonical category manifest omitted the typed path equation"
  ensure (match manifest.arrows[4]? with
    | some arrow => arrow.kind == .function
    | none => false)
    "canonical category manifest lost explicit function kind"

private def assertDependentWitnesses : IO Unit := do
  let relation ←
    match compileRelation validCore parentRelation with
    | .ok relation => pure relation
    | .error err => throw (IO.userError s!"typed relation rejected: {repr err}")
  ensure (relation.roles.size == 2) "compiled relation omitted projection roles"
  let witness : RoleWitness tinyModel childTypedRole := {
    tuple := ⟨0, by simp [tinyModel]⟩
    value := ⟨0, by simp [tinyModel]⟩
    projectionHolds := rfl
  }
  ensure (witness.value.val == 0) "dependent role witness projected wrong value"

  let accepted : Fin allVisible.contextNames.size := ⟨0, by decide⟩
  let value : ContextValue allVisible accepted person := {
    value := ⟨0, by decide⟩
    visible := rfl
  }
  let transported := transportContextValue (ContextTransport.identity allVisible accepted) value
  ensure (transported.value.val == value.value.val) "identity context transport changed value"

  let candidate : PathCandidate validCore person person := {
    path := .generator ⟨2, by decide⟩
    explanation := "mentor generator"
  }
  let hole : TypedPathHole validCore person person := {
    holeId := "path-hole:person:person"
    candidates := #[candidate]
    residualObligations := #[residualAt "path-hole:person:person" .typedHole
      "caller must select an admissible typed path" (some person.val) (some person.val)]
  }
  ensure (hole.lifecycle == .residual) "open typed hole lost residual lifecycle"
  ensure (hole.candidates.size == 1) "typed hole lost candidate"
  let some selected := hole.select? 0
    | throw (IO.userError "typed hole candidate did not resolve")
  ensure (selected.lifecycle == .explanationVerified)
    "selected typed path did not enter checked lifecycle"

private def assertRefinements : IO Unit := do
  match compileRefinement 3 "refinement:ok" (.memberOf #[0, 2]) with
  | .ok refinement =>
      ensure (decide (refinement.Holds (⟨2, by decide⟩ : Fin 3)))
        "finite membership refinement rejected an allowed value"
  | .error err => throw (IO.userError s!"finite refinement rejected: {repr err}")

  match compileRefinement 3 "refinement:oob" (.equals 4) with
  | .error err => do
      ensure (err.kind == .invalidRefinementValue)
        "out-of-bounds refinement had wrong residual kind"
  | .ok _ => throw (IO.userError "out-of-bounds refinement was accepted")

  match compileRefinement 3 "refinement:opaque" (.unsupported "arbitrary_user_predicate") with
  | .error err => do
      ensure (err.kind == .unsupportedRefinement)
        "unsupported refinement had wrong residual kind"
  | .ok _ => throw (IO.userError "unsupported refinement was accepted")

private def assertFiniteSaturation : IO Unit := do
  let report := saturateFinite validPresentation 8
  ensure report.complete "finite saturation did not report completion"
  let some certificate := report.certificate
    | throw (IO.userError "finite saturation omitted explanation certificate")
  match verifySaturationCertificate validPresentation certificate with
  | .ok _ => pure ()
  | .error err => throw (IO.userError s!"generated certificate rejected: {repr err}")
  ensure ((reachabilityEntry? certificate parentFact.val person.val).isSome)
    "projection reachability missing from finite saturation"
  ensure ((reachabilityEntry? certificate person.val person.val).isSome)
    "identity reachability missing from finite saturation"

  let tampered : SaturationCertificate := {
    certificate with
    entries := certificate.entries.push {
      source := person.val
      target := context.val
      explanation := .identity person.val
    }
  }
  match verifySaturationCertificate validPresentation tampered with
  | .error err => do
      ensure (err.kind == .malformedExplanation)
        "tampered explanation had wrong residual kind"
  | .ok _ => throw (IO.userError "tampered explanation certificate was accepted")

  let some first := certificate.entries[0]?
    | throw (IO.userError "generated saturation certificate had no identity")
  let duplicate : SaturationCertificate := {
    certificate with entries := certificate.entries.push first
  }
  match verifySaturationCertificate validPresentation duplicate with
  | .error err =>
      ensure (err.kind == .incompleteCertificate)
        "duplicate endpoint entry had wrong residual kind"
  | .ok _ => throw (IO.userError "duplicate endpoint entry was accepted")

  let missingIdentity : SaturationCertificate := {
    certificate with
    entries := certificate.entries.filter (fun entry =>
      !(entry.source == person.val && entry.target == person.val))
  }
  match verifySaturationCertificate validPresentation missingIdentity with
  | .error err =>
      ensure (err.kind == .incompleteCertificate)
        "missing identity had wrong residual kind"
  | .ok _ => throw (IO.userError "certificate missing an identity was accepted")

  let bounded := saturateFinite validPresentation 2
  ensure (!bounded.complete) "configured finite saturation bound was ignored"
  ensure (bounded.certificate.isNone) "bounded-out saturation emitted a completion certificate"
  ensure (bounded.residualObligations.any (fun item => item.kind == .saturationBound))
    "bounded-out saturation omitted its residual obligation"

private def assertAdversarialPresentations : IO Unit := do
  match validatePresentation badProjectionPresentation with
  | .error errors => do
      ensure (errors.any (fun err => err.kind == .invalidProjection))
        "bad projection had wrong residual kind"
  | .ok _ => throw (IO.userError "relation role with wrong projection target was accepted")

  match validatePresentation badRoleOrderPresentation with
  | .error errors => do
      ensure (errors.any (fun err => err.obligationId == "role-order:BadOrder"))
        "swapped role order had no order-indexed residual"
  | .ok _ => throw (IO.userError "swapped role order was accepted")

  match validatePresentation badEquationPresentation with
  | .error errors => do
      ensure (errors.any (fun err => err.kind == .illTypedEquation))
        "non-composable equation had wrong residual kind"
  | .ok _ => throw (IO.userError "non-composable path equation was accepted")

private def assertRegulatedShipmentUsefulness : IO Unit := do
  let axi ← IO.FS.readFile "../examples/regulated_shipment/RegulatedShipment.axi"
  let module ←
    match Axiograph.Axi.SchemaV1.parseSchemaV1 axi with
    | .ok module => pure module
    | .error err => throw (IO.userError s!"regulated-shipment .axi rejected: {repr err}")
  let some schema := module.schemas.find? (fun schema => schema.name == "RegulatedShipment")
    | throw (IO.userError "regulated-shipment module omitted its schema")
  let presentation ←
    match compileAxiSchemaPresentation module schema with
    | .ok presentation => pure presentation
    | .error errors =>
        throw (IO.userError s!"regulated-shipment presentation rejected: {repr errors}")
  let manifest ←
    match categoryKernelPresentationV3 presentation with
    | .ok manifest => pure manifest
    | .error errors =>
        throw (IO.userError s!"regulated-shipment manifest rejected: {repr errors}")
  ensure (manifest.objectNames.size == 23)
    "regulated-shipment relation objects were not part of the canonical object order"
  ensure (manifest.arrows.size == 43)
    "regulated-shipment ordered projections or explicit functions were omitted"
  ensure (manifest.identityObjects == Array.range manifest.objectNames.size)
    "regulated-shipment explicit identity family drifted"
  ensure (manifest.equations.size == 1)
    "supported regulated-shipment forward equation was not compiled"
  let some dispatch := manifest.relations.find? (fun relation => relation.name == "DispatchReview")
    | throw (IO.userError "regulated-shipment manifest omitted DispatchReview")
  let some containedBatch := dispatch.roles.find? (fun role => role.name == "contained_batch")
    | throw (IO.userError "DispatchReview omitted contained_batch")
  ensure (manifest.objectNames[containedBatch.target]? == some "ShipmentContainsBatch")
    "relation-valued projection did not target ShipmentContainsBatch"
  ensure (dispatch.roles.map (·.declaredOrder) == #[0, 1, 2, 3])
    "DispatchReview projection declaration order drifted"

  let some equation := manifest.equations[0]?
    | throw (IO.userError "regulated-shipment manifest omitted its forward equation")
  let some prefixIndex := manifest.arrows.findIdx?
      (fun arrow => arrow.name == "ShipmentUsesLane.lane")
    | throw (IO.userError "regulated-shipment manifest omitted the Lane projection prefix")
  let some prefixArrow := manifest.arrows[prefixIndex]?
    | throw (IO.userError "regulated-shipment prefix index was out of range")
  let contextual : CategoryKernelCongruenceCertificateV3 := {
    input := {
      source := prefixArrow.source
      target := equation.lhs.target
      arrows := #[prefixIndex] ++ equation.lhs.arrows
    }
    steps := #[{ equation := 0, direction := .forward, offset := 1 }]
    output := {
      source := prefixArrow.source
      target := equation.rhs.target
      arrows := #[prefixIndex] ++ equation.rhs.arrows
    }
  }
  match verifyCategoryKernelCongruenceV3 manifest #[contextual] with
  | .ok _ => pure ()
  | .error err =>
      throw (IO.userError s!"regulated-shipment congruence replay failed: {repr err}")
  let badCongruence : CategoryKernelCongruenceCertificateV3 := {
    contextual with steps := #[{ equation := 0, direction := .forward, offset := 0 }]
  }
  match verifyCategoryKernelCongruenceV3 manifest #[badCongruence] with
  | .error err =>
      ensure (err.kind == .malformedExplanation)
        "bad congruence offset had the wrong residual kind"
  | .ok _ => throw (IO.userError "bad congruence offset was accepted")

  match compileRefinement 3 "regulated-reviewers" (.memberOf #[0, 1]) with
  | .ok allowedReviewers => do
      ensure (decide (allowedReviewers.Holds (⟨0, by decide⟩ : Fin 3)))
        "allowed regulated-shipment reviewer was rejected"
      ensure (!(decide (allowedReviewers.Holds (⟨2, by decide⟩ : Fin 3))))
        "reviewer outside the finite refinement was accepted"
  | .error err =>
      throw (IO.userError s!"regulated-shipment reviewer refinement rejected: {repr err}")

  let report := saturateFinite presentation 64
  ensure report.complete "regulated-shipment finite reachability did not complete"
  let some reachability := report.certificate
    | throw (IO.userError "regulated-shipment saturation omitted its explanation certificate")
  match verifySaturationCertificate presentation reachability with
  | .ok _ => pure ()
  | .error err =>
      throw (IO.userError s!"regulated-shipment explanation replay failed: {repr err}")
  let some lane := manifest.objectNames.findIdx? (· == "Lane")
    | throw (IO.userError "regulated-shipment manifest omitted Lane")
  let some jurisdiction := manifest.objectNames.findIdx? (· == "Jurisdiction")
    | throw (IO.userError "regulated-shipment manifest omitted Jurisdiction")
  ensure ((reachabilityEntry? reachability lane jurisdiction).isSome)
    "lane-to-jurisdiction path was absent from finite saturation"

  let tampered : SaturationCertificate := {
    reachability with
    entries := reachability.entries.push {
      source := lane
      target := 0
      explanation := .identity lane
    }
  }
  match verifySaturationCertificate presentation tampered with
  | .error err =>
      ensure (err.kind == .malformedExplanation || err.kind == .incompleteCertificate)
        "tampered regulated-shipment explanation had the wrong residual kind"
  | .ok _ =>
      throw (IO.userError "tampered regulated-shipment explanation was accepted")

/-- The test executable intentionally runs both positive and adversarial cases.
Any missed rejection exits nonzero. -/
def run : IO UInt32 := do
  assertValidPresentation
  assertCanonicalAxiDerivation
  assertDependentWitnesses
  assertRefinements
  assertFiniteSaturation
  assertAdversarialPresentations
  assertRegulatedShipmentUsefulness
  IO.println "ok: finite category/dependent/groupoid theory tests passed (including regulated shipment)"
  pure 0

end Axiograph.Theory.FiniteTests

def main : IO UInt32 :=
  Axiograph.Theory.FiniteTests.run
