import Std
import Axiograph.Axi.AxiV1

/-!
# `.axi` well-typedness checking (AST-level)

This module implements a **small decision procedure** that checks whether a
canonical `.axi` module is *well-formed and well-typed* with respect to its
declared schema.

It is intentionally conservative and mirrors the Rust-side checker used for
`axi_well_typed_v1` certificates.

## What is checked?

For each module, this checker validates schema declarations, dependent role and
refinement types, supported theory constraint references, typed rewrite
references and endpoints, and instance assignments. Relation tuples must have
exactly the declared fields. Relation tuples may introduce objects implicitly,
but subtyping-based reuse is checked for ambiguity.

This check is designed to keep the trusted kernel small:
Lean can re-run it directly on the parsed `.axi` AST.
-/

namespace Axiograph.Axi.TypeCheck

open Axiograph.Axi.SchemaV1

structure TypeCheckSummaryV1 where
  moduleName : Name
  schemaCount : Nat
  theoryCount : Nat
  instanceCount : Nat
  assignmentCount : Nat
  tupleCount : Nat
  deriving Repr, DecidableEq

def TypeCheckSummaryV1.ofModule (m : Axiograph.Axi.AxiV1.AxiV1Module) : TypeCheckSummaryV1 :=
  let assignmentCount :=
    m.instances.foldl (fun acc inst => acc + inst.assignments.size) 0
  let tupleCount :=
    m.instances.foldl
      (fun acc inst =>
        inst.assignments.foldl
          (fun acc2 a =>
            acc2 + a.value.items.foldl
              (fun acc3 it => acc3 + match it with
                | .tuple _ _ => 1
                | .ident _ => 0)
              0)
          acc)
      0
  {
    moduleName := m.moduleName
    schemaCount := m.schemas.size
    theoryCount := m.theories.size
    instanceCount := m.instances.size
    assignmentCount := assignmentCount
    tupleCount := tupleCount
  }

structure SchemaIndex where
  objectTypes : Std.HashSet Name
  relationDecls : Std.HashMap Name RelationDeclV1
  generatorDecls : Std.HashMap Name GeneratorDeclV1
  supertypesOf : Std.HashMap Name (Std.HashSet Name)
  subtypesOf : Std.HashMap Name (Std.HashSet Name)
  deriving Repr

def SchemaIndex.isSubtype (idx : SchemaIndex) (sub sup : Name) : Bool :=
  match idx.supertypesOf.get? sub with
  | none => sub == sup
  | some supers => supers.contains sup

def SchemaIndex.isValueType (idx : SchemaIndex) (ty : Name) : Bool :=
  idx.objectTypes.contains ty || idx.relationDecls.contains ty

def SchemaIndex.relationDecl (idx : SchemaIndex) (relation : Name) : Except String RelationDeclV1 :=
  match idx.relationDecls.get? relation with
  | some declaration => pure declaration
  | none => throw s!"unknown relation `{relation}` in schema"

def SchemaIndex.fieldDecl
    (idx : SchemaIndex) (relation field : Name) : Except String FieldDeclV1 := do
  let declaration ← idx.relationDecl relation
  match declaration.fields.find? (fun candidate => candidate.field == field) with
  | some found => pure found
  | none => throw s!"relation `{relation}` has no field `{field}`"

def SchemaIndex.relatedTypesIncludingSelf (idx : SchemaIndex) (ty : Name) : List Name := Id.run do
  let mut related : Std.HashSet Name := {}
  for t in (idx.supertypesOf.getD ty {}).toList do
    related := related.insert t
  for t in (idx.subtypesOf.getD ty {}).toList do
    related := related.insert t
  related.toList

def computeSupertypesClosure
    (objectTypes : Std.HashSet Name)
    (subtypeDecls : Array SubtypeDeclV1) :
    Std.HashMap Name (Std.HashSet Name) := Id.run do

  let mut directSupers : Std.HashMap Name (List Name) := {}
  for st in subtypeDecls do
    let prev := directSupers.getD st.sub []
    directSupers := directSupers.insert st.sub (st.sup :: prev)

  let mut out : Std.HashMap Name (Std.HashSet Name) := {}
  for ty in objectTypes.toList do
    let mut supers : Std.HashSet Name := {}
    supers := supers.insert ty
    let mut stack : List Name := directSupers.getD ty []
    while !stack.isEmpty do
      let sup := stack.head!
      stack := stack.tail!
      if !supers.contains sup then
        supers := supers.insert sup
        stack := stack ++ directSupers.getD sup []
    out := out.insert ty supers

  out

def computeSubtypesClosure
    (objectTypes : Std.HashSet Name)
    (subtypeDecls : Array SubtypeDeclV1) :
    Std.HashMap Name (Std.HashSet Name) := Id.run do

  let mut directSubs : Std.HashMap Name (List Name) := {}
  for st in subtypeDecls do
    let prev := directSubs.getD st.sup []
    directSubs := directSubs.insert st.sup (st.sub :: prev)

  let mut out : Std.HashMap Name (Std.HashSet Name) := {}
  for ty in objectTypes.toList do
    let mut subs : Std.HashSet Name := {}
    subs := subs.insert ty
    let mut stack : List Name := directSubs.getD ty []
    while !stack.isEmpty do
      let sub := stack.head!
      stack := stack.tail!
      if !subs.contains sub then
        subs := subs.insert sub
        stack := stack ++ directSubs.getD sub []
    out := out.insert ty subs

  out

def SchemaIndex.ofSchema (s : SchemaV1Schema) : SchemaIndex :=
  let objectTypes : Std.HashSet Name :=
    s.objects.foldl (fun acc o => acc.insert o) {}
  let relationDecls : Std.HashMap Name RelationDeclV1 :=
    s.relations.foldl (fun acc r => acc.insert r.name r) {}
  let generatorDecls : Std.HashMap Name GeneratorDeclV1 :=
    s.generators.foldl (fun acc g => acc.insert g.name g) {}
  let supertypesOf := computeSupertypesClosure objectTypes s.subtypes
  let subtypesOf := computeSubtypesClosure objectTypes s.subtypes
  { objectTypes, relationDecls, generatorDecls, supertypesOf, subtypesOf }

partial def validateTypeExpr
    (schemaName relationName roleName : Name)
    (objects relations earlierRoles : Std.HashSet Name) : TypeExprV1 → Except String Unit
  | .object target =>
      if objects.contains target then pure ()
      else throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` references unknown object `{target}`"
  | .relationObject target =>
      if relations.contains target then pure ()
      else throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` references unknown relation object `{target}`"
  | .indexed base overRoles => do
      validateTypeExpr schemaName relationName roleName objects relations earlierRoles base
      if overRoles.isEmpty then
        throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` has an empty index"
      for indexRole in overRoles do
        if !earlierRoles.contains indexRole then
          throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` indexes unknown or non-earlier role `{indexRole}`"
  | .refined base predicates => do
      validateTypeExpr schemaName relationName roleName objects relations earlierRoles base
      if predicates.isEmpty then
        throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` has an empty refinement"
      for predicate in predicates do
        match predicate with
        | .memberOf values | .enum values =>
            if values.isEmpty then
              throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` has an empty finite refinement"
        | .cardinality min max =>
            if max < min then
              throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` has inverted cardinality bounds"
        | .key roles =>
            if roles.isEmpty then
              throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` has an empty key refinement"
            for keyRole in roles do
              if !earlierRoles.contains keyRole then
                throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` key references unknown or non-earlier role `{keyRole}`"
        | .predicate name _ =>
            if name != "non_empty" then
              throw s!"schema `{schemaName}` relation `{relationName}` role `{roleName}` uses unsupported predicate `{name}`"
        | .equals _ => pure ()

def validateSchema (schema : SchemaV1Schema) : Except String SchemaIndex := do
  let mut objects : Std.HashSet Name := {}
  for objectName in schema.objects do
    if objects.contains objectName then
      throw s!"schema `{schema.name}` declares duplicate object `{objectName}`"
    objects := objects.insert objectName

  let mut relationNames : Std.HashSet Name := {}
  for relation in schema.relations do
    if relationNames.contains relation.name then
      throw s!"schema `{schema.name}` declares duplicate relation `{relation.name}`"
    if objects.contains relation.name then
      throw s!"schema `{schema.name}` declares both object and relation `{relation.name}`"
    relationNames := relationNames.insert relation.name

  for relation in schema.relations do
    let mut earlierRoles : Std.HashSet Name := {}
    for role in relation.fields do
      if earlierRoles.contains role.field then
        throw s!"schema `{schema.name}` relation `{relation.name}` declares duplicate role `{role.field}`"
      validateTypeExpr schema.name relation.name role.field objects relationNames earlierRoles role.ty
      earlierRoles := earlierRoles.insert role.field

  let mut subtypeEdges : Std.HashSet (Name × Name) := {}
  for subtype in schema.subtypes do
    if !objects.contains subtype.sub then
      throw s!"schema `{schema.name}` subtype references unknown subtype `{subtype.sub}`"
    if !objects.contains subtype.sup then
      throw s!"schema `{schema.name}` subtype references unknown supertype `{subtype.sup}`"
    if subtypeEdges.contains (subtype.sub, subtype.sup) then
      throw s!"schema `{schema.name}` repeats subtype `{subtype.sub} < {subtype.sup}`"
    subtypeEdges := subtypeEdges.insert (subtype.sub, subtype.sup)

  let index := SchemaIndex.ofSchema schema
  for subtype in schema.subtypes do
    if subtype.sub == subtype.sup || index.isSubtype subtype.sup subtype.sub then
      throw s!"schema `{schema.name}` has a subtype cycle involving `{subtype.sub}` and `{subtype.sup}`"

  let mut generatorNames : Std.HashSet Name := {}
  for generator in schema.generators do
    if generatorNames.contains generator.name || objects.contains generator.name || relationNames.contains generator.name then
      throw s!"schema `{schema.name}` declares duplicate or colliding generator `{generator.name}`"
    generatorNames := generatorNames.insert generator.name
    for endpoint in #[generator.source, generator.target] do
      if !objects.contains endpoint && !relationNames.contains endpoint then
        throw s!"schema `{schema.name}` generator `{generator.name}` references unknown endpoint `{endpoint}`"

  pure index

def findNamedCarrierPair
    (relation : RelationDeclV1) : List (Name × Name) → Option (FieldDeclV1 × FieldDeclV1)
  | [] => none
  | (left, right) :: rest =>
      match relation.fields.find? (fun field => field.field == left),
          relation.fields.find? (fun field => field.field == right) with
      | some leftField, some rightField => some (leftField, rightField)
      | _, _ => findNamedCarrierPair relation rest

def relationNameHintsHomotopy (name : Name) : Bool :=
  let lowered := name.toLower
  (lowered.splitOn "equiv").length > 1 || (lowered.splitOn "homotopy").length > 1

def relationCarrierPair? (relation : RelationDeclV1) : Option (FieldDeclV1 × FieldDeclV1) :=
  let homotopy := findNamedCarrierPair relation [
    ("lhs", "rhs"), ("route1", "route2"), ("path1", "path2"),
    ("rel1", "rel2"), ("i1", "i2"), ("s1", "s2"), ("left", "right")]
  let endpoint := findNamedCarrierPair relation [
    ("from", "to"), ("source", "target"), ("src", "dst")]
  let dataFields := relation.fields.filter (fun field => field.kind == .data)
  let declaredOrder :=
    match dataFields[0]?, dataFields[1]?, dataFields[2]? with
    | some sourceRole, some targetRole, none => some (sourceRole, targetRole)
    | _, _, _ => none
  match homotopy with
  | some pair =>
      if endpoint.isSome || relationNameHintsHomotopy relation.name then
        some pair
      else
        declaredOrder
  | none =>
      match endpoint with
      | some pair => some pair
      | none => declaredOrder

structure RewriteTypeEnv where
  objectVars : Std.HashMap Name Name := {}
  pathVars : Std.HashMap Name (Name × Name) := {}

def inferRewriteEndpoints
    (index : SchemaIndex) (env : RewriteTypeEnv) : PathExprV3 → Except String (Name × Name)
  | .var name =>
      match env.pathVars.get? name with
      | some endpoints => pure endpoints
      | none => throw s!"unbound path variable `{name}`"
  | .reflexive entity =>
      if env.objectVars.contains entity then pure (entity, entity)
      else throw s!"unbound object variable `{entity}`"
  | .step src relation dst => do
      let declaration ← index.relationDecl relation
      let some (sourceRole, targetRole) := relationCarrierPair? declaration
        | throw s!"relation `{relation}` does not expose a compiled carrier pair"
      let some sourceType := env.objectVars.get? src
        | throw s!"unbound object variable `{src}`"
      let some targetType := env.objectVars.get? dst
        | throw s!"unbound object variable `{dst}`"
      if !index.isSubtype sourceType sourceRole.ty.referencedName then
        throw s!"`{src}` has type `{sourceType}`, expected subtype of `{sourceRole.ty.referencedName}`"
      if !index.isSubtype targetType targetRole.ty.referencedName then
        throw s!"`{dst}` has type `{targetType}`, expected subtype of `{targetRole.ty.referencedName}`"
      pure (src, dst)
  | .trans left right => do
      let (a, b) ← inferRewriteEndpoints index env left
      let (c, d) ← inferRewriteEndpoints index env right
      if b != c then
        throw s!"cannot compose paths because the left path ends at `{b}` and the right path starts at `{c}`"
      pure (a, d)
  | .inv path => do
      let (a, b) ← inferRewriteEndpoints index env path
      pure (b, a)

def validateRewriteRule
    (theoryName : Name) (index : SchemaIndex) (rule : RewriteRuleV1) : Except String Unit := do
  let mut env : RewriteTypeEnv := {}
  let mut seenVariableNames : Std.HashSet Name := {}
  let mut pendingPathVars : Array (Name × Name × Name) := #[]
  for rewriteVar in rule.vars do
    if seenVariableNames.contains rewriteVar.name then
      throw s!"theory `{theoryName}` rewrite rule `{rule.name}` declares duplicate variable `{rewriteVar.name}`"
    seenVariableNames := seenVariableNames.insert rewriteVar.name
    match rewriteVar.ty with
    | .object ty =>
        if !index.objectTypes.contains ty then
          throw s!"theory `{theoryName}` rewrite rule `{rule.name}` references unknown object type `{ty}` for variable `{rewriteVar.name}`"
        env := { env with objectVars := env.objectVars.insert rewriteVar.name ty }
    | .path src dst =>
        pendingPathVars := pendingPathVars.push (rewriteVar.name, src, dst)
  for (pathVar, src, dst) in pendingPathVars do
    if !env.objectVars.contains src then
      throw s!"theory `{theoryName}` rewrite rule `{rule.name}` path variable `{pathVar}` references unknown endpoint `{src}`"
    if !env.objectVars.contains dst then
      throw s!"theory `{theoryName}` rewrite rule `{rule.name}` path variable `{pathVar}` references unknown endpoint `{dst}`"
    env := { env with pathVars := env.pathVars.insert pathVar (src, dst) }
  let lhsEndpoints ← inferRewriteEndpoints index env rule.lhs
  let rhsEndpoints ← inferRewriteEndpoints index env rule.rhs
  if lhsEndpoints != rhsEndpoints then
    throw s!"theory `{theoryName}` rewrite rule `{rule.name}` has mismatched endpoints"

def validateUniqueConstraintFields
    (context : String) (fields : Array Name) : Except String Unit := do
  let mut seen : Std.HashSet Name := {}
  for field in fields do
    if seen.contains field then
      throw s!"{context} repeats field `{field}`"
    seen := seen.insert field

def validateConstraint
    (theoryName : Name) (index : SchemaIndex) : ConstraintV1 → Except String Unit
  | .functional relation srcField dstField => do
      let _ ← index.fieldDecl relation srcField
      let _ ← index.fieldDecl relation dstField
  | .atMost relation srcField dstField _ params => do
      let _ ← index.fieldDecl relation srcField
      let _ ← index.fieldDecl relation dstField
      let parameters := params.getD #[]
      validateUniqueConstraintFields s!"theory `{theoryName}` at_most constraint on relation `{relation}`" parameters
      for parameter in parameters do
        let _ ← index.fieldDecl relation parameter
  | .typing relation rule => do
      let _ ← index.relationDecl relation
      if rule.trimAscii.toString.isEmpty then
        throw s!"theory `{theoryName}` typing constraint on relation `{relation}` has an empty rule name"
  | .symmetricWhereIn relation field values carriers params => do
      let _ ← index.fieldDecl relation field
      if values.isEmpty then
        throw s!"theory `{theoryName}` symmetric-where-in constraint on relation `{relation}` must list at least one value"
      match carriers with
      | some pair =>
          if pair.leftField == pair.rightField then
            throw s!"theory `{theoryName}` symmetric where-in constraint on relation `{relation}` must name distinct carrier fields"
          let _ ← index.fieldDecl relation pair.leftField
          let _ ← index.fieldDecl relation pair.rightField
      | none => pure ()
      let parameters := params.getD #[]
      validateUniqueConstraintFields s!"theory `{theoryName}` symmetric where-in constraint on relation `{relation}`" parameters
      for parameter in parameters do
        let _ ← index.fieldDecl relation parameter
  | .symmetric relation carriers params | .transitive relation carriers params => do
      let _ ← index.relationDecl relation
      match carriers with
      | some pair =>
          if pair.leftField == pair.rightField then
            throw s!"theory `{theoryName}` closure constraint on relation `{relation}` must name distinct carrier fields"
          let _ ← index.fieldDecl relation pair.leftField
          let _ ← index.fieldDecl relation pair.rightField
      | none => pure ()
      let parameters := params.getD #[]
      validateUniqueConstraintFields s!"theory `{theoryName}` closure constraint on relation `{relation}`" parameters
      for parameter in parameters do
        let _ ← index.fieldDecl relation parameter
  | .key relation fields => do
      let _ ← index.relationDecl relation
      if fields.isEmpty then
        throw s!"theory `{theoryName}` key constraint on relation `{relation}` must name at least one field"
      validateUniqueConstraintFields s!"theory `{theoryName}` key constraint on relation `{relation}`" fields
      for field in fields do
        let _ ← index.fieldDecl relation field
  | .namedBlock name body =>
      if name.trimAscii.toString.isEmpty then
        throw s!"theory `{theoryName}` has a named constraint block with an empty name"
      else if body.all (fun line => line.trimAscii.toString.isEmpty) then
        throw s!"theory `{theoryName}` named constraint block `{name}` must not be empty"
      else pure ()
  | .unknown text =>
      if text.trimAscii.toString.isEmpty then
        throw s!"theory `{theoryName}` contains an empty unknown constraint"
      else pure ()

inductive AssignmentKind where
  | object
  | relation
  deriving Repr, DecidableEq

def classifyAssignment (a : InstanceAssignmentV1) : Except String AssignmentKind := do
  let allIdents := a.value.items.all (fun it => match it with | .ident _ => true | _ => false)
  let allTuples := a.value.items.all (fun it => match it with | .tuple _ _ => true | _ => false)
  if !(allIdents || allTuples) then
    throw s!"assignment `{a.name}` mixes identifiers and tuples"
  if allIdents then
    pure .object
  else
    pure .relation

def getOrCreateEntity
    (idx : SchemaIndex)
    (entities : Std.HashSet (Name × Name))
    (desiredType : Name)
    (name : Name) :
    Except String (Std.HashSet (Name × Name)) := do

  if !(idx.objectTypes.contains desiredType) then
    throw s!"unknown object type `{desiredType}` (while checking element `{name}`)"

  let related := idx.relatedTypesIncludingSelf desiredType
  let candidates := related.filter (fun ty => entities.contains (ty, name))

  if candidates.length > 1 then
    throw s!"ambiguous element `{name}`: multiple entities exist across related types for `{desiredType}`: {candidates}"

  if candidates.length == 1 then
    let existingType := candidates.head!
    if idx.isSubtype desiredType existingType && desiredType != existingType then
      -- Upgrade to the more specific type.
      let entities := (entities.erase (existingType, name)).insert (desiredType, name)
      pure entities
    else
      pure entities
  else
    pure (entities.insert (desiredType, name))

def checkRelationTuple
    (idx : SchemaIndex)
    (instName : Name)
    (relationName : Name)
    (decl : RelationDeclV1)
    (factLabels : Std.HashMap Name Name)
    (entities : Std.HashSet (Name × Name))
    (fields : Array (Name × Name)) :
    Except String (Std.HashSet (Name × Name)) := do

  let declaredFields : Std.HashSet Name :=
    decl.fields.foldl (fun acc f => acc.insert f.field) {}

  let mut tupleMap : Std.HashMap Name Name := {}
  for (fieldName, valueName) in fields do
    if tupleMap.contains fieldName then
      throw s!"instance `{instName}` relation `{relationName}`: duplicate field `{fieldName}` in tuple"
    if !(declaredFields.contains fieldName) then
      throw s!"instance `{instName}` relation `{relationName}`: unknown field `{fieldName}`"
    tupleMap := tupleMap.insert fieldName valueName

  let mut entities := entities
  for f in decl.fields do
    let some valueName := tupleMap.get? f.field
      | throw s!"instance `{instName}` relation `{relationName}`: missing field `{f.field}` in tuple"

    let target := f.ty.referencedName
    if !(idx.objectTypes.contains target) && f.ty.relationObjectName?.isNone then
      throw s!"instance `{instName}` relation `{relationName}`: field `{f.field}` expects unknown object type `{target}`"

    match f.ty.relationObjectName? with
    | none => entities ← getOrCreateEntity idx entities target valueName
    | some targetRelation =>
        if factLabels.get? valueName != some targetRelation then
          throw s!"instance `{instName}` relation `{relationName}` field `{f.field}` references unknown fact label `{valueName}` of relation `{targetRelation}`"

  pure entities

def typecheckInstance
    (schemas : Std.HashMap Name SchemaIndex)
    (inst : SchemaV1Instance) :
    Except String Unit := do

  let some idx := schemas.get? inst.schema
    | throw s!"instance `{inst.name}` references unknown schema `{inst.schema}`"

  let mut seenAssignments : Std.HashSet Name := {}
  let mut factLabels : Std.HashMap Name Name := {}
  for assignment in inst.assignments do
    if seenAssignments.contains assignment.name then
      throw s!"instance `{inst.name}` repeats assignment `{assignment.name}`"
    seenAssignments := seenAssignments.insert assignment.name
    if idx.relationDecls.contains assignment.name then
      for item in assignment.value.items do
        match item with
        | .tuple (some label) _ =>
            if factLabels.contains label then
              throw s!"instance `{inst.name}` repeats fact label `{label}`"
            factLabels := factLabels.insert label assignment.name
        | _ => pure ()

  let mut entities : Std.HashSet (Name × Name) := {}
  for a in inst.assignments do
    let kind ← classifyAssignment a
    match kind with
    | .object =>
        if !(idx.objectTypes.contains a.name) && (idx.relationDecls.contains a.name) then
          throw s!"instance `{inst.name}` assignment `{a.name}` contains identifiers but `{a.name}` is declared as a relation"
        if !(idx.objectTypes.contains a.name) then
          throw s!"instance `{inst.name}` assignment `{a.name}` contains identifiers but `{a.name}` is not a declared object type"
        for it in a.value.items do
          match it with
          | .ident n =>
              entities ← getOrCreateEntity idx entities a.name n
          | .tuple _ _ => pure ()
    | .relation =>
        match idx.relationDecls.get? a.name with
        | some relDecl =>
            for it in a.value.items do
              match it with
              | .tuple _ fields =>
                  let ents ← checkRelationTuple idx inst.name a.name relDecl factLabels entities fields
                  entities := ents
              | .ident _ => pure ()
        | none =>
            let some generator := idx.generatorDecls.get? a.name
              | throw s!"instance `{inst.name}` assignment `{a.name}` contains tuples but `{a.name}` is not a declared relation or generator in schema `{inst.schema}`"
            for item in a.value.items do
              let .tuple none fields := item
                | throw s!"instance `{inst.name}` generator `{generator.name}` expects unlabeled tuples"
              let mut mapping : Std.HashMap Name Name := {}
              for (field, value) in fields do
                if mapping.contains field then
                  throw s!"instance `{inst.name}` generator `{generator.name}` repeats field `{field}`"
                mapping := mapping.insert field value
              if mapping.size != 2 || !(mapping.contains "source") || !(mapping.contains "target") then
                throw s!"instance `{inst.name}` generator `{generator.name}` expects exactly source/target fields"
              for (endpoint, targetType) in #[
                ("source", generator.source),
                ("target", generator.target)
              ] do
                let value := mapping.get! endpoint
                if idx.relationDecls.contains targetType then
                  if factLabels.get? value != some targetType then
                    throw s!"instance `{inst.name}` generator `{generator.name}` {endpoint} references unknown fact `{value}` of relation `{targetType}`"
                else
                  entities ← getOrCreateEntity idx entities targetType value

  pure ()

structure EquationTypeEnv where
  objectVars : Std.HashMap Name Name := {}

structure EquationEndpointInference where
  env : EquationTypeEnv
  source : Name
  target : Name


def unifyEquationValueRequirement
    (index : SchemaIndex) (env : EquationTypeEnv) (variableName expectedType : Name) :
    Except String EquationTypeEnv := do
  if !index.isValueType expectedType then
    throw s!"unknown value type `{expectedType}` in schema"
  match env.objectVars.get? variableName with
  | none => pure { env with objectVars := env.objectVars.insert variableName expectedType }
  | some existing =>
      if existing == expectedType then
        pure env
      else if index.isSubtype expectedType existing then
        pure { env with objectVars := env.objectVars.insert variableName expectedType }
      else if index.isSubtype existing expectedType then
        pure env
      else
        throw s!"variable `{variableName}` is required to have incompatible object types `{existing}` and `{expectedType}`"

partial def inferEquationEndpoints
    (index : SchemaIndex) (env : EquationTypeEnv) : PathExprV3 →
    Except String EquationEndpointInference
  | .var name =>
      throw s!"free path variable `{name}` is not yet supported in runtime-checked path equations"
  | .reflexive entity =>
      pure { env, source := entity, target := entity }
  | .step src relation dst => do
      let declaration ← index.relationDecl relation
      let some (sourceRole, targetRole) := relationCarrierPair? declaration
        | throw s!"relation `{relation}` does not expose a compiled carrier pair"
      let env ← unifyEquationValueRequirement index env src sourceRole.ty.referencedName
      let env ← unifyEquationValueRequirement index env dst targetRole.ty.referencedName
      pure { env, source := src, target := dst }
  | .trans left right => do
      let leftResult ← inferEquationEndpoints index env left
      let rightResult ← inferEquationEndpoints index leftResult.env right
      if leftResult.target != rightResult.source then
        throw s!"cannot compose paths because the left path ends at `{leftResult.target}` and the right path starts at `{rightResult.source}`"
      pure { env := rightResult.env, source := leftResult.source, target := rightResult.target }
  | .inv path => do
      let result ← inferEquationEndpoints index env path
      pure { env := result.env, source := result.target, target := result.source }


def validateEquation
    (theoryName : Name) (index : SchemaIndex) (equation : EquationV1) : Except String Unit := do
  if equation.lhs.trimAscii.toString.isEmpty || equation.rhs.trimAscii.toString.isEmpty then
    throw s!"theory `{theoryName}` equation `{equation.name}` must have non-empty lhs and rhs"
  match parsePathExprV3FromString equation.lhs, parsePathExprV3FromString equation.rhs with
  | .ok lhs, .ok rhs =>
      let lhsResult ←
        inferEquationEndpoints index {} lhs |>.mapError (fun message =>
          s!"theory `{theoryName}` equation `{equation.name}` lhs ill-typed: {message}")
      let rhsResult ←
        inferEquationEndpoints index lhsResult.env rhs |>.mapError (fun message =>
          s!"theory `{theoryName}` equation `{equation.name}` rhs ill-typed: {message}")
      if lhsResult.source != rhsResult.source || lhsResult.target != rhsResult.target then
        throw s!"theory `{theoryName}` equation `{equation.name}` has mismatched path endpoints lhs=Path({lhsResult.source},{lhsResult.target}) rhs=Path({rhsResult.source},{rhsResult.target})"
  | _, _ => pure ()

def typecheckModule (m : Axiograph.Axi.AxiV1.AxiV1Module) : Except String TypeCheckSummaryV1 := do
  let mut schemas : Std.HashMap Name SchemaIndex := {}
  for s in m.schemas do
    if schemas.contains s.name then
      throw s!"duplicate schema `{s.name}` in module"
    schemas := schemas.insert s.name (← validateSchema s)

  let mut theories : Std.HashSet (Name × Name) := {}
  for theory in m.theories do
    if !schemas.contains theory.schema then
      throw s!"theory `{theory.name}` references unknown schema `{theory.schema}`"
    if theories.contains (theory.schema, theory.name) then
      throw s!"duplicate theory `{theory.name}` on schema `{theory.schema}`"
    let some index := schemas.get? theory.schema
      | throw s!"theory `{theory.name}` references unknown schema `{theory.schema}`"
    let mut equationNames : Std.HashSet Name := {}
    for equation in theory.equations do
      if equationNames.contains equation.name then
        throw s!"theory `{theory.name}` declares duplicate equation `{equation.name}`"
      equationNames := equationNames.insert equation.name
      validateEquation theory.name index equation
    let mut rewriteNames : Std.HashSet Name := {}
    for rule in theory.rewriteRules do
      if rewriteNames.contains rule.name then
        throw s!"theory `{theory.name}` declares duplicate rewrite rule `{rule.name}`"
      rewriteNames := rewriteNames.insert rule.name
      validateRewriteRule theory.name index rule
    for constraint in theory.constraints do
      validateConstraint theory.name index constraint
    theories := theories.insert (theory.schema, theory.name)

  let mut instances : Std.HashSet (Name × Name) := {}
  for inst in m.instances do
    if instances.contains (inst.schema, inst.name) then
      throw s!"duplicate instance `{inst.name}` on schema `{inst.schema}`"
    instances := instances.insert (inst.schema, inst.name)
    typecheckInstance schemas inst

  pure (TypeCheckSummaryV1.ofModule m)

/-!
## Bridging to Lean’s type system

The functions above implement a *decidable* well-typedness check for canonical
`.axi` modules.

To make this usable throughout the trusted layer, we also expose a Prop-level
predicate and a dependent wrapper:

* `WellTypedModule m : Prop` is the specification boundary (“this module is
  well-typed”).
* `TypedModule` packages a module together with a proof of well-typedness.

This is a lightweight but important pattern:

* Rust can remain the untrusted engine that emits results + certificates.
* Lean keeps a small kernel by checking the certificate and producing (when it
  accepts) a value that is *typed by construction*.

Later, as the Lean kernel grows, `WellTypedModule` can be refined to a richer
logical specification without changing its consumers.
-/

structure WellTypedModuleWitness (m : Axiograph.Axi.AxiV1.AxiV1Module) where
  summary : TypeCheckSummaryV1
  ok : typecheckModule m = .ok summary

def WellTypedModule (m : Axiograph.Axi.AxiV1.AxiV1Module) : Prop :=
  Nonempty (WellTypedModuleWitness m)

def typecheckModuleWitness (m : Axiograph.Axi.AxiV1.AxiV1Module) :
    Except String (WellTypedModuleWitness m) := by
  -- We keep the witness equality, so downstream code can use it directly.
  classical
  cases h : typecheckModule m with
  | ok summary =>
      exact .ok { summary := summary, ok := h }
  | error msg =>
      exact .error msg

instance (m : Axiograph.Axi.AxiV1.AxiV1Module) : Decidable (WellTypedModule m) := by
  classical
  cases h : typecheckModule m with
  | ok summary =>
      exact isTrue ⟨⟨summary, h⟩⟩
  | error msg =>
      exact isFalse (by
        intro hWT
        rcases hWT with ⟨w⟩
        -- `typecheckModule m` can’t be both `.error _` and `.ok _`.
        have impossible :
            (Except.error msg : Except String TypeCheckSummaryV1) =
              Except.ok w.summary := by
          simpa [h] using w.ok
        cases impossible)

abbrev TypedModule := { m : Axiograph.Axi.AxiV1.AxiV1Module // WellTypedModule m }

end Axiograph.Axi.TypeCheck
