import Std
import Axiograph.HoTT.FreeGroupoid
import Axiograph.Axi.SchemaV1

/-!
# Finite categorical and dependent theory kernel

This module implements the strongest deliberately finite fragment currently
claimed by Axiograph:

* a category presentation whose relations are objects and whose roles are
  projection arrows;
* endpoint-indexed category paths, accepted path equations, and free-groupoid
  path laws;
* finite interpretations with dependent role witnesses and finite refinements;
* context-indexed values and proof-carrying context transport;
* typed path holes and explicit residual obligations; and
* a bounded finite reachability saturation procedure whose every derived pair
  carries a replayable explanation certificate.

The saturation procedure proves only finite generator reachability. Presented
path equations identify parallel paths but do not create new endpoints. Nothing
here claims univalence, higher inductive types, arbitrary dependent products,
open-world ontology closure, topos/sheaf completeness, or termination of
user-authored rewrite systems.
-/

namespace Axiograph.Theory.Finite

-- =============================================================================
-- Category presentation: relation objects and projection arrows
-- =============================================================================

inductive ArrowKind where
  | projection
  | subtypeInclusion
  | aspect
  | function
  deriving Repr, DecidableEq

structure ArrowDecl (objectCount : Nat) where
  name : String
  source : Fin objectCount
  target : Fin objectCount
  kind : ArrowKind
  reversible : Bool := false
  deriving Repr, DecidableEq

structure PresentationCore where
  objectNames : Array String
  arrows : Array (ArrowDecl objectNames.size)
  deriving Repr

inductive RoleKind where
  | data
  | context
  | world
  | temporal
  | parameter
  | evidence
  deriving Repr, DecidableEq

structure RoleDecl (p : PresentationCore) where
  name : String
  target : Fin p.objectNames.size
  projection : Fin p.arrows.size
  /-- Zero-based order in the relation declaration. This is semantic data:
  role tuples and dependent witnesses must preserve it exactly. -/
  declaredOrder : Nat
  kind : RoleKind := .data
  deriving Repr

structure RelationObjectDecl (p : PresentationCore) where
  name : String
  object : Fin p.objectNames.size
  roles : Array (RoleDecl p)
  deriving Repr

inductive RawPath where
  | identity (object : Nat)
  | generator (arrow : Nat)
  | trans (left right : RawPath)
  deriving Repr, DecidableEq

structure RawEquation where
  name : String
  lhs : RawPath
  rhs : RawPath
  deriving Repr, DecidableEq

structure Presentation where
  core : PresentationCore
  relations : Array (RelationObjectDecl core)
  equations : Array RawEquation
  deriving Repr

/-- Lean-friendly, index-based projection of the canonical Rust
`SchemaPresentationIr`. The trusted checker reconstructs this exact structure
from anchored `.axi` before accepting compiler-emitted evidence. -/
structure CategoryKernelArrowV3 where
  name : String
  source : Nat
  target : Nat
  kind : ArrowKind
  reversible : Bool
  deriving Repr, DecidableEq

structure CategoryKernelRoleV3 where
  name : String
  target : Nat
  projection : Nat
  declaredOrder : Nat
  kind : RoleKind
  deriving Repr, DecidableEq

structure CategoryKernelRelationV3 where
  name : String
  object : Nat
  roles : Array CategoryKernelRoleV3
  deriving Repr, DecidableEq

structure CategoryKernelPathV3 where
  source : Nat
  target : Nat
  arrows : Array Nat
  deriving Repr, DecidableEq

inductive CategoryKernelFormalDirectionV3 where
  | forward
  | inverse
  deriving Repr, DecidableEq

def CategoryKernelFormalDirectionV3.opposite : CategoryKernelFormalDirectionV3 →
    CategoryKernelFormalDirectionV3
  | .forward => .inverse
  | .inverse => .forward

structure CategoryKernelFormalStepV3 where
  arrow : Nat
  direction : CategoryKernelFormalDirectionV3
  deriving Repr, DecidableEq

structure CategoryKernelFormalPathV3 where
  source : Nat
  target : Nat
  steps : Array CategoryKernelFormalStepV3
  deriving Repr, DecidableEq

structure CategoryKernelFormalRewriteStepV3 where
  offset : Nat
  arrow : Nat
  firstDirection : CategoryKernelFormalDirectionV3
  deriving Repr, DecidableEq

structure CategoryKernelFormalNormalizationV3 where
  input : CategoryKernelFormalPathV3
  rewriteTrace : Array CategoryKernelFormalRewriteStepV3
  normalized : CategoryKernelFormalPathV3
  deriving Repr, DecidableEq

structure CategoryKernelEquationV3 where
  name : String
  lhs : CategoryKernelPathV3
  rhs : CategoryKernelPathV3
  deriving Repr, DecidableEq

structure CategoryKernelPresentationV3 where
  objectNames : Array String
  arrows : Array CategoryKernelArrowV3
  relations : Array CategoryKernelRelationV3
  identityObjects : Array Nat
  equations : Array CategoryKernelEquationV3
  deriving Repr, DecidableEq

inductive EquationDirection where
  | forward
  | reverse
  deriving Repr, DecidableEq

structure CategoryKernelCongruenceStepV3 where
  equation : Nat
  direction : EquationDirection
  offset : Nat
  deriving Repr, DecidableEq

structure CategoryKernelCongruenceCertificateV3 where
  input : CategoryKernelPathV3
  steps : Array CategoryKernelCongruenceStepV3
  output : CategoryKernelPathV3
  deriving Repr, DecidableEq

-- =============================================================================
-- Endpoint-indexed paths and category equations
-- =============================================================================

inductive Path (p : PresentationCore) :
    Fin p.objectNames.size → Fin p.objectNames.size → Type where
  | identity (object : Fin p.objectNames.size) : Path p object object
  | generator (arrow : Fin p.arrows.size) :
      Path p (p.arrows[arrow].source) (p.arrows[arrow].target)
  | trans {a b c : Fin p.objectNames.size} : Path p a b → Path p b c → Path p a c

def Path.length {p : PresentationCore} {a b : Fin p.objectNames.size} : Path p a b → Nat
  | .identity _ => 0
  | .generator _ => 1
  | .trans left right => left.length + right.length

structure SomePath (p : PresentationCore) where
  source : Fin p.objectNames.size
  target : Fin p.objectNames.size
  path : Path p source target

structure CheckedEquation (p : PresentationCore) where
  name : String
  source : Fin p.objectNames.size
  target : Fin p.objectNames.size
  lhs : Path p source target
  rhs : Path p source target

inductive PathEquiv (p : PresentationCore) :
    {a b : Fin p.objectNames.size} → Path p a b → Path p a b → Type where
  | refl {a b} {path : Path p a b} : PathEquiv p path path
  | symm {a b} {left right : Path p a b} :
      PathEquiv p left right → PathEquiv p right left
  | transitive {a b} {first second third : Path p a b} :
      PathEquiv p first second → PathEquiv p second third → PathEquiv p first third
  | congr {a b c} {p₁ p₂ : Path p a b} {q₁ q₂ : Path p b c} :
      PathEquiv p p₁ p₂ → PathEquiv p q₁ q₂ →
      PathEquiv p (.trans p₁ q₁) (.trans p₂ q₂)
  | idLeft {a b} (path : Path p a b) :
      PathEquiv p (.trans (.identity a) path) path
  | idRight {a b} (path : Path p a b) :
      PathEquiv p (.trans path (.identity b)) path
  | assoc {a b c d} (first : Path p a b) (second : Path p b c)
      (third : Path p c d) :
      PathEquiv p (.trans (.trans first second) third)
        (.trans first (.trans second third))
  | presented (equation : CheckedEquation p) :
      PathEquiv p equation.lhs equation.rhs

-- =============================================================================
-- Free-groupoid completion and sound laws
-- =============================================================================

inductive GroupoidPath (p : PresentationCore) :
    Fin p.objectNames.size → Fin p.objectNames.size → Type where
  | identity (object : Fin p.objectNames.size) : GroupoidPath p object object
  | generator (arrow : Fin p.arrows.size) :
      GroupoidPath p (p.arrows[arrow].source) (p.arrows[arrow].target)
  | trans {a b c} : GroupoidPath p a b → GroupoidPath p b c → GroupoidPath p a c
  | inverse {a b} : GroupoidPath p a b → GroupoidPath p b a

open CategoryTheory
open Axiograph.HoTT

def GroupoidPath.denote {p : PresentationCore} {a b : Fin p.objectNames.size} :
    GroupoidPath p a b → (fgObj a.val ⟶ fgObj b.val)
  | .identity object => 𝟙 (fgObj object.val)
  | .generator arrow =>
      fgStep (p.arrows[arrow].source.val) arrow.val (p.arrows[arrow].target.val)
  | .trans left right => left.denote ≫ right.denote
  | .inverse path => Groupoid.inv path.denote

theorem GroupoidPath.denote_id_left {p : PresentationCore}
    {a b : Fin p.objectNames.size} (path : GroupoidPath p a b) :
    (GroupoidPath.trans (.identity a) path).denote = path.denote := by
  simp [GroupoidPath.denote]

theorem GroupoidPath.denote_id_right {p : PresentationCore}
    {a b : Fin p.objectNames.size} (path : GroupoidPath p a b) :
    (GroupoidPath.trans path (.identity b)).denote = path.denote := by
  simp [GroupoidPath.denote]

theorem GroupoidPath.denote_assoc {p : PresentationCore}
    {a b c d : Fin p.objectNames.size} (first : GroupoidPath p a b)
    (second : GroupoidPath p b c) (third : GroupoidPath p c d) :
    (GroupoidPath.trans (GroupoidPath.trans first second) third).denote =
      (GroupoidPath.trans first (GroupoidPath.trans second third)).denote := by
  simp [GroupoidPath.denote, Category.assoc]

theorem GroupoidPath.denote_congr_left {p : PresentationCore}
    {a b c : Fin p.objectNames.size} {left right : GroupoidPath p a b}
    (tail : GroupoidPath p b c) (equal : left.denote = right.denote) :
    (GroupoidPath.trans left tail).denote =
      (GroupoidPath.trans right tail).denote := by
  simp only [GroupoidPath.denote]
  rw [equal]

theorem GroupoidPath.denote_congr_right {p : PresentationCore}
    {a b c : Fin p.objectNames.size} (head : GroupoidPath p a b)
    {left right : GroupoidPath p b c} (equal : left.denote = right.denote) :
    (GroupoidPath.trans head left).denote =
      (GroupoidPath.trans head right).denote := by
  simp only [GroupoidPath.denote]
  rw [equal]

theorem GroupoidPath.denote_congr_inverse {p : PresentationCore}
    {a b : Fin p.objectNames.size} {left right : GroupoidPath p a b}
    (equal : left.denote = right.denote) :
    (GroupoidPath.inverse left).denote = (GroupoidPath.inverse right).denote := by
  simp only [GroupoidPath.denote]
  rw [equal]

theorem GroupoidPath.denote_inverse_identity {p : PresentationCore}
    (object : Fin p.objectNames.size) :
    (GroupoidPath.inverse (GroupoidPath.identity object)).denote =
      (GroupoidPath.identity object).denote := by
  simp [GroupoidPath.denote]

theorem GroupoidPath.denote_inverse_inverse {p : PresentationCore}
    {a b : Fin p.objectNames.size} (path : GroupoidPath p a b) :
    (GroupoidPath.inverse (GroupoidPath.inverse path)).denote = path.denote := by
  simp [GroupoidPath.denote]

theorem GroupoidPath.denote_inverse_trans {p : PresentationCore}
    {a b c : Fin p.objectNames.size} (left : GroupoidPath p a b)
    (right : GroupoidPath p b c) :
    (GroupoidPath.inverse (GroupoidPath.trans left right)).denote =
      (GroupoidPath.trans (GroupoidPath.inverse right) (GroupoidPath.inverse left)).denote := by
  simp [GroupoidPath.denote]

theorem GroupoidPath.denote_inverse_right {p : PresentationCore}
    {a b : Fin p.objectNames.size} (path : GroupoidPath p a b) :
    (GroupoidPath.trans path (.inverse path)).denote =
      (GroupoidPath.identity a).denote := by
  simp [GroupoidPath.denote]

theorem GroupoidPath.denote_inverse_left {p : PresentationCore}
    {a b : Fin p.objectNames.size} (path : GroupoidPath p a b) :
    (GroupoidPath.trans (.inverse path) path).denote =
      (GroupoidPath.identity b).denote := by
  simp [GroupoidPath.denote]

-- =============================================================================
-- Explicit residual obligations
-- =============================================================================

inductive ResidualKind where
  | duplicateName
  | invalidProjection
  | illTypedEquation
  | unsupportedRefinement
  | invalidRefinementValue
  | saturationBound
  | malformedExplanation
  | incompleteCertificate
  | typedHole
  deriving Repr, DecidableEq

structure ResidualObligation where
  obligationId : String
  kind : ResidualKind
  message : String
  source : Option Nat := none
  target : Option Nat := none
  deriving Repr, DecidableEq

def residual (id : String) (kind : ResidualKind) (message : String) : ResidualObligation :=
  { obligationId := id, kind, message }

def residualAt (id : String) (kind : ResidualKind) (message : String)
    (source target : Option Nat) : ResidualObligation :=
  { obligationId := id, kind, message, source, target }

-- =============================================================================
-- Compilation/checking of raw paths, equations, and relation projections
-- =============================================================================

def inferPath (p : PresentationCore) : RawPath → Except ResidualObligation (SomePath p)
  | .identity object =>
      if h : object < p.objectNames.size then
        let typed : Fin p.objectNames.size := ⟨object, h⟩
        .ok { source := typed, target := typed, path := .identity typed }
      else
        .error <| residualAt s!"path:identity:{object}" .illTypedEquation
          s!"identity object index {object} is outside the finite presentation"
          (some object) (some object)
  | .generator arrow =>
      if h : arrow < p.arrows.size then
        let typed : Fin p.arrows.size := ⟨arrow, h⟩
        .ok {
          source := p.arrows[typed].source
          target := p.arrows[typed].target
          path := .generator typed
        }
      else
        .error <| residual s!"path:generator:{arrow}" .illTypedEquation
          s!"generator index {arrow} is outside the finite presentation"
  | .trans left right => do
      let left ← inferPath p left
      let right ← inferPath p right
      if h : left.target = right.source then
        let rightPath : Path p left.target right.target := by
          simpa [h] using right.path
        pure {
          source := left.source
          target := right.target
          path := .trans left.path rightPath
        }
      else
        throw <| residualAt "path:composition" .illTypedEquation
          s!"path composition endpoint mismatch: {left.target.val} != {right.source.val}"
          (some left.target.val) (some right.source.val)

def checkEquation (p : PresentationCore) (equation : RawEquation) :
    Except ResidualObligation (CheckedEquation p) := do
  let lhs ← inferPath p equation.lhs
  let rhs ← inferPath p equation.rhs
  if hSource : lhs.source = rhs.source then
    if hTarget : lhs.target = rhs.target then
      let rhsPath : Path p lhs.source lhs.target := by
        simpa [hSource, hTarget] using rhs.path
      pure {
        name := equation.name
        source := lhs.source
        target := lhs.target
        lhs := lhs.path
        rhs := rhsPath
      }
    else
      throw <| residualAt s!"equation:{equation.name}" .illTypedEquation
        s!"equation `{equation.name}` has different targets"
        (some lhs.target.val) (some rhs.target.val)
  else
    throw <| residualAt s!"equation:{equation.name}" .illTypedEquation
      s!"equation `{equation.name}` has different sources"
      (some lhs.source.val) (some rhs.source.val)

structure TypedRole (p : PresentationCore) where
  name : String
  kind : RoleKind
  declaredOrder : Nat
  relationObject : Fin p.objectNames.size
  target : Fin p.objectNames.size
  projection : Path p relationObject target

structure TypedRelation (p : PresentationCore) where
  name : String
  object : Fin p.objectNames.size
  roles : Array (TypedRole p)

private def compileRole (p : PresentationCore) (relation : RelationObjectDecl p)
    (role : RoleDecl p) : Except ResidualObligation (TypedRole p) := do
  let arrow := p.arrows[role.projection]
  if arrow.kind != .projection then
    throw <| residualAt s!"role:{relation.name}:{role.name}" .invalidProjection
      s!"role `{role.name}` references non-projection arrow `{arrow.name}`"
      (some relation.object.val) (some role.target.val)
  if hSource : arrow.source = relation.object then
    if hTarget : arrow.target = role.target then
      let projection : Path p relation.object role.target := by
        rw [← hSource, ← hTarget]
        exact Path.generator (p := p) role.projection
      pure {
        name := role.name
        kind := role.kind
        declaredOrder := role.declaredOrder
        relationObject := relation.object
        target := role.target
        projection
      }
    else
      throw <| residualAt s!"role:{relation.name}:{role.name}" .invalidProjection
        s!"projection `{arrow.name}` target does not match role `{role.name}`"
        (some relation.object.val) (some role.target.val)
  else
    throw <| residualAt s!"role:{relation.name}:{role.name}" .invalidProjection
      s!"projection `{arrow.name}` does not start at relation object `{relation.name}`"
      (some relation.object.val) (some role.target.val)

def compileRelation (p : PresentationCore) (relation : RelationObjectDecl p) :
    Except ResidualObligation (TypedRelation p) := do
  let mut roles : Array (TypedRole p) := #[]
  for role in relation.roles do
    roles := roles.push (← compileRole p relation role)
  pure { name := relation.name, object := relation.object, roles }

private def duplicateStrings (values : Array String) : Array String := Id.run do
  let mut seen : Std.HashSet String := {}
  let mut duplicates : Array String := #[]
  for value in values do
    if seen.contains value && !duplicates.contains value then
      duplicates := duplicates.push value
    seen := seen.insert value
  duplicates

def validatePresentation (presentation : Presentation) :
    Except (Array ResidualObligation) Unit := do
  let mut errors : Array ResidualObligation := #[]
  for name in duplicateStrings presentation.core.objectNames do
    errors := errors.push <| residual s!"object:{name}" .duplicateName
      s!"duplicate object name `{name}`"
  for name in duplicateStrings (presentation.core.arrows.map (·.name)) do
    errors := errors.push <| residual s!"arrow:{name}" .duplicateName
      s!"duplicate arrow name `{name}`"
  for name in duplicateStrings (presentation.relations.map (·.name)) do
    errors := errors.push <| residual s!"relation:{name}" .duplicateName
      s!"duplicate relation-object name `{name}`"
  for name in duplicateStrings (presentation.equations.map (·.name)) do
    errors := errors.push <| residual s!"equation:{name}" .duplicateName
      s!"duplicate presented equation name `{name}`"
  let relationObjects := presentation.relations.map (fun relation => relation.object.val)
  for object in relationObjects do
    if (relationObjects.filter (· == object)).size > 1 then
      let objectName := presentation.core.objectNames[object]?
        |>.getD s!"object#{object}"
      let obligationId := s!"relation-object:{object}"
      if !errors.any (fun error => error.obligationId == obligationId) then
        errors := errors.push <| residual obligationId .duplicateName
          s!"multiple relation declarations use category object `{objectName}`"
  for relation in presentation.relations do
    for name in duplicateStrings (relation.roles.map (·.name)) do
      errors := errors.push <| residual s!"role:{relation.name}:{name}" .duplicateName
        s!"duplicate role `{name}` on relation object `{relation.name}`"
    let declaredOrders := relation.roles.map (·.declaredOrder)
    if declaredOrders != Array.range relation.roles.size then
      errors := errors.push <| residual s!"role-order:{relation.name}" .invalidProjection
        s!"relation object `{relation.name}` roles are not ordered exactly 0..{relation.roles.size}"
    let projectionIds := relation.roles.map (fun role => role.projection.val)
    for projection in projectionIds do
      if (projectionIds.filter (· == projection)).size > 1 then
        let obligationId := s!"role-projection:{relation.name}:{projection}"
        if !errors.any (fun error => error.obligationId == obligationId) then
          errors := errors.push <| residual obligationId .duplicateName
            s!"relation object `{relation.name}` reuses projection arrow index {projection}"
    match compileRelation presentation.core relation with
    | .ok _ => pure ()
    | .error err => errors := errors.push err
  for equation in presentation.equations do
    match checkEquation presentation.core equation with
    | .ok _ => pure ()
    | .error err => errors := errors.push err
  if errors.isEmpty then pure () else throw errors

-- =============================================================================
-- Derivation from the canonical `.axi` schema surface
-- =============================================================================

private def roleKindFromAxi : Axiograph.Axi.SchemaV1.RoleKindV1 → RoleKind
  | .data => .data
  | .context => .context
  | .world => .world
  | .temporal => .temporal
  | .parameter => .parameter
  | .evidence => .evidence

private def namedIndex (names : Array String) (name obligationId : String) :
    Except (Array ResidualObligation) (Fin names.size) := do
  let some index := names.findIdx? (· == name)
    | throw #[residual obligationId .invalidProjection
        s!"category presentation references unknown object `{name}`"]
  if h : index < names.size then pure ⟨index, h⟩
  else throw #[residual obligationId .invalidProjection
    s!"internal category-presentation index failure for `{name}`"]

private def arrowIndex (core : PresentationCore) (name obligationId : String) :
    Except (Array ResidualObligation) (Fin core.arrows.size) := do
  let some index := core.arrows.findIdx? (fun arrow => arrow.name == name)
    | throw #[residual obligationId .invalidProjection
        s!"category presentation references unknown projection arrow `{name}`"]
  if h : index < core.arrows.size then pure ⟨index, h⟩
  else throw #[residual obligationId .invalidProjection
    s!"internal projection-arrow index failure for `{name}`"]

private def stripSuffix? (value suffix : String) : Option String :=
  let valueChars := value.toList
  let suffixChars := suffix.toList
  if valueChars.drop (valueChars.length - suffixChars.length) == suffixChars then
    some (String.ofList (valueChars.take (valueChars.length - suffixChars.length)))
  else
    none

private def schemaPathLooksExplicit (text : String) : Bool :=
  text.contains ';' || Axiograph.Axi.SchemaV1.startsWith text.trim "id("

private def compileAxiSchemaPath (core : PresentationCore) (text equationName : String) :
    Except (Array ResidualObligation) (Option RawPath) := do
  let text := text.trim
  if let some rest := Axiograph.Axi.SchemaV1.stripPrefix? text "id(" then
    let some objectName := stripSuffix? rest ")"
      | throw #[residual s!"equation:{equationName}" .illTypedEquation
          "identity path is missing its closing `)`"]
    let some object := core.objectNames.findIdx? (· == objectName.trim)
      | throw #[residual s!"equation:{equationName}" .illTypedEquation
          s!"identity path references unknown object `{objectName.trim}`"]
    return some (.identity object)

  let labels := (text.splitToList (· == ';')).map String.trim |>.filter (!·.isEmpty)
  if labels.isEmpty then return none
  let mut indices : Array Nat := #[]
  for label in labels do
    let some index := core.arrows.findIdx? (fun arrow => arrow.name == label)
      | if labels.length == 1 && !schemaPathLooksExplicit text then
          return none
        else
          throw #[residual s!"equation:{equationName}" .illTypedEquation
            s!"schema path references unknown arrow `{label}`"]
    indices := indices.push index
  let some first := indices[0]?
    | return none
  let mut path : RawPath := .generator first
  for index in indices.toList.drop 1 do
    path := .trans path (.generator index)
  pure (some path)

private partial def validateCategoryTypeExpr
    (schemaName relationName roleName : String)
    (objects : Array String)
    (relations : Array Axiograph.Axi.SchemaV1.RelationDeclV1)
    (earlierRoles : Array Axiograph.Axi.SchemaV1.FieldDeclV1) :
    Axiograph.Axi.SchemaV1.TypeExprV1 → Except (Array ResidualObligation) Unit
  | .object target =>
      if objects.contains target then pure ()
      else throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
        s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` references unknown object `{target}`"]
  | .relationObject target =>
      if relations.any (fun relation => relation.name == target) then pure ()
      else throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
        s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` references unknown relation object `{target}`"]
  | .indexed base overRoles => do
      validateCategoryTypeExpr schemaName relationName roleName objects relations earlierRoles base
      let mut seenRoles : Array String := #[]
      for indexRole in overRoles do
        if seenRoles.contains indexRole then
          throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
            s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` repeats index role `{indexRole}`"]
        seenRoles := seenRoles.push indexRole
        if !earlierRoles.any (fun role => role.field == indexRole) then
          throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
            s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` indexes unknown or non-earlier role `{indexRole}`"]
      if let some targetRelationName := base.relationObjectName? then
        let some targetRelation := relations.find? (fun relation => relation.name == targetRelationName)
          | throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
              s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` references unknown relation object `{targetRelationName}`"]
        for indexRole in overRoles do
          let some localRole := earlierRoles.find? (fun role => role.field == indexRole)
            | throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
                s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` indexes unknown or non-earlier role `{indexRole}`"]
          let some targetRole := targetRelation.fields.find? (fun role => role.field == localRole.field)
            | throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
                s!"target relation `{targetRelation.name}` has no role named `{localRole.field}` for the indexed fiber"]
          if targetRole.ty.referencedName != localRole.ty.referencedName ||
              targetRole.kind != localRole.kind then
            throw #[residual s!"role:{relationName}:{roleName}" .invalidProjection
              s!"target relation `{targetRelation.name}.{targetRole.field}` does not match local index role `{localRole.field}` in carrier and role kind"]
  | .refined base predicates => do
      validateCategoryTypeExpr schemaName relationName roleName objects relations earlierRoles base
      for predicate in predicates do
        match predicate with
        | .memberOf values | .enum values =>
            if values.isEmpty then
              throw #[residual s!"role:{relationName}:{roleName}" .invalidRefinementValue
                "membership/enum set is empty"]
        | .cardinality min max =>
            if min > max then
              throw #[residual s!"role:{relationName}:{roleName}" .invalidRefinementValue
                "cardinality minimum exceeds maximum"]
        | .key _ =>
            throw #[residual s!"role:{relationName}:{roleName}" .unsupportedRefinement
              "key(...) role refinements have no implemented finite witness; declare a theory key constraint instead"]
        | .predicate name _ =>
            if name != "non_empty" then
              throw #[residual s!"role:{relationName}:{roleName}" .unsupportedRefinement
                s!"unsupported predicate `{name}`"]
        | .equals _ => pure ()

private def subtypeReachable
    (subtypes : Array Axiograph.Axi.SchemaV1.SubtypeDeclV1)
    (start target : String) : Bool := Id.run do
  let mut seen : Std.HashSet String := {}
  let mut stack : List String := [start]
  let mut found := false
  while !stack.isEmpty && !found do
    let current := stack.head!
    stack := stack.tail!
    if current == target then
      found := true
    else if !seen.contains current then
      seen := seen.insert current
      for subtype in subtypes do
        if subtype.sub == current then
          stack := subtype.sup :: stack
  found

private def validateAxiCategorySchema
    (schema : Axiograph.Axi.SchemaV1.SchemaV1Schema) :
    Except (Array ResidualObligation) Unit := do
  for relation in schema.relations do
    let mut earlierRoles : Array Axiograph.Axi.SchemaV1.FieldDeclV1 := #[]
    for field in relation.fields do
      validateCategoryTypeExpr schema.name relation.name field.field schema.objects schema.relations
        earlierRoles field.ty
      earlierRoles := earlierRoles.push field

  let mut subtypeEdges : Std.HashSet (String × String) := {}
  for subtype in schema.subtypes do
    if !schema.objects.contains subtype.sub then
      throw #[residual s!"subtype:{subtype.sub}:{subtype.sup}" .invalidProjection
        s!"schema `{schema.name}` subtype references unknown subtype `{subtype.sub}`"]
    if !schema.objects.contains subtype.sup then
      throw #[residual s!"subtype:{subtype.sub}:{subtype.sup}" .invalidProjection
        s!"schema `{schema.name}` subtype references unknown supertype `{subtype.sup}`"]
    if subtypeEdges.contains (subtype.sub, subtype.sup) then
      throw #[residual s!"subtype:{subtype.sub}:{subtype.sup}" .duplicateName
        s!"schema `{schema.name}` repeats subtype `{subtype.sub} <: {subtype.sup}`"]
    subtypeEdges := subtypeEdges.insert (subtype.sub, subtype.sup)
  for subtype in schema.subtypes do
    if subtype.sub == subtype.sup || subtypeReachable schema.subtypes subtype.sup subtype.sub then
      throw #[residual s!"subtype-cycle:{subtype.sub}:{subtype.sup}" .invalidProjection
        s!"schema `{schema.name}` has a subtype cycle involving `{subtype.sub}` and `{subtype.sup}`"]

/-- Derive the supported finite category presentation from one canonical `.axi`
schema and the equations in its module. This is a projection of accepted exact
syntax, not a second authoring authority. Only explicit generator paths (`id(X)`
or semicolon composition) become category equations; relation-span/groupoid and
opaque theory equations remain outside this finite certificate claim. -/
def compileAxiSchemaPresentation
    (module : Axiograph.Axi.SchemaV1.SchemaV1Module)
    (schema : Axiograph.Axi.SchemaV1.SchemaV1Schema) :
    Except (Array ResidualObligation) Presentation := do
  validateAxiCategorySchema schema
  let objectNames := schema.objects ++ schema.relations.map (·.name)
  let mut arrows : Array (ArrowDecl objectNames.size) := #[]

  for relation in schema.relations do
    let relationObject ← namedIndex objectNames relation.name s!"relation:{relation.name}"
    for field in relation.fields do
      let targetName := field.ty.referencedName
      let target ← namedIndex objectNames targetName s!"role:{relation.name}:{field.field}"
      arrows := arrows.push {
        name := s!"{relation.name}.{field.field}"
        source := relationObject
        target
        kind := .projection
      }

  for subtype in schema.subtypes do
    let source ← namedIndex objectNames subtype.sub s!"subtype:{subtype.sub}:{subtype.sup}"
    let target ← namedIndex objectNames subtype.sup s!"subtype:{subtype.sub}:{subtype.sup}"
    arrows := arrows.push {
      name := subtype.inclusion.getD s!"{subtype.sub}_to_{subtype.sup}"
      source
      target
      kind := .subtypeInclusion
    }

  for generator in schema.generators do
    let source ← namedIndex objectNames generator.source s!"generator:{generator.name}"
    let target ← namedIndex objectNames generator.target s!"generator:{generator.name}"
    arrows := arrows.push {
      name := generator.name
      source
      target
      kind := match generator.kind with
        | .aspect => .aspect
        | .function => .function
      reversible := generator.reversible
    }

  let core : PresentationCore := { objectNames, arrows }
  let mut relations : Array (RelationObjectDecl core) := #[]
  for relation in schema.relations do
    let object ← namedIndex core.objectNames relation.name s!"relation:{relation.name}"
    let mut roles : Array (RoleDecl core) := #[]
    let mut declaredOrder := 0
    for field in relation.fields do
      let targetName := field.ty.referencedName
      let target ← namedIndex core.objectNames targetName s!"role:{relation.name}:{field.field}"
      let projectionName := s!"{relation.name}.{field.field}"
      let projection ← arrowIndex core projectionName s!"role:{relation.name}:{field.field}"
      roles := roles.push {
        name := field.field
        target
        projection
        declaredOrder
        kind := roleKindFromAxi field.kind
      }
      declaredOrder := declaredOrder + 1
    relations := relations.push { name := relation.name, object, roles }

  let base : Presentation := { core, relations, equations := #[] }
  validatePresentation base
  let mut equations : Array RawEquation := #[]
  for theory in module.theories do
    if theory.schema == schema.name then
      for equation in theory.equations do
        let lhs ← compileAxiSchemaPath core equation.lhs equation.name
        let rhs ← compileAxiSchemaPath core equation.rhs equation.name
        match lhs, rhs with
        | some lhs, some rhs =>
            equations := equations.push { name := equation.name, lhs, rhs }
        | none, none =>
            if schemaPathLooksExplicit equation.lhs || schemaPathLooksExplicit equation.rhs then
              throw #[residual s!"equation:{equation.name}" .illTypedEquation
                "explicit schema equation did not resolve to category arrows"]
        | _, _ =>
            throw #[residual s!"equation:{equation.name}" .illTypedEquation
              "only one side of the equation resolved to a category path"]

  let presentation : Presentation := { core, relations, equations }
  validatePresentation presentation
  pure presentation

-- =============================================================================
-- Finite interpretations, dependent role witnesses, and equations
-- =============================================================================

structure FiniteInterpretation (p : PresentationCore) where
  carrierSize : Fin p.objectNames.size → Nat
  arrowMap : (arrow : Fin p.arrows.size) →
    Fin (carrierSize (p.arrows[arrow].source)) →
    Fin (carrierSize (p.arrows[arrow].target))

def evalPath {p : PresentationCore} (model : FiniteInterpretation p) :
    {a b : Fin p.objectNames.size} → Path p a b →
      Fin (model.carrierSize a) → Fin (model.carrierSize b)
  | _, _, .identity _, value => value
  | _, _, .generator arrow, value => model.arrowMap arrow value
  | _, _, .trans left right, value => evalPath model right (evalPath model left value)

theorem evalPath_identity {p : PresentationCore} (model : FiniteInterpretation p)
    (object : Fin p.objectNames.size) (value : Fin (model.carrierSize object)) :
    evalPath model (.identity object) value = value := rfl

theorem evalPath_trans {p : PresentationCore} (model : FiniteInterpretation p)
    {a b c : Fin p.objectNames.size} (left : Path p a b) (right : Path p b c)
    (value : Fin (model.carrierSize a)) :
    evalPath model (.trans left right) value =
      evalPath model right (evalPath model left value) := rfl

def EquationSatisfied {p : PresentationCore} (model : FiniteInterpretation p)
    (equation : CheckedEquation p) : Prop :=
  ∀ value, evalPath model equation.lhs value = evalPath model equation.rhs value

def compileEquations (presentation : Presentation) :
    Except ResidualObligation (Array (CheckedEquation presentation.core)) := do
  let mut equations : Array (CheckedEquation presentation.core) := #[]
  for equation in presentation.equations do
    equations := equations.push (← checkEquation presentation.core equation)
  pure equations

private def rawPathArrows : RawPath → Array Nat
  | .identity _ => #[]
  | .generator arrow => #[arrow]
  | .trans left right => rawPathArrows left ++ rawPathArrows right

private def categoryKernelPathV3 (core : PresentationCore) (path : RawPath) :
    Except ResidualObligation CategoryKernelPathV3 := do
  let checked ← inferPath core path
  pure {
    source := checked.source.val
    target := checked.target.val
    arrows := rawPathArrows path
  }

/-- Canonical name/index projection used to compare Lean formation with the
compiler-emitted Rust presentation under the same exact-byte anchor. -/
def categoryKernelPresentationV3 (presentation : Presentation) :
    Except (Array ResidualObligation) CategoryKernelPresentationV3 := do
  validatePresentation presentation
  let arrows := presentation.core.arrows.map (fun arrow => {
    name := arrow.name
    source := arrow.source.val
    target := arrow.target.val
    kind := arrow.kind
    reversible := arrow.reversible
  })
  let relations := presentation.relations.map (fun relation => {
    name := relation.name
    object := relation.object.val
    roles := relation.roles.map (fun role => {
      name := role.name
      target := role.target.val
      projection := role.projection.val
      declaredOrder := role.declaredOrder
      kind := role.kind
    })
  })
  let mut equations : Array CategoryKernelEquationV3 := #[]
  for equation in presentation.equations do
    let lhs ←
      match categoryKernelPathV3 presentation.core equation.lhs with
      | .ok path => pure path
      | .error err => throw #[err]
    let rhs ←
      match categoryKernelPathV3 presentation.core equation.rhs with
      | .ok path => pure path
      | .error err => throw #[err]
    equations := equations.push { name := equation.name, lhs, rhs }
  pure {
    objectNames := presentation.core.objectNames
    arrows
    relations
    identityObjects := Array.range presentation.core.objectNames.size
    equations
  }

private def checkCategoryKernelPathV3 (presentation : CategoryKernelPresentationV3)
    (path : CategoryKernelPathV3) : Except ResidualObligation Unit := do
  if path.source >= presentation.objectNames.size || path.target >= presentation.objectNames.size then
    throw <| residualAt "category-kernel:path:endpoint" .malformedExplanation
      "category path endpoint is outside the finite presentation"
      (some path.source) (some path.target)
  let mut cursor := path.source
  for arrowIndex in path.arrows do
    let some arrow := presentation.arrows[arrowIndex]?
      | throw <| residual "category-kernel:path:arrow" .malformedExplanation
          s!"category path references unknown arrow index {arrowIndex}"
    if arrow.source != cursor then
      throw <| residualAt "category-kernel:path:composition" .malformedExplanation
        s!"category path does not compose at arrow index {arrowIndex}"
        (some cursor) (some arrow.source)
    cursor := arrow.target
  if cursor != path.target then
    throw <| residualAt "category-kernel:path:target" .malformedExplanation
      "category path does not end at its declared target"
      (some cursor) (some path.target)

private def checkCategoryKernelFormalPathV3
    (presentation : CategoryKernelPresentationV3)
    (path : CategoryKernelFormalPathV3) : Except ResidualObligation Unit := do
  if path.source >= presentation.objectNames.size || path.target >= presentation.objectNames.size then
    throw <| residualAt "category-kernel:groupoid:endpoint" .malformedExplanation
      "formal groupoid path endpoint is outside the finite presentation"
      (some path.source) (some path.target)
  let mut cursor := path.source
  for step in path.steps do
    let some arrow := presentation.arrows[step.arrow]?
      | throw <| residual "category-kernel:groupoid:arrow" .malformedExplanation
          s!"formal groupoid path references unknown arrow index {step.arrow}"
    let (source, target) := match step.direction with
      | .forward => (arrow.source, arrow.target)
      | .inverse => (arrow.target, arrow.source)
    if source != cursor then
      throw <| residualAt "category-kernel:groupoid:composition" .malformedExplanation
        s!"formal groupoid path does not compose at arrow index {step.arrow}"
        (some cursor) (some source)
    cursor := target
  if cursor != path.target then
    throw <| residualAt "category-kernel:groupoid:target" .malformedExplanation
      "formal groupoid path does not end at its declared target"
      (some cursor) (some path.target)

private def applyCategoryKernelFormalRewriteStepV3
    (presentation : CategoryKernelPresentationV3)
    (path : CategoryKernelFormalPathV3)
    (step : CategoryKernelFormalRewriteStepV3) :
    Except ResidualObligation CategoryKernelFormalPathV3 := do
  checkCategoryKernelFormalPathV3 presentation path
  let steps := path.steps.toList
  let some first := steps[step.offset]?
    | throw <| residual "category-kernel:groupoid:rewrite-offset" .malformedExplanation
        s!"formal rewrite offset {step.offset} is out of range"
  let some second := steps[step.offset + 1]?
    | throw <| residual "category-kernel:groupoid:rewrite-adjacent" .malformedExplanation
        s!"formal rewrite offset {step.offset} has no adjacent step"
  if first.arrow != step.arrow || first.direction != step.firstDirection ||
      second.arrow != step.arrow || second.direction != step.firstDirection.opposite then
    throw <| residual "category-kernel:groupoid:rewrite-pair" .malformedExplanation
      "formal rewrite step does not cite an adjacent inverse pair"
  let rewritten := (steps.take step.offset ++ steps.drop (step.offset + 2)).toArray
  let result : CategoryKernelFormalPathV3 := {
    source := path.source
    target := path.target
    steps := rewritten
  }
  checkCategoryKernelFormalPathV3 presentation result
  pure result

private def replayCategoryKernelFormalNormalizationV3
    (presentation : CategoryKernelPresentationV3)
    (certificate : CategoryKernelFormalNormalizationV3) :
    Except ResidualObligation Unit := do
  checkCategoryKernelFormalPathV3 presentation certificate.input
  checkCategoryKernelFormalPathV3 presentation certificate.normalized
  let mut current := certificate.input
  for step in certificate.rewriteTrace do
    current ← applyCategoryKernelFormalRewriteStepV3 presentation current step
  if current != certificate.normalized then
    throw <| residual "category-kernel:groupoid:normalization-output" .malformedExplanation
      "formal rewrite trace does not produce the declared normal form"

private def canonicalCategoryKernelFormalNormalizationV3
    (presentation : CategoryKernelPresentationV3)
    (arrowIndex : Nat)
    (firstDirection : CategoryKernelFormalDirectionV3) :
    Except ResidualObligation CategoryKernelFormalNormalizationV3 := do
  let some arrow := presentation.arrows[arrowIndex]?
    | throw <| residual "category-kernel:groupoid:canonical-arrow" .malformedExplanation
        s!"canonical inverse-law witness references unknown arrow index {arrowIndex}"
  let source := match firstDirection with
    | .forward => arrow.source
    | .inverse => arrow.target
  let input : CategoryKernelFormalPathV3 := {
    source
    target := source
    steps := #[
      { arrow := arrowIndex, direction := firstDirection },
      { arrow := arrowIndex, direction := firstDirection.opposite }
    ]
  }
  let normalized : CategoryKernelFormalPathV3 := {
    source
    target := source
    steps := #[]
  }
  let certificate : CategoryKernelFormalNormalizationV3 := {
    input
    rewriteTrace := #[{ offset := 0, arrow := arrowIndex, firstDirection }]
    normalized
  }
  replayCategoryKernelFormalNormalizationV3 presentation certificate
  pure certificate

/-- Check exact Rust/Lean parity for the two formal inverse cancellation
words of every presented generator. This is syntactic wire replay: these values
are not retyped as `GroupoidPath`, and no acceptance theorem connects this
function to `GroupoidPath.denote`. It makes no executable inverse claim. -/
def verifyCategoryKernelGroupoidNormalizationsV3
    (presentation : CategoryKernelPresentationV3)
    (certificates : Array CategoryKernelFormalNormalizationV3) :
    Except ResidualObligation Unit := do
  let mut expected : Array CategoryKernelFormalNormalizationV3 := #[]
  for arrowIndex in List.range presentation.arrows.size do
    expected := expected.push
      (← canonicalCategoryKernelFormalNormalizationV3 presentation arrowIndex .forward)
    expected := expected.push
      (← canonicalCategoryKernelFormalNormalizationV3 presentation arrowIndex .inverse)
  for certificate in certificates do
    replayCategoryKernelFormalNormalizationV3 presentation certificate
  if certificates != expected then
    throw <| residual "category-kernel:groupoid:coverage" .incompleteCertificate
      "formal groupoid normalization witnesses do not exactly cover both inverse laws for every arrow"

private def categoryKernelObjectAtOffsetV3
    (presentation : CategoryKernelPresentationV3)
    (path : CategoryKernelPathV3)
    (offset : Nat) : Except ResidualObligation Nat := do
  if offset > path.arrows.size then
    throw <| residual "category-kernel:congruence:offset" .malformedExplanation
      s!"congruence offset {offset} is outside the input path"
  let mut cursor := path.source
  for arrowIndex in path.arrows.toList.take offset do
    let some arrow := presentation.arrows[arrowIndex]?
      | throw <| residual "category-kernel:congruence:arrow" .malformedExplanation
          s!"congruence context references unknown arrow index {arrowIndex}"
    if arrow.source != cursor then
      throw <| residual "category-kernel:congruence:composition" .malformedExplanation
        "congruence context is not composable"
    cursor := arrow.target
  pure cursor

private def applyCategoryKernelCongruenceStepV3
    (presentation : CategoryKernelPresentationV3)
    (path : CategoryKernelPathV3)
    (step : CategoryKernelCongruenceStepV3) :
    Except ResidualObligation CategoryKernelPathV3 := do
  checkCategoryKernelPathV3 presentation path
  let some equation := presentation.equations[step.equation]?
    | throw <| residual "category-kernel:congruence:equation" .malformedExplanation
        s!"congruence step references unknown equation index {step.equation}"
  let (fromPath, toPath) := match step.direction with
    | .forward => (equation.lhs, equation.rhs)
    | .reverse => (equation.rhs, equation.lhs)
  checkCategoryKernelPathV3 presentation fromPath
  checkCategoryKernelPathV3 presentation toPath
  let pathArrows := path.arrows.toList
  let fromArrows := fromPath.arrows.toList
  let offset := step.offset
  let endOffset := offset + fromArrows.length
  let segmentSource ← categoryKernelObjectAtOffsetV3 presentation path offset
  let segmentTarget ← categoryKernelObjectAtOffsetV3 presentation path endOffset
  if endOffset > pathArrows.length ||
      (pathArrows.drop offset).take fromArrows.length != fromArrows ||
      segmentSource != fromPath.source || segmentTarget != fromPath.target then
    throw <| residual "category-kernel:congruence:segment" .malformedExplanation
      "congruence step does not match the selected path segment"
  let rewritten :=
    (pathArrows.take offset ++ toPath.arrows.toList ++ pathArrows.drop endOffset).toArray
  let result : CategoryKernelPathV3 := {
    source := path.source
    target := path.target
    arrows := rewritten
  }
  checkCategoryKernelPathV3 presentation result
  pure result

/-- Check contextual equation replacement with the canonical wire coverage:
certificate `i` contains exactly one forward application of equation `i`. -/
def verifyCategoryKernelCongruenceV3
    (presentation : CategoryKernelPresentationV3)
    (certificates : Array CategoryKernelCongruenceCertificateV3) :
    Except ResidualObligation Unit := do
  if certificates.size != presentation.equations.size then
    throw <| residual "category-kernel:congruence:coverage" .incompleteCertificate
      "congruence certificate count does not equal the presented equation count"
  for equationIndex in List.range presentation.equations.size do
    let some certificate := certificates[equationIndex]?
      | throw <| residual "category-kernel:congruence:coverage" .incompleteCertificate
          s!"presented equation {equationIndex} has no congruence replay witness"
    let some step := certificate.steps[0]?
      | throw <| residual "category-kernel:congruence:empty" .incompleteCertificate
          "congruence certificate must contain one equation step"
    if certificate.steps.size != 1 || step.equation != equationIndex ||
        step.direction != .forward then
      throw <| residual "category-kernel:congruence:canonical" .incompleteCertificate
        s!"congruence certificate {equationIndex} is not the one-step forward witness for equation {equationIndex}"
    checkCategoryKernelPathV3 presentation certificate.input
    checkCategoryKernelPathV3 presentation certificate.output
    let output ← applyCategoryKernelCongruenceStepV3 presentation certificate.input step
    if output != certificate.output then
      throw <| residual "category-kernel:congruence:output" .malformedExplanation
        "congruence replay output does not match the declared output path"

/-- A finite interpretation becomes a model of the presented category only when
it satisfies every compiled path equation. -/
structure FiniteCategoryModel (p : PresentationCore)
    (equations : Array (CheckedEquation p)) extends FiniteInterpretation p where
  equationHolds : (index : Fin equations.size) →
    EquationSatisfied toFiniteInterpretation equations[index]

structure RoleWitness {p : PresentationCore} (model : FiniteInterpretation p)
    (role : TypedRole p) where
  tuple : Fin (model.carrierSize role.relationObject)
  value : Fin (model.carrierSize role.target)
  projectionHolds : evalPath model role.projection tuple = value

-- =============================================================================
-- Finite refinements
-- =============================================================================

inductive RawRefinement where
  | equals (value : Nat)
  | memberOf (values : Array Nat)
  | unsupported (name : String)
  deriving Repr, DecidableEq

inductive FiniteRefinement (size : Nat) where
  | equals (value : Fin size)
  | memberOf (values : Array (Fin size))
  deriving Repr

def compileRefinement (size : Nat) (id : String) : RawRefinement →
    Except ResidualObligation (FiniteRefinement size)
  | .equals value =>
      if h : value < size then pure (.equals ⟨value, h⟩)
      else throw (residual id .invalidRefinementValue
        s!"refinement value {value} is outside carrier size {size}")
  | .memberOf values => do
      if values.isEmpty then
        throw (residual id .invalidRefinementValue "finite membership refinement is empty")
      let mut compiled : Array (Fin size) := #[]
      for value in values do
        if h : value < size then compiled := compiled.push ⟨value, h⟩
        else throw (residual id .invalidRefinementValue
          s!"refinement value {value} is outside carrier size {size}")
      pure (.memberOf compiled)
  | .unsupported name =>
      throw (residual id .unsupportedRefinement
        s!"refinement predicate `{name}` is outside the finite decidable fragment")

def FiniteRefinement.Holds {size : Nat} (value : Fin size) :
    FiniteRefinement size → Prop
  | .equals expected => value = expected
  | .memberOf values => value ∈ values

instance {size : Nat} (value : Fin size) (refinement : FiniteRefinement size) :
    Decidable (refinement.Holds value) := by
  cases refinement with
  | equals expected =>
      change Decidable (value = expected)
      exact inferInstance
  | memberOf values =>
      change Decidable (value ∈ values)
      exact inferInstance

structure RefinedValue (size : Nat) (refinements : Array (FiniteRefinement size)) where
  value : Fin size
  satisfies : ∀ refinement, refinement ∈ refinements → refinement.Holds value

-- =============================================================================
-- Context-indexed values and transports
-- =============================================================================

structure ContextFamily {p : PresentationCore} (model : FiniteInterpretation p) where
  contextNames : Array String
  visible : (context : Fin contextNames.size) →
    (object : Fin p.objectNames.size) → Fin (model.carrierSize object) → Bool

structure ContextValue {p : PresentationCore} {model : FiniteInterpretation p}
    (family : ContextFamily model) (context : Fin family.contextNames.size)
    (object : Fin p.objectNames.size) where
  value : Fin (model.carrierSize object)
  visible : family.visible context object value = true

structure ContextTransport {p : PresentationCore} {model : FiniteInterpretation p}
    (family : ContextFamily model) (source target : Fin family.contextNames.size) where
  preserves : ∀ (object : Fin p.objectNames.size)
    (value : Fin (model.carrierSize object)),
    family.visible source object value = true →
    family.visible target object value = true

def ContextTransport.identity {p : PresentationCore} {model : FiniteInterpretation p}
    (family : ContextFamily model) (context : Fin family.contextNames.size) :
    ContextTransport family context context where
  preserves := fun _ _ visible => visible

def ContextTransport.trans {p : PresentationCore} {model : FiniteInterpretation p}
    {family : ContextFamily model} {first second third : Fin family.contextNames.size}
    (left : ContextTransport family first second)
    (right : ContextTransport family second third) :
    ContextTransport family first third where
  preserves := fun object value visible => right.preserves object value
    (left.preserves object value visible)

def transportContextValue {p : PresentationCore} {model : FiniteInterpretation p}
    {family : ContextFamily model} {source target : Fin family.contextNames.size}
    {object : Fin p.objectNames.size} (transport : ContextTransport family source target)
    (value : ContextValue family source object) : ContextValue family target object :=
  { value := value.value
    visible := transport.preserves object value.value value.visible }

structure ContextualInterpretation (p : PresentationCore) extends FiniteInterpretation p where
  contextNames : Array String
  visible : (context : Fin contextNames.size) →
    (object : Fin p.objectNames.size) → Fin (carrierSize object) → Bool
  arrowPreserves : ∀ (context : Fin contextNames.size) (arrow : Fin p.arrows.size)
    (value : Fin (carrierSize (p.arrows[arrow].source))),
    visible context (p.arrows[arrow].source) value = true →
    visible context (p.arrows[arrow].target) (arrowMap arrow value) = true

theorem evalPath_preserves_context {p : PresentationCore}
    (model : ContextualInterpretation p) (context : Fin model.contextNames.size)
    {a b : Fin p.objectNames.size} (path : Path p a b)
    (value : Fin (model.carrierSize a))
    (visible : model.visible context a value = true) :
    model.visible context b (evalPath model.toFiniteInterpretation path value) = true := by
  induction path with
  | identity _ => exact visible
  | generator arrow => exact model.arrowPreserves context arrow value visible
  | trans left right ihLeft ihRight =>
      exact ihRight _ (ihLeft _ visible)

-- =============================================================================
-- Checked lifecycle states and typed holes
-- =============================================================================

inductive CheckedLifecycleState where
  | formationChecked
  | explanationVerified
  | residual
  | rejected
  deriving Repr, DecidableEq

structure PathCandidate (p : PresentationCore)
    (source target : Fin p.objectNames.size) where
  path : Path p source target
  explanation : String

structure TypedPathHole (p : PresentationCore)
    (source target : Fin p.objectNames.size) where
  holeId : String
  candidates : Array (PathCandidate p source target)
  lifecycle : CheckedLifecycleState := .residual
  residualObligations : Array ResidualObligation

structure CheckedPathSelection (p : PresentationCore)
    (source target : Fin p.objectNames.size) where
  holeId : String
  selected : PathCandidate p source target
  lifecycle : CheckedLifecycleState := .explanationVerified

def TypedPathHole.select? {p : PresentationCore}
    {source target : Fin p.objectNames.size}
    (hole : TypedPathHole p source target) (candidate : Nat) :
    Option (CheckedPathSelection p source target) :=
  if hole.lifecycle != .residual || hole.residualObligations.isEmpty ||
      !hole.residualObligations.all (fun obligation =>
        obligation.kind == .typedHole && obligation.obligationId == hole.holeId &&
        obligation.source == some source.val && obligation.target == some target.val) then
    none
  else do
    let selected ← hole.candidates[candidate]?
    pure { holeId := hole.holeId, selected }

-- =============================================================================
-- Finite reachability saturation with replayable explanations
-- =============================================================================

inductive Explanation where
  | identity (object : Nat)
  | generator (arrow : Nat)
  | trans (left right : Explanation)
  deriving Repr, DecidableEq

structure ReachabilityEntry where
  source : Nat
  target : Nat
  explanation : Explanation
  deriving Repr, DecidableEq

structure SaturationCertificate where
  presentationObjectCount : Nat
  presentationArrowCount : Nat
  entries : Array ReachabilityEntry
  algorithm : String := "finite_floyd_warshall_v2"
  deriving Repr, DecidableEq

structure CheckedSaturation (p : PresentationCore) where
  certificate : SaturationCertificate
  witnesses : Array (SomePath p)
  lifecycle : CheckedLifecycleState := .explanationVerified

structure SaturationReport where
  certificate : Option SaturationCertificate
  complete : Bool
  residualObligations : Array ResidualObligation
  deriving Repr, DecidableEq

def checkExplanation (p : PresentationCore) (explanation : Explanation) :
    Except ResidualObligation (SomePath p) :=
  inferPath p <| match explanation with
    | .identity object => .identity object
    | .generator arrow => .generator arrow
    | .trans left right =>
        let rec toRaw : Explanation → RawPath
          | .identity object => .identity object
          | .generator arrow => .generator arrow
          | .trans a b => .trans (toRaw a) (toRaw b)
        .trans (toRaw left) (toRaw right)

private def closureIndex (source target : Nat) : Nat × Nat := (source, target)

def saturateFinite (presentation : Presentation) (maxObjects : Nat) : SaturationReport := Id.run do
  match validatePresentation presentation with
  | .error errors =>
      return { certificate := none, complete := false, residualObligations := errors }
  | .ok _ => pure ()

  let objectCount := presentation.core.objectNames.size
  if objectCount > maxObjects then
    return {
      certificate := none
      complete := false
      residualObligations := #[residual "saturation:object_bound" .saturationBound
        s!"finite saturation object count {objectCount} exceeds configured bound {maxObjects}"]
    }

  let mut closure : Std.HashMap (Nat × Nat) Explanation := {}
  for object in List.range objectCount do
    closure := closure.insert (closureIndex object object) (.identity object)
  for arrow in List.range presentation.core.arrows.size do
    match presentation.core.arrows[arrow]? with
    | some decl =>
        let key := closureIndex decl.source.val decl.target.val
        if !closure.contains key then
          closure := closure.insert key (.generator arrow)
    | none => pure ()

  for middle in List.range objectCount do
    for source in List.range objectCount do
      for target in List.range objectCount do
        let key := closureIndex source target
        if !closure.contains key then
          match closure.get? (closureIndex source middle),
              closure.get? (closureIndex middle target) with
          | some left, some right => closure := closure.insert key (.trans left right)
          | _, _ => pure ()

  let mut entries : Array ReachabilityEntry := #[]
  for source in List.range objectCount do
    for target in List.range objectCount do
      match closure.get? (closureIndex source target) with
      | some explanation => entries := entries.push { source, target, explanation }
      | none => pure ()
  return {
    certificate := some {
      presentationObjectCount := objectCount
      presentationArrowCount := presentation.core.arrows.size
      entries
    }
    complete := true
    residualObligations := #[]
  }

def verifySaturationCertificate (presentation : Presentation)
    (certificate : SaturationCertificate) :
    Except ResidualObligation (Array (SomePath presentation.core)) := do
  match validatePresentation presentation with
  | .ok _ => pure ()
  | .error errors =>
      throw <| errors[0]?.getD (residual "presentation" .incompleteCertificate
        "presentation validation failed")
  if certificate.algorithm != "finite_floyd_warshall_v2" then
    throw <| residual "saturation:algorithm" .incompleteCertificate
      s!"unknown saturation algorithm `{certificate.algorithm}`"
  if presentation.core.objectNames.size > 64 || presentation.core.arrows.size > 4096 then
    throw <| residual "saturation:checker_bound" .saturationBound
      "presentation exceeds the trusted finite saturation checker bounds"
  if certificate.entries.size > presentation.core.objectNames.size *
      presentation.core.objectNames.size then
    throw <| residual "saturation:entry_bound" .incompleteCertificate
      "certificate has more endpoint entries than the finite presentation permits"
  if certificate.presentationObjectCount != presentation.core.objectNames.size ||
      certificate.presentationArrowCount != presentation.core.arrows.size then
    throw <| residual "saturation:presentation_shape" .incompleteCertificate
      "certificate presentation shape does not match the checked presentation"

  let mut entries : Std.HashMap (Nat × Nat) Explanation := {}
  let mut witnesses : Array (SomePath presentation.core) := #[]
  for entry in certificate.entries do
    if entries.contains (entry.source, entry.target) then
      throw <| residual s!"saturation:duplicate:{entry.source}:{entry.target}"
        .incompleteCertificate "duplicate reachability entry"
    let checked ← checkExplanation presentation.core entry.explanation
    if checked.source.val != entry.source || checked.target.val != entry.target then
      throw <| residualAt s!"saturation:explanation:{entry.source}:{entry.target}"
        .malformedExplanation "explanation endpoints do not match the claimed entry"
        (some entry.source) (some entry.target)
    entries := entries.insert (entry.source, entry.target) entry.explanation
    witnesses := witnesses.push checked

  for object in List.range presentation.core.objectNames.size do
    if !entries.contains (object, object) then
      throw <| residual s!"saturation:identity:{object}" .incompleteCertificate
        "certificate omits a required identity reachability entry"
  for arrowIndex in List.range presentation.core.arrows.size do
    match presentation.core.arrows[arrowIndex]? with
    | some arrow =>
        if !entries.contains (arrow.source.val, arrow.target.val) then
          throw <| residual s!"saturation:generator:{arrowIndex}" .incompleteCertificate
            "certificate omits a required generator reachability entry"
    | none => pure ()
  for source in List.range presentation.core.objectNames.size do
    for middle in List.range presentation.core.objectNames.size do
      for target in List.range presentation.core.objectNames.size do
        if entries.contains (source, middle) && entries.contains (middle, target) &&
            !entries.contains (source, target) then
          throw <| residual s!"saturation:trans:{source}:{middle}:{target}"
            .incompleteCertificate "certificate is not closed under path composition"
  pure witnesses

def checkSaturationCertificate (presentation : Presentation)
    (certificate : SaturationCertificate) :
    Except ResidualObligation (CheckedSaturation presentation.core) := do
  let witnesses ← verifySaturationCertificate presentation certificate
  pure { certificate, witnesses }

def reachabilityEntry? (certificate : SaturationCertificate) (source target : Nat) :
    Option ReachabilityEntry :=
  certificate.entries.find? (fun entry => entry.source == source && entry.target == target)

/-!
## Scope

Successful `verifySaturationCertificate` means exactly:

1. every entry is explained by identities, declared generators, and typed
   composition;
2. every identity and generator is present; and
3. the finite entry set is closed under composition.

It does not use presented equations as rewrite rules, prove confluence, infer
new ontology facts, close an open world, or establish arbitrary category/HoTT
semantics. Those remain explicit residual or non-claims at higher layers.
-/

end Axiograph.Theory.Finite
