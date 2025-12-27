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

For each instance in a module:

1. The referenced schema exists.
2. Every object assignment is to a declared object type.
3. Every relation assignment is to a declared relation.
4. Every tuple has **exactly** the declared fields (no missing/extra/duplicate fields).
5. Relation tuples may introduce objects implicitly, but subtyping-based reuse is
   checked for ambiguity: using a name at a supertype must not be ambiguous across
   multiple subtype inhabitants with that name.

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
                | .tuple _ => 1
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
  supertypesOf : Std.HashMap Name (Std.HashSet Name)
  subtypesOf : Std.HashMap Name (Std.HashSet Name)
  deriving Repr

def SchemaIndex.isSubtype (idx : SchemaIndex) (sub sup : Name) : Bool :=
  match idx.supertypesOf.get? sub with
  | none => sub == sup
  | some supers => supers.contains sup

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
  let supertypesOf := computeSupertypesClosure objectTypes s.subtypes
  let subtypesOf := computeSubtypesClosure objectTypes s.subtypes
  { objectTypes, relationDecls, supertypesOf, subtypesOf }

inductive AssignmentKind where
  | object
  | relation
  deriving Repr, DecidableEq

def classifyAssignment (a : InstanceAssignmentV1) : Except String AssignmentKind := do
  let allIdents := a.value.items.all (fun it => match it with | .ident _ => true | _ => false)
  let allTuples := a.value.items.all (fun it => match it with | .tuple _ => true | _ => false)
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

    if !(idx.objectTypes.contains f.ty) then
      throw s!"instance `{instName}` relation `{relationName}`: field `{f.field}` expects unknown object type `{f.ty}`"

    entities ← getOrCreateEntity idx entities f.ty valueName

  pure entities

def typecheckInstance
    (schemas : Std.HashMap Name SchemaIndex)
    (inst : SchemaV1Instance) :
    Except String Unit := do

  let some idx := schemas.get? inst.schema
    | throw s!"instance `{inst.name}` references unknown schema `{inst.schema}`"

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
          | .tuple _ => pure ()
    | .relation =>
        let some relDecl := idx.relationDecls.get? a.name
          | throw s!"instance `{inst.name}` assignment `{a.name}` contains tuples but `{a.name}` is not a declared relation in schema `{inst.schema}`"
        for it in a.value.items do
          match it with
          | .tuple fields =>
              let ents ← checkRelationTuple idx inst.name a.name relDecl entities fields
              entities := ents
          | .ident _ => pure ()

  pure ()

def typecheckModule (m : Axiograph.Axi.AxiV1.AxiV1Module) : Except String TypeCheckSummaryV1 := do
  let mut schemas : Std.HashMap Name SchemaIndex := {}
  for s in m.schemas do
    if schemas.contains s.name then
      throw s!"duplicate schema `{s.name}` in module"
    schemas := schemas.insert s.name (SchemaIndex.ofSchema s)

  for inst in m.instances do
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

Later, as we port more of the Idris semantics, `WellTypedModule` can be refined
to a richer logical specification without changing its consumers.
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
