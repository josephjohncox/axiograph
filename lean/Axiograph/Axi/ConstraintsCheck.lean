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
* `constraint symmetric Rel`
* `constraint symmetric Rel where Rel.field in {A, B, ...}`
* `constraint transitive Rel` (closure-compatibility for keys/functionals on carrier fields)
* `constraint typing Rel: rule_name` (small builtin rule set)

We intentionally do **not** certify global entailment/inference or relational
algebra beyond these small, checkable invariants in this first pass.
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
  | symmetric (schema : Name) (relation : Name) (carriers : Option CarrierFieldsV1) (params : Option (Array Name))
  | symmetricWhereIn (schema : Name) (relation : Name) (field : Name) (values : Array Name) (carriers : Option CarrierFieldsV1) (params : Option (Array Name))
  | transitive (schema : Name) (relation : Name) (carriers : Option CarrierFieldsV1) (params : Option (Array Name))
  | typing (schema : Name) (relation : Name) (rule : Name)
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
        | .symmetric rel carriers params =>
            out := out.push (.symmetric th.schema rel carriers params)
        | .symmetricWhereIn rel field values carriers params =>
            out := out.push (.symmetricWhereIn th.schema rel field values carriers params)
        | .transitive rel carriers params =>
            out := out.push (.transitive th.schema rel carriers params)
        | .typing rel rule =>
            out := out.push (.typing th.schema rel rule)
        | _ =>
            pure ()
    out

private def findSchema? (m : Axiograph.Axi.AxiV1.AxiV1Module) (schemaName : Name) :
    Option SchemaV1Schema :=
  m.schemas.find? (fun s => s.name == schemaName)

private def findRelationDecl?
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (schemaName relationName : Name) : Option RelationDeclV1 := do
  let schema ← findSchema? m schemaName
  schema.relations.find? (fun r => r.name == relationName)

private def relationFieldOrder
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (schemaName relationName : Name) : Except String (Array Name) := do
  let some rel := findRelationDecl? m schemaName relationName
    | throw s!"unknown relation `{relationName}` in schema `{schemaName}`"
  pure (rel.fields.map (fun f => f.field))

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

private def tupleValuesInOrder
    (instName relationName : Name)
    (tupleFields : Array (Name × Name))
    (orderedFields : Array Name) :
    Except String (List Name) := do
  let tmap ← tupleToMap instName relationName tupleFields
  let mut out : List Name := []
  for f in orderedFields do
    let some v := tmap.get? f
      | throw s!"instance `{instName}` relation `{relationName}`: missing field `{f}` in tuple"
    out := out.concat v
  pure out

private def fieldIndex
    (instName relationName : Name)
    (relationFields : Array Name)
    (field : Name) : Except String Nat := do
  let some idx := relationFields.findIdx? (fun f => f == field)
    | throw s!"instance `{instName}` relation `{relationName}`: field `{field}` is not declared in the schema"
  pure idx

private def listGet!
    (instName relationName : Name)
    (xs : List Name)
    (idx : Nat) : Except String Name := do
  let rec get? : List Name → Nat → Option Name
    | [], _ => none
    | x :: _, 0 => some x
    | _ :: xs, i + 1 => get? xs i

  let some v := get? xs idx
    | throw s!"instance `{instName}` relation `{relationName}`: internal error (tuple too short)"
  pure v

private def listSet!
    (instName relationName : Name)
    (xs : List Name)
    (idx : Nat)
    (value : Name) : Except String (List Name) := do
  let rec go : List Name → Nat → Except String (List Name)
    | [], _ => throw s!"instance `{instName}` relation `{relationName}`: internal error (tuple too short)"
    | _ :: xs, 0 => pure (value :: xs)
    | x :: xs, i + 1 => do
        let rest ← go xs i
        pure (x :: rest)
  go xs idx

private def checkKeyOnTuples
    (instName relationName : Name)
    (relationFields : Array Name)
    (tuples : Array (List Name))
    (keyFields : Array Name) : Except String Unit := do
  if keyFields.isEmpty then
    pure ()
  let mut keyIdxs : Array Nat := #[]
  for f in keyFields do
    keyIdxs := keyIdxs.push (← fieldIndex instName relationName relationFields f)

  let mut seen : Std.HashMap (List Name) Nat := {}
  for i in List.range tuples.size do
    let tuple := tuples[i]!
    let mut key : List Name := []
    for idx in keyIdxs do
      key := key.concat (← listGet! instName relationName tuple idx)
    match seen.get? key with
    | some prev =>
        throw s!"key violation in instance `{instName}` on `{relationName}({String.intercalate ", " keyFields.toList})`: duplicate key at tuples {prev} and {i}"
    | none =>
        seen := seen.insert key i

private def checkFunctionalOnTuples
    (instName relationName : Name)
    (relationFields : Array Name)
    (tuples : Array (List Name))
    (srcField dstField : Name) : Except String Unit := do
  let srcIdx ← fieldIndex instName relationName relationFields srcField
  let dstIdx ← fieldIndex instName relationName relationFields dstField
  let mut map : Std.HashMap Name Name := {}
  for i in List.range tuples.size do
    let tuple := tuples[i]!
    let src ← listGet! instName relationName tuple srcIdx
    let dst ← listGet! instName relationName tuple dstIdx
    match map.get? src with
    | some prev =>
        if prev != dst then
          throw s!"functional violation in instance `{instName}` on `{relationName}`.{srcField} -> {relationName}.{dstField}: src `{src}` maps to both `{prev}` and `{dst}` (tuple {i})"
    | none =>
        map := map.insert src dst

private def symmetricClosure
    (instName relationName : Name)
    (relationFields : Array Name)
    (tuplesRaw : Array (Array (Name × Name)))
    (carriers : Option CarrierFieldsV1)
    (whereField : Option Name)
    (whereValues : Array Name) : Except String (Array (List Name)) := do
  if relationFields.size < 2 then
    throw s!"instance `{instName}` relation `{relationName}`: symmetric constraint requires at least 2 fields"

  let (swapLeftIdx, swapRightIdx) ←
    match carriers with
    | none => pure (0, 1)
    | some c =>
        let leftIdx ← fieldIndex instName relationName relationFields c.leftField
        let rightIdx ← fieldIndex instName relationName relationFields c.rightField
        if leftIdx == rightIdx then
          throw s!"instance `{instName}` relation `{relationName}`: symmetric carrier fields must be distinct (got `{c.leftField}` twice)"
        pure (leftIdx, rightIdx)

  let condIdx : Option Nat ←
    match whereField with
    | none => pure none
    | some f => pure (some (← fieldIndex instName relationName relationFields f))

  let mut out : Array (List Name) := #[]
  let mut seen : Std.HashSet (List Name) := {}

  for tupFields in tuplesRaw do
    let vals ← tupleValuesInOrder instName relationName tupFields relationFields
    if !seen.contains vals then
      seen := seen.insert vals
      out := out.push vals

    let apply :=
      match condIdx with
      | none => true
      | some idx =>
          let rec get? : List Name → Nat → Option Name
            | [], _ => none
            | x :: _, 0 => some x
            | _ :: xs, i + 1 => get? xs i
          match get? vals idx with
          | some v => whereValues.contains v
          | none => false
    if apply then
      let a ← listGet! instName relationName vals swapLeftIdx
      let b ← listGet! instName relationName vals swapRightIdx
      let vals1 ← listSet! instName relationName vals swapLeftIdx b
      let swapped ← listSet! instName relationName vals1 swapRightIdx a
      if !seen.contains swapped then
        seen := seen.insert swapped
        out := out.push swapped

  pure out

private def checkSymmetricCompatibleWithKeysAndFunctionals
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (constraints : Array CoreConstraint)
    (inst : SchemaV1Instance)
    (relationName : Name)
    (carriers : Option CarrierFieldsV1)
    (params : Option (Array Name))
    (whereField : Option Name)
    (whereValues : Array Name) : Except String Unit := do
  let relationFieldsFull ← relationFieldOrder m inst.schema relationName
  let tuplesRaw := relationTuples inst relationName

  if relationFieldsFull.size < 2 then
    throw s!"instance `{inst.name}` relation `{relationName}`: symmetric constraint requires at least 2 fields"

  let (carrierLeft, carrierRight, _swapLeftIdx, _swapRightIdx) ←
    match carriers with
    | none =>
        pure (relationFieldsFull[0]!, relationFieldsFull[1]!, 0, 1)
    | some c =>
        let leftIdx ← fieldIndex inst.name relationName relationFieldsFull c.leftField
        let rightIdx ← fieldIndex inst.name relationName relationFieldsFull c.rightField
        if leftIdx == rightIdx then
          throw s!"instance `{inst.name}` relation `{relationName}`: symmetric carrier fields must be distinct (got `{c.leftField}` twice)"
        pure (c.leftField, c.rightField, leftIdx, rightIdx)

  let (relationFields, closure) ←
    match params with
    | none =>
        let closure ← symmetricClosure inst.name relationName relationFieldsFull tuplesRaw carriers whereField whereValues
        pure (relationFieldsFull, closure)
    | some ps =>
        -- Validate params (existence + no duplicates + not carriers).
        for p in ps do
          if p == carrierLeft || p == carrierRight then
            throw s!"symmetric `{inst.schema}.{relationName}`: param field `{p}` must not be a carrier field"
        -- Duplicate check.
        let mut seen : Std.HashSet Name := {}
        for p in ps do
          if seen.contains p then
            throw s!"symmetric `{inst.schema}.{relationName}`: duplicate param field `{p}`"
          seen := seen.insert p
        -- Existence check.
        for p in ps do
          let _ ← fieldIndex inst.name relationName relationFieldsFull p

        -- Build projection field order (preserve declared order).
        let allowed : Array Name := #[carrierLeft, carrierRight] ++ ps
        let mut projectionFields : Array Name := #[]
        let mut projectionIdxs : Array Nat := #[]
        for i in List.range relationFieldsFull.size do
          let f := relationFieldsFull[i]!
          if allowed.contains f then
            projectionFields := projectionFields.push f
            projectionIdxs := projectionIdxs.push i

        let some swapLeftProj := projectionFields.findIdx? (fun f => f == carrierLeft)
          | throw "internal error: symmetric carriers missing from projection fields"
        let some swapRightProj := projectionFields.findIdx? (fun f => f == carrierRight)
          | throw "internal error: symmetric carriers missing from projection fields"

        let condIdx : Option Nat ←
          match whereField with
          | none => pure none
          | some f => pure (some (← fieldIndex inst.name relationName relationFieldsFull f))

        let mut out : Array (List Name) := #[]
        let mut seenTups : Std.HashSet (List Name) := {}

        for tupFields in tuplesRaw do
          let fullVals ← tupleValuesInOrder inst.name relationName tupFields relationFieldsFull

          -- Project to (carrier + params).
          let mut projVals : List Name := []
          for idx in projectionIdxs do
            projVals := projVals.concat (← listGet! inst.name relationName fullVals idx)

          if !seenTups.contains projVals then
            seenTups := seenTups.insert projVals
            out := out.push projVals

          let apply ←
            match condIdx with
            | none => pure true
            | some idx => do
                let v ← listGet! inst.name relationName fullVals idx
                pure (whereValues.contains v)

          if apply then
            let a ← listGet! inst.name relationName projVals swapLeftProj
            let b ← listGet! inst.name relationName projVals swapRightProj
            let vals1 ← listSet! inst.name relationName projVals swapLeftProj b
            let swapped ← listSet! inst.name relationName vals1 swapRightProj a
            if !seenTups.contains swapped then
              seenTups := seenTups.insert swapped
              out := out.push swapped

        pure (projectionFields, out)

  -- Re-check keys/functionals for this relation on the symmetric closure.
  for c in constraints do
    match c with
    | .key schema rel keyFields =>
        if schema == inst.schema && rel == relationName then
          if params.isSome then
            let allowed : Array Name := #[carrierLeft, carrierRight] ++ (params.getD #[])
            for f in keyFields do
              if !allowed.contains f then
                throw s!"symmetric `{inst.schema}.{relationName}`: key constraint mentions non-carrier/non-param field `{f}`"
          checkKeyOnTuples inst.name relationName relationFields closure keyFields
    | .functional schema rel src dst =>
        if schema == inst.schema && rel == relationName then
          if params.isSome then
            let allowed : Array Name := #[carrierLeft, carrierRight] ++ (params.getD #[])
            if !allowed.contains src then
              throw s!"symmetric `{inst.schema}.{relationName}`: functional src field `{src}` is not a carrier/param field"
            if !allowed.contains dst then
              throw s!"symmetric `{inst.schema}.{relationName}`: functional dst field `{dst}` is not a carrier/param field"
          checkFunctionalOnTuples inst.name relationName relationFields closure src dst
    | _ => pure ()

private def transitiveClosurePairs
    (instName relationName : Name)
    (relationFields : Array Name)
    (tuplesRaw : Array (Array (Name × Name)))
    (carrier0Idx carrier1Idx : Nat) : Except String (Array (List Name)) := do
  if relationFields.size < 2 then
    throw s!"instance `{instName}` relation `{relationName}`: transitive constraint requires at least 2 fields"

  let mut adj : Std.HashMap Name (Array Name) := {}
  for tupFields in tuplesRaw do
    let vals ← tupleValuesInOrder instName relationName tupFields relationFields
    let a ← listGet! instName relationName vals carrier0Idx
    let b ← listGet! instName relationName vals carrier1Idx
    let current := adj.getD a #[]
    adj := adj.insert a (current.push b)

  let mut out : Array (List Name) := #[]
  for (src, neighs) in adj.toList do
    let mut visited : Std.HashSet Name := {}
    let mut queue : Array Name := neighs
    let mut i : Nat := 0
    while i < queue.size do
      let v := queue[i]!
      i := i + 1
      if visited.contains v then
        continue
      visited := visited.insert v
      out := out.push [src, v]
      let more := adj.getD v #[]
      for w in more do
        queue := queue.push w

  pure out

private def checkTransitiveCompatibleWithKeysAndFunctionals
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (constraints : Array CoreConstraint)
    (inst : SchemaV1Instance)
    (relationName : Name)
    (carriers : Option CarrierFieldsV1)
    (params : Option (Array Name)) : Except String Unit := do
  let relationFieldsFull ← relationFieldOrder m inst.schema relationName
  if relationFieldsFull.size < 2 then
    throw s!"instance `{inst.name}` relation `{relationName}`: transitive constraint requires at least 2 fields"

  let (carrier0, carrier1, carrier0Idx, carrier1Idx) ←
    match carriers with
    | none =>
        pure (relationFieldsFull[0]!, relationFieldsFull[1]!, 0, 1)
    | some c =>
        let leftIdx ← fieldIndex inst.name relationName relationFieldsFull c.leftField
        let rightIdx ← fieldIndex inst.name relationName relationFieldsFull c.rightField
        if leftIdx == rightIdx then
          throw s!"instance `{inst.name}` relation `{relationName}`: transitive carrier fields must be distinct (got `{c.leftField}` twice)"
        pure (c.leftField, c.rightField, leftIdx, rightIdx)

  -- Optional "fiber" parameters: transitivity applies within each fixed assignment
  -- of these parameters (e.g. ctx/time), rather than globally.
  let ps : Array Name := params.getD #[]
  -- Validate params (existence + no duplicates + not carriers).
  for p in ps do
    if p == carrier0 || p == carrier1 then
      throw s!"transitive `{inst.schema}.{relationName}`: param field `{p}` must not be a carrier field"
  let mut seen : Std.HashSet Name := {}
  for p in ps do
    if seen.contains p then
      throw s!"transitive `{inst.schema}.{relationName}`: duplicate param field `{p}`"
    seen := seen.insert p
  let mut paramIdxs : Array Nat := #[]
  for p in ps do
    paramIdxs := paramIdxs.push (← fieldIndex inst.name relationName relationFieldsFull p)

  let allowed : Array Name := #[carrier0, carrier1] ++ ps

  -- We only certify "closure compatibility" when keys/functionals are present.
  -- Keys may mention carrier fields and (when `param (...)` is present) param fields.
  let mut hasRelevantChecks := false
  for c in constraints do
    match c with
    | .key schema rel keyFields =>
        if schema == inst.schema && rel == relationName then
          hasRelevantChecks := true
          for f in keyFields do
            if !allowed.contains f then
              throw s!"transitive `{inst.schema}.{relationName}`: key constraint mentions non-carrier/non-param field `{f}`"
    | .functional schema rel src dst =>
        if schema == inst.schema && rel == relationName then
          hasRelevantChecks := true
          if src != carrier0 && src != carrier1 then
            throw s!"transitive `{inst.schema}.{relationName}`: functional src field `{src}` is not a carrier field (`{carrier0}` or `{carrier1}`)"
          if dst != carrier0 && dst != carrier1 then
            throw s!"transitive `{inst.schema}.{relationName}`: functional dst field `{dst}` is not a carrier field (`{carrier0}` or `{carrier1}`)"
    | _ => pure ()

  if !hasRelevantChecks then
    pure ()
  else
    let tuplesRaw := relationTuples inst relationName
    let (relationFields, closure) ←
      if paramIdxs.isEmpty then
        let closure ← transitiveClosurePairs inst.name relationName relationFieldsFull tuplesRaw carrier0Idx carrier1Idx
        pure (#[carrier0, carrier1], closure)
      else
        -- Projection field order (preserve declared order).
        let mut projectionFields : Array Name := #[]
        for f in relationFieldsFull do
          if allowed.contains f then
            projectionFields := projectionFields.push f

        -- Map param field -> index in `ps` / `paramKey`.
        let mut paramPos : Std.HashMap Name Nat := {}
        for i in List.range ps.size do
          paramPos := paramPos.insert (ps[i]!) i

        -- Build adjacency per fiber.
        let mut adjByParam : Std.HashMap (List Name) (Std.HashMap Name (Array Name)) := {}
        for tupFields in tuplesRaw do
          let fullVals ← tupleValuesInOrder inst.name relationName tupFields relationFieldsFull
          let mut pkey : List Name := []
          for idx in paramIdxs do
            pkey := pkey.concat (← listGet! inst.name relationName fullVals idx)
          let a ← listGet! inst.name relationName fullVals carrier0Idx
          let b ← listGet! inst.name relationName fullVals carrier1Idx

          let mut adj : Std.HashMap Name (Array Name) := adjByParam.getD pkey {}
          let current := adj.getD a #[]
          adj := adj.insert a (current.push b)
          adjByParam := adjByParam.insert pkey adj

        let mut out : Array (List Name) := #[]
        for (pkey, adj) in adjByParam.toList do
          -- Compute carrier closure within this fiber.
          let mut closurePairs : Std.HashSet (Name × Name) := {}
          for (src, neighs) in adj.toList do
            let mut visited : Std.HashSet Name := {}
            let mut queue : Array Name := neighs
            let mut i : Nat := 0
            while i < queue.size do
              let v := queue[i]!
              i := i + 1
              if visited.contains v then
                continue
              visited := visited.insert v
              closurePairs := closurePairs.insert (src, v)
              let more := adj.getD v #[]
              for w in more do
                queue := queue.push w

          -- Emit closure tuples in projection field order.
          for (a, b) in closurePairs.toList do
            let mut tup : List Name := []
            for f in projectionFields do
              if f == carrier0 then
                tup := tup.concat a
              else if f == carrier1 then
                tup := tup.concat b
              else
                let some i := paramPos.get? f
                  | throw s!"internal error: transitive projection field `{f}` is neither a carrier nor a param"
                -- pkey is in `ps` order.
                let pv ← listGet! inst.name relationName pkey i
                tup := tup.concat pv
            out := out.push tup

        pure (projectionFields, out)

    for c in constraints do
      match c with
      | .key schema rel keyFields =>
          if schema == inst.schema && rel == relationName then
            checkKeyOnTuples inst.name relationName relationFields closure keyFields
      | .functional schema rel src dst =>
          if schema == inst.schema && rel == relationName then
            checkFunctionalOnTuples inst.name relationName relationFields closure src dst
      | _ => pure ()

private def parseNatConst (n : Name) : Option Nat :=
  let s : String := n
  if s.startsWith "Nat" then
    let rest := s.drop 3
    rest.toNat?
  else
    none

private def natConst (n : Nat) : Name :=
  s!"Nat{n}"

private def binaryRelationMap
    (inst : SchemaV1Instance)
    (relationName keyField valueField : Name) :
    Except String (Std.HashMap Name Name) := do
  let tuples := relationTuples inst relationName
  let mut out : Std.HashMap Name Name := {}
  for tup in tuples do
    let tmap ← tupleToMap inst.name relationName tup
    let some k := tmap.get? keyField | continue
    let some v := tmap.get? valueField | continue
    match out.get? k with
    | some prev =>
        if prev != v then
          throw s!"instance `{inst.name}` relation `{relationName}`: `{keyField}` `{k}` maps to both `{prev}` and `{v}`"
    | none =>
        out := out.insert k v
  pure out

private def getField
    (instName relationName : Name)
    (relationFields : Array Name)
    (tupleVals : List Name)
    (field : Name) : Except String Name := do
  let idx ← fieldIndex instName relationName relationFields field
  listGet! instName relationName tupleVals idx

private def checkTypingConstraint
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (inst : SchemaV1Instance)
    (relationName rule : Name) : Except String Unit := do
  match rule with
  | "preserves_manifold_and_increments_degree" =>
      let _ ← relationFieldOrder m inst.schema "FormOn"
      let _ ← relationFieldOrder m inst.schema "FormDegree"
      let relFields ← relationFieldOrder m inst.schema relationName

      let formOn ← binaryRelationMap inst "FormOn" "form" "manifold"
      let formDegree ← binaryRelationMap inst "FormDegree" "form" "degree"
      let mut derivedFormOn : Std.HashMap Name Name := {}
      let mut derivedFormDegree : Std.HashMap Name Name := {}

      for tupFields in relationTuples inst relationName do
        let vals ← tupleValuesInOrder inst.name relationName tupFields relFields
        let input ← getField inst.name relationName relFields vals "input"
        let output ← getField inst.name relationName relFields vals "output"

        let some mIn := formOn.get? input
          | throw s!"typing {relationName}: missing FormOn(form={input}, manifold=...)"
        match formOn.get? output with
        | some mOut =>
            if mOut != mIn then
              throw s!"typing {relationName}: output form `{output}` is on `{mOut}`, expected `{mIn}`"
        | none => pure ()
        match derivedFormOn.get? output with
        | some prev =>
            if prev != mIn then
              throw s!"typing {relationName}: output form `{output}` inferred on both `{prev}` and `{mIn}`"
        | none =>
            derivedFormOn := derivedFormOn.insert output mIn

        let some k := formDegree.get? input
          | throw s!"typing {relationName}: missing FormDegree(form={input}, degree=...)"
        let some kNum := parseNatConst k
          | throw s!"typing {relationName}: unsupported Nat constant `{k}` (expected Nat0, Nat1, ...)"
        let kp1 := natConst (kNum + 1)

        match formDegree.get? output with
        | some kOut =>
            if kOut != kp1 then
              throw s!"typing {relationName}: output form `{output}` has degree `{kOut}`, expected `{kp1}`"
        | none => pure ()
        match derivedFormDegree.get? output with
        | some prev =>
            if prev != kp1 then
              throw s!"typing {relationName}: output form `{output}` inferred degrees conflict: `{prev}` vs `{kp1}`"
        | none =>
            derivedFormDegree := derivedFormDegree.insert output kp1
  | "preserves_manifold_and_adds_degree" =>
      let _ ← relationFieldOrder m inst.schema "FormOn"
      let _ ← relationFieldOrder m inst.schema "FormDegree"
      let relFields ← relationFieldOrder m inst.schema relationName

      let formOn ← binaryRelationMap inst "FormOn" "form" "manifold"
      let formDegree ← binaryRelationMap inst "FormDegree" "form" "degree"
      let mut derivedFormOn : Std.HashMap Name Name := {}
      let mut derivedFormDegree : Std.HashMap Name Name := {}

      for tupFields in relationTuples inst relationName do
        let vals ← tupleValuesInOrder inst.name relationName tupFields relFields
        let left ← getField inst.name relationName relFields vals "left"
        let right ← getField inst.name relationName relFields vals "right"
        let out ← getField inst.name relationName relFields vals "out"

        let some mLeft := formOn.get? left
          | throw s!"typing {relationName}: missing FormOn(form={left}, manifold=...)"
        let some mRight := formOn.get? right
          | throw s!"typing {relationName}: missing FormOn(form={right}, manifold=...)"
        if mLeft != mRight then
          throw s!"typing {relationName}: forms `{left}` and `{right}` live on different manifolds (`{mLeft}` vs `{mRight}`)"

        match formOn.get? out with
        | some mOut =>
            if mOut != mLeft then
              throw s!"typing {relationName}: output form `{out}` is on `{mOut}`, expected `{mLeft}`"
        | none => pure ()
        match derivedFormOn.get? out with
        | some prev =>
            if prev != mLeft then
              throw s!"typing {relationName}: output form `{out}` inferred on both `{prev}` and `{mLeft}`"
        | none =>
            derivedFormOn := derivedFormOn.insert out mLeft

        let some kLeft := formDegree.get? left
          | throw s!"typing {relationName}: missing FormDegree(form={left}, degree=...)"
        let some kRight := formDegree.get? right
          | throw s!"typing {relationName}: missing FormDegree(form={right}, degree=...)"
        let some kLeftNum := parseNatConst kLeft
          | throw s!"typing {relationName}: unsupported Nat constant `{kLeft}` (expected Nat0, Nat1, ...)"
        let some kRightNum := parseNatConst kRight
          | throw s!"typing {relationName}: unsupported Nat constant `{kRight}` (expected Nat0, Nat1, ...)"
        let sum := natConst (kLeftNum + kRightNum)

        match formDegree.get? out with
        | some kOut =>
            if kOut != sum then
              throw s!"typing {relationName}: output form `{out}` has degree `{kOut}`, expected `{sum}`"
        | none => pure ()
        match derivedFormDegree.get? out with
        | some prev =>
            if prev != sum then
              throw s!"typing {relationName}: output form `{out}` inferred degrees conflict: `{prev}` vs `{sum}`"
        | none =>
            derivedFormDegree := derivedFormDegree.insert out sum
  | "depends_on_metric_and_dualizes_degree" =>
      let _ ← relationFieldOrder m inst.schema "MetricOn"
      let _ ← relationFieldOrder m inst.schema "ManifoldDimension"
      let _ ← relationFieldOrder m inst.schema "FormOn"
      let _ ← relationFieldOrder m inst.schema "FormDegree"
      let relFields ← relationFieldOrder m inst.schema relationName

      let metricOn ← binaryRelationMap inst "MetricOn" "metric" "manifold"
      let manifoldDim ← binaryRelationMap inst "ManifoldDimension" "manifold" "dim"
      let formOn ← binaryRelationMap inst "FormOn" "form" "manifold"
      let formDegree ← binaryRelationMap inst "FormDegree" "form" "degree"
      let mut derivedFormOn : Std.HashMap Name Name := {}
      let mut derivedFormDegree : Std.HashMap Name Name := {}

      for tupFields in relationTuples inst relationName do
        let vals ← tupleValuesInOrder inst.name relationName tupFields relFields
        let metric ← getField inst.name relationName relFields vals "metric"
        let input ← getField inst.name relationName relFields vals "input"
        let output ← getField inst.name relationName relFields vals "output"

        let some m := metricOn.get? metric
          | throw s!"typing {relationName}: missing MetricOn(metric={metric}, manifold=...)"
        let some mIn := formOn.get? input
          | throw s!"typing {relationName}: missing FormOn(form={input}, manifold=...)"
        if mIn != m then
          throw s!"typing {relationName}: metric `{metric}` is on `{m}`, but input form `{input}` is on `{mIn}`"

        match formOn.get? output with
        | some mOut =>
            if mOut != m then
              throw s!"typing {relationName}: output form `{output}` is on `{mOut}`, expected `{m}`"
        | none => pure ()
        match derivedFormOn.get? output with
        | some prev =>
            if prev != m then
              throw s!"typing {relationName}: output form `{output}` inferred on both `{prev}` and `{m}`"
        | none =>
            derivedFormOn := derivedFormOn.insert output m

        let some n := manifoldDim.get? m
          | throw s!"typing {relationName}: missing ManifoldDimension(manifold={m}, dim=...)"
        let some k := formDegree.get? input
          | throw s!"typing {relationName}: missing FormDegree(form={input}, degree=...)"
        let some nNum := parseNatConst n
          | throw s!"typing {relationName}: unsupported Nat constant `{n}` (expected Nat0, Nat1, ...)"
        let some kNum := parseNatConst k
          | throw s!"typing {relationName}: unsupported Nat constant `{k}` (expected Nat0, Nat1, ...)"
        if nNum < kNum then
          throw s!"typing {relationName}: cannot compute n-k with n={n} and k={k}"
        let outDeg := natConst (nNum - kNum)

        match formDegree.get? output with
        | some kOut =>
            if kOut != outDeg then
              throw s!"typing {relationName}: output form `{output}` has degree `{kOut}`, expected `{outDeg}`"
        | none => pure ()
        match derivedFormDegree.get? output with
        | some prev =>
            if prev != outDeg then
              throw s!"typing {relationName}: output form `{output}` inferred degrees conflict: `{prev}` vs `{outDeg}`"
        | none =>
            derivedFormDegree := derivedFormDegree.insert output outDeg
  | other =>
      throw s!"unsupported typing constraint rule `{other}` for relation `{relationName}`"

def checkModule (m : Axiograph.Axi.AxiV1.AxiV1Module) : Except String ConstraintsCheckSummaryV1 := do
  -- First, require AST well-typedness (keeps errors clearer and avoids
  -- constraint checks running on malformed tuples).
  let _ ← Axiograph.Axi.TypeCheck.typecheckModule m

  -- Fail-closed: `axi_constraints_ok_v1` is a conservative certificate kind. If
  -- the anchored module contains truly unknown/unsupported constraints, we
  -- refuse to certify it (even if the known subset happens to pass), because
  -- that would silently ignore meaning-plane semantics drift.
  let mut unknown : Array (Name × String) := #[]
  for th in m.theories do
    for c in th.constraints do
      match c with
      | .unknown text =>
          unknown := unknown.push (th.name, text)
      | _ => pure ()
  if !unknown.isEmpty then
    let mut msg := "axi_constraints_ok_v1 refused: unknown/unsupported theory constraints found.\n"
    msg := msg ++ "Rewrite them into canonical structured forms (or use a `constraint Name:` named-block).\n"
    msg := msg ++ "Unknown constraints:\n"
    for i in List.range (Nat.min 8 unknown.size) do
      let (thName, text) := unknown[i]!
      msg := msg ++ s!"  {i}: theory `{thName}`: {text}\n"
    if unknown.size > 8 then
      msg := msg ++ s!"  ... ({unknown.size - 8} more)\n"
    throw msg.trimRight

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
      | .symmetric schema rel carriers params =>
          if schema == inst.schema then
            checkCount := checkCount + 1
            checkSymmetricCompatibleWithKeysAndFunctionals m constraints inst rel carriers params none #[]
      | .symmetricWhereIn schema rel field values carriers params =>
          if schema == inst.schema then
            checkCount := checkCount + 1
            checkSymmetricCompatibleWithKeysAndFunctionals m constraints inst rel carriers params (some field) values
      | .transitive schema rel carriers params =>
          if schema == inst.schema then
            checkCount := checkCount + 1
            checkTransitiveCompatibleWithKeysAndFunctionals m constraints inst rel carriers params
      | .typing schema rel rule =>
          if schema == inst.schema then
            checkCount := checkCount + 1
            checkTypingConstraint m inst rel rule

  pure {
    moduleName := m.moduleName
    constraintCount := constraints.size
    instanceCount := m.instances.size
    checkCount := checkCount
  }

end Axiograph.Axi.ConstraintsCheck
