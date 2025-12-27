import Std
import Axiograph.Axi.AxiV1
import Axiograph.Axi.TypeCheck

/-!
# Core constraint checking (AST-level)

This module implements a conservative subset of theory-constraint checking for
canonical `.axi` modules.

Why a separate checker?

* `Axiograph.Axi.TypeCheck` is a **well-typedness** gate (names/fields/types).
* Ontology engineering workflows also need **data-quality** gates:
  keys, functionals, and other invariants that make schemas usable for
  optimization and querying.

This file is intentionally small and auditable: it is designed to be used by a
certificate kind (Rust emits, Lean re-checks).

## Scope (initial release)

We start with constraints that are:

* low ambiguity across dialects,
* common in query planning, and
* easy to explain.

Certified subset:

* `constraint key Rel(field, ...)`
* `constraint functional Rel.field -> Rel.field`

We intentionally do **not** certify conditional constraints (`... where ...`) or
global entailment/inference in this first pass.
-/

namespace Axiograph.Axi.ConstraintsCheck

open Axiograph.Axi.SchemaV1

structure ConstraintsCheckSummaryV1 where
  moduleName : Name
  constraintCount : Nat
  instanceCount : Nat
  checkCount : Nat
  deriving Repr, DecidableEq

private inductive CoreConstraint where
  | key (schema : Name) (relation : Name) (fields : Array Name)
  | functional (schema : Name) (relation : Name) (srcField : Name) (dstField : Name)
  deriving Repr, DecidableEq

private def gatherCoreConstraints (m : Axiograph.Axi.AxiV1.AxiV1Module) : Array CoreConstraint :=
  Id.run do
    let mut out : Array CoreConstraint := #[]
    for th in m.theories do
      for c in th.constraints do
        match c with
        | .key rel fields =>
            out := out.push (.key th.schema rel fields)
        | .functional rel src dst =>
            out := out.push (.functional th.schema rel src dst)
        | _ =>
            pure ()
    out

private def relationTuples
    (inst : SchemaV1Instance)
    (relationName : Name) : Array (Array (Name × Name)) :=
  Id.run do
    let mut out : Array (Array (Name × Name)) := #[]
    for a in inst.assignments do
      if a.name != relationName then
        continue
      for it in a.value.items do
        match it with
        | .tuple fields => out := out.push fields
        | _ => pure ()
    out

private def tupleToMap
    (instName relationName : Name)
    (fields : Array (Name × Name)) :
    Except String (Std.HashMap Name Name) := do
  let mut m : Std.HashMap Name Name := {}
  for (k, v) in fields do
    if m.contains k then
      throw s!"instance `{instName}` relation `{relationName}`: duplicate field `{k}` in tuple"
    m := m.insert k v
  pure m

private def checkKeyConstraint
    (inst : SchemaV1Instance)
    (relationName : Name)
    (keyFields : Array Name) : Except String Unit := do
  if keyFields.isEmpty then
    pure ()
  let tuples := relationTuples inst relationName
  let mut seen : Std.HashMap (List Name) Nat := {}
  for i in List.range tuples.size do
    let fields := tuples[i]!
    let tmap ← tupleToMap inst.name relationName fields
    let mut key : List Name := []
    for f in keyFields do
      let some v := tmap.get? f
        | throw s!"instance `{inst.name}` relation `{relationName}`: key field `{f}` missing from tuple"
      key := key.concat v
    match seen.get? key with
    | some prev =>
        throw s!"key violation in instance `{inst.name}` on `{relationName}({String.intercalate ", " keyFields.toList})`: duplicate key at tuples {prev} and {i}"
    | none =>
        seen := seen.insert key i

private def checkFunctionalConstraint
    (inst : SchemaV1Instance)
  (relationName : Name)
  (srcField dstField : Name) : Except String Unit := do
  let tuples := relationTuples inst relationName
  let mut map : Std.HashMap Name Name := {}
  for i in List.range tuples.size do
    let fields := tuples[i]!
    let tmap ← tupleToMap inst.name relationName fields
    let some src := tmap.get? srcField
      | throw s!"instance `{inst.name}` relation `{relationName}`: functional src field `{srcField}` missing from tuple"
    let some dst := tmap.get? dstField
      | throw s!"instance `{inst.name}` relation `{relationName}`: functional dst field `{dstField}` missing from tuple"
    match map.get? src with
    | some prev =>
        if prev != dst then
          throw s!"functional violation in instance `{inst.name}` on `{relationName}`.{srcField} -> {relationName}.{dstField}: src `{src}` maps to both `{prev}` and `{dst}` (tuple {i})"
    | none =>
        map := map.insert src dst

def checkModule (m : Axiograph.Axi.AxiV1.AxiV1Module) : Except String ConstraintsCheckSummaryV1 := do
  -- First, require AST well-typedness (keeps errors clearer and avoids
  -- constraint checks running on malformed tuples).
  let _ ← Axiograph.Axi.TypeCheck.typecheckModule m

  let constraints := gatherCoreConstraints m
  let mut checkCount : Nat := 0

  for inst in m.instances do
    for c in constraints do
      match c with
      | .key schema rel fields =>
          if schema == inst.schema then
            checkCount := checkCount + 1
            checkKeyConstraint inst rel fields
      | .functional schema rel src dst =>
          if schema == inst.schema then
            checkCount := checkCount + 1
            checkFunctionalConstraint inst rel src dst

  pure {
    moduleName := m.moduleName
    constraintCount := constraints.size
    instanceCount := m.instances.size
    checkCount := checkCount
  }

end Axiograph.Axi.ConstraintsCheck
