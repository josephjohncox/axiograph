import Std
import Axiograph.Certificate.Format
import Axiograph.Axi.PathDBExportV1
import Axiograph.Axi.ConstraintsCheck
import Axiograph.Axi.TypeCheck
import Axiograph.Util.Fnv1a
import Mathlib.Computability.RegularExpressions

namespace Axiograph

namespace Reachability

structure ReachabilityResult where
  start : Nat
  end_ : Nat
  pathLen : Nat
  confidence : Float
  deriving Repr

def verifyReachabilityProof : ReachabilityProof → Except String ReachabilityResult
  | .reflexive entity =>
      pure { start := entity, end_ := entity, pathLen := 0, confidence := 1.0 }
  | .step src _relType dst relConfidence rest =>
      match ensureProb relConfidence with
      | .error msg => .error msg
      | .ok relConfidence =>
          match verifyReachabilityProof rest with
          | .error msg => .error msg
          | .ok restRes =>
              if restRes.start != dst then
                .error s!"invalid proof chain: expected rest.start = {dst}, got {restRes.start}"
              else
                .ok {
                  start := src,
                  end_ := restRes.end_,
                  pathLen := restRes.pathLen + 1,
                  confidence := relConfidence * restRes.confidence
                }

structure ReachabilityResultV2 where
  start : Nat
  end_ : Nat
  pathLen : Nat
  confidence : Prob.VProb
  deriving Repr

def verifyReachabilityProofV2 : ReachabilityProofV2 → Except String ReachabilityResultV2
  | .reflexive entity =>
      pure { start := entity, end_ := entity, pathLen := 0, confidence := Prob.vOne }
  | .step src _relType dst relConfidence _relationId? rest =>
      match verifyReachabilityProofV2 rest with
      | .error msg => .error msg
      | .ok restRes =>
          if restRes.start != dst then
            .error s!"invalid proof chain: expected rest.start = {dst}, got {restRes.start}"
          else
            .ok {
              start := src,
              end_ := restRes.end_,
              pathLen := restRes.pathLen + 1,
              confidence := Prob.vMult relConfidence restRes.confidence
            }

/-!
### Snapshot-scoped reachability checking (anchored to `.axi`)

`verifyReachabilityProofV2` checks the *internal* structure of a proof but is
intentionally independent of any particular graph/snapshot.

For end-to-end verification we also want the option to require that a reachability
witness only uses edges that exist in a canonical `.axi` snapshot (e.g. a
`PathDBExportV1` export).

This is the first step toward “query certificates anchored to canonical inputs”:

* certificates carry `relation_id` fact IDs,
* the verifier loads the snapshot and extracts `relation_info`,
* and we check every step references a real snapshot edge.
-/

open Axiograph.Axi.PathDBExportV1

def verifyReachabilityProofV2Anchored
    (relationInfo : Std.HashMap Nat RelationInfoRow) :
    ReachabilityProofV2 → Except String ReachabilityResultV2
  | .reflexive entity =>
      pure { start := entity, end_ := entity, pathLen := 0, confidence := Prob.vOne }
  | .step src relType dst relConfidence relationId? rest =>
      match relationId? with
      | none => .error "anchored reachability step is missing `relation_id`"
      | some rid =>
          match relationInfo.get? rid with
          | none =>
              .error s!"unknown relation_id {rid} (missing from snapshot relation_info)"
          | some row =>
              if row.source != src || row.target != dst then
                .error s!"relation_id {rid} endpoints mismatch: expected ({row.source},{row.target}), got ({src},{dst})"
              else if row.relTypeId != relType then
                .error s!"relation_id {rid} rel_type mismatch: expected {row.relTypeId}, got {relType}"
              else if Prob.toNat row.confidence != Prob.toNat relConfidence then
                .error s!"relation_id {rid} confidence mismatch: expected {Prob.toNat row.confidence}, got {Prob.toNat relConfidence}"
              else
                match verifyReachabilityProofV2Anchored relationInfo rest with
                | .error msg => .error msg
                | .ok restRes =>
                    if restRes.start != dst then
                      .error s!"invalid proof chain: expected rest.start = {dst}, got {restRes.start}"
                    else
                      .ok {
                        start := src,
                        end_ := restRes.end_,
                        pathLen := restRes.pathLen + 1,
                        confidence := Prob.vMult relConfidence restRes.confidence
                      }

end Reachability

namespace Resolution

structure ResolutionResultV2 where
  firstConfidence : Prob.VProb
  secondConfidence : Prob.VProb
  threshold : Prob.VProb
  decision : Prob.Resolution
  deriving Repr

def verifyResolutionProofV2 (proof : ResolutionProofV2) : Except String ResolutionResultV2 :=
  if Prob.decideResolution proof.firstConfidence proof.secondConfidence proof.threshold != proof.decision then
    .error
      s!"resolution decision mismatch: expected {reprStr (Prob.decideResolution proof.firstConfidence proof.secondConfidence proof.threshold)}, got {reprStr proof.decision}"
  else
    .ok {
      firstConfidence := proof.firstConfidence,
      secondConfidence := proof.secondConfidence,
      threshold := proof.threshold,
      decision := Prob.decideResolution proof.firstConfidence proof.secondConfidence proof.threshold
    }

end Resolution

namespace Query

/-!
## Certified query checking (anchored)

`query_result_v1` certificates are intended to support *conjunctive queries*
(AxQL / SQL-ish) in a “Rust computes, Lean verifies” pipeline.

Important: this verifier checks **soundness of the returned rows**, not completeness.
It proves “these rows satisfy the query”, not “these are all the satisfying rows”.

We also intentionally require a `PathDBExportV1` `.axi` anchor context:

* type constraints are checked against `entity_type`,
* attribute constraints are checked against `entity_attribute`,
* path witnesses are checked against `relation_info` using `relation_id` fact ids.
-/

open Axiograph.Axi.PathDBExportV1
open RegularExpression

structure QueryResultV1 where
  rowCount : Nat
  truncated : Bool
  deriving Repr

def resolveTerm (bindings : Std.HashMap String Nat) : QueryTermV1 → Except String Nat
  | .const entity => pure entity
  | .var name =>
      match bindings.get? name with
      | some entity => pure entity
      | none => throw s!"missing binding for variable `{name}`"

def toRegularExpression : QueryRegexV1 → RegularExpression Nat
  | .epsilon => (1 : RegularExpression Nat)
  | .rel relTypeId => RegularExpression.char relTypeId
  | .seq parts =>
      parts.foldl (fun acc p => acc * toRegularExpression p) (1 : RegularExpression Nat)
  | .alt parts =>
      parts.foldl (fun acc p => acc + toRegularExpression p) (0 : RegularExpression Nat)
  | .star inner =>
      RegularExpression.star (toRegularExpression inner)
  | .plus inner =>
      let re := toRegularExpression inner
      re * RegularExpression.star re
  | .opt inner =>
      (1 : RegularExpression Nat) + toRegularExpression inner

/-!
### Subtyping in anchored snapshots

When the anchor snapshot contains the meta-plane, Rust interprets type atoms as:

`?x : T`  means  “x has type T **or any subtype of T**”.

This matches typical query semantics (asking for `Agent` should include `Firm`,
`Household`, …). For legacy snapshot-anchored certificates we recover the
subtyping relation from meta-plane edges (`axi_subtype_of`) when present.

If no meta-plane subtype edges exist in the snapshot, we fall back to **exact**
type matching (subtype = supertype).
-/

structure SubtypeIndexV1 where
  /-- Subtyping adjacency (sub → immediate supertypes). -/
  supertypesOf : Std.HashMap Nat (Array Nat) := {}
  deriving Repr

def findInternedId? (internedString : Std.HashMap Nat String) (needle : String) : Option Nat :=
  Id.run do
    for (k, v) in internedString.toList do
      if v == needle then
        return some k
    return none

def buildSubtypeIndexV1
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (entityAttribute : Std.HashMap (Nat × Nat) Nat)
    (internedString : Std.HashMap Nat String) : SubtypeIndexV1 :=
  match findInternedId? internedString "axi_subtype_of",
        findInternedId? internedString "name" with
  | some subtypeRelTypeId, some nameKeyId =>
      Id.run do
        let mut out : Std.HashMap Nat (Array Nat) := {}
        for (_, row) in relationInfo.toList do
          if row.relTypeId == subtypeRelTypeId then
            match entityAttribute.get? (row.source, nameKeyId),
                  entityAttribute.get? (row.target, nameKeyId) with
            | some subNameId, some supNameId =>
                let current := out.getD subNameId #[]
                out := out.insert subNameId (current.push supNameId)
            | _, _ => pure ()
        pure { supertypesOf := out }
  | _, _ => {}

def isSubtypeV1Fuel (idx : SubtypeIndexV1) (fuel : Nat) (subType superType : Nat) (seen : Std.HashSet Nat) : Bool :=
  match fuel with
  | 0 => false
  | fuel + 1 =>
      if subType == superType then
        true
      else if seen.contains subType then
        false
      else
        let seen := seen.insert subType
        match idx.supertypesOf.get? subType with
        | none => false
        | some sups =>
            sups.any (fun next => isSubtypeV1Fuel idx fuel next superType seen)

def isSubtypeV1 (idx : SubtypeIndexV1) (subType superType : Nat) : Bool :=
  isSubtypeV1Fuel idx (idx.supertypesOf.size + 1) subType superType {}

def reachabilityRelTypes : ReachabilityProofV2 → List Nat
  | .reflexive _ => []
  | .step _ relType _ _ _ rest => relType :: reachabilityRelTypes rest

partial def ensureReachabilityMinConfidence
    (proof : ReachabilityProofV2)
    (minConfidence : Prob.VProb) : Except String Unit := do
  match proof with
  | .reflexive _ => pure ()
  | .step _ _ _ relConfidence _ rest => do
      if Prob.toNat relConfidence < Prob.toNat minConfidence then
        throw s!"reachability step below min_confidence_fp: got {Prob.toNat relConfidence}, expected ≥ {Prob.toNat minConfidence}"
      ensureReachabilityMinConfidence rest minConfidence

def verifyQueryRowV1Anchored
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (entityType : Std.HashMap Nat Nat)
    (entityAttribute : Std.HashMap (Nat × Nat) Nat)
    (subtypes : SubtypeIndexV1)
    (query : QueryV1)
    (row : QueryRowV1) : Except String Unit := do
  -- Build a binding map and reject duplicates (fail-closed).
  let mut bindings : Std.HashMap String Nat := {}
  for b in row.bindings do
    if bindings.contains b.var then
      throw s!"duplicate binding for variable `{b.var}`"
    bindings := bindings.insert b.var b.entity

  if row.witnesses.size != query.atoms.size then
    throw s!"witness count mismatch: expected {query.atoms.size}, got {row.witnesses.size}"

  for (atom, witness) in query.atoms.zip row.witnesses do
    match atom, witness with
    | .type term typeId, .type entity typeId' => do
        if typeId != typeId' then
          throw s!"type witness mismatch: expected type_id={typeId}, got {typeId'}"
        let entity' ← resolveTerm bindings term
        if entity != entity' then
          throw s!"type witness mismatch: expected entity={entity'}, got {entity}"
        let some actual := entityType.get? entity
          | throw s!"missing entity_type fact for entity {entity}"
        if !isSubtypeV1 subtypes actual typeId then
          throw s!"entity_type mismatch for entity {entity}: expected type_id={typeId} (allowing subtypes), got {actual}"

    | .attrEq term keyId valueId, .attrEq entity keyId' valueId' => do
        if keyId != keyId' || valueId != valueId' then
          throw s!"attr witness mismatch: expected (key_id={keyId}, value_id={valueId}), got (key_id={keyId'}, value_id={valueId'})"
        let entity' ← resolveTerm bindings term
        if entity != entity' then
          throw s!"attr witness mismatch: expected entity={entity'}, got {entity}"
        let some actual := entityAttribute.get? (entity, keyId)
          | throw s!"missing entity_attribute fact for entity {entity} and key_id {keyId}"
        if actual != valueId then
          throw s!"entity_attribute mismatch for entity {entity} and key_id {keyId}: expected value_id={valueId}, got {actual}"

    | .path left regex right, .path proof => do
        let src ← resolveTerm bindings left
        let dst ← resolveTerm bindings right

        let res ← Reachability.verifyReachabilityProofV2Anchored relationInfo proof
        if res.start != src then
          throw s!"path witness start mismatch: expected {src}, got {res.start}"
        if res.end_ != dst then
          throw s!"path witness end mismatch: expected {dst}, got {res.end_}"

        match query.maxHops? with
        | none => pure ()
        | some maxHops =>
            if res.pathLen > maxHops then
              throw s!"path witness exceeds max_hops={maxHops} (got len={res.pathLen})"

        match query.minConfidence? with
        | none => pure ()
        | some minConf => ensureReachabilityMinConfidence proof minConf

        let labels := reachabilityRelTypes proof
        if labels.length != res.pathLen then
          throw s!"internal error: relTypes length {labels.length} != pathLen {res.pathLen}"

        let re := toRegularExpression regex
        if !(labels ∈ re.matches') then
          throw s!"path witness labels do not match RPQ (labels={labels})"

    | _, _ =>
        throw "atom/witness kind mismatch"

def verifyQueryResultProofV1Anchored
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (entityType : Std.HashMap Nat Nat)
    (entityAttribute : Std.HashMap (Nat × Nat) Nat)
    (internedString : Std.HashMap Nat String)
    (proof : QueryResultProofV1) : Except String QueryResultV1 := do
  let subtypes := buildSubtypeIndexV1 relationInfo entityAttribute internedString
  for row in proof.rows do
    verifyQueryRowV1Anchored relationInfo entityType entityAttribute subtypes proof.query row
  pure { rowCount := proof.rows.size, truncated := proof.truncated }

structure QueryResultV2 where
  rowCount : Nat
  truncated : Bool
  deriving Repr

def verifyQueryRowV2Anchored
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (entityType : Std.HashMap Nat Nat)
    (entityAttribute : Std.HashMap (Nat × Nat) Nat)
    (subtypes : SubtypeIndexV1)
    (query : QueryV2)
    (row : QueryRowV2) : Except String Unit := do
  -- Build a binding map and reject duplicates (fail-closed).
  let mut bindings : Std.HashMap String Nat := {}
  for b in row.bindings do
    if bindings.contains b.var then
      throw s!"duplicate binding for variable `{b.var}`"
    bindings := bindings.insert b.var b.entity

  let mut chosen : Option (Array QueryAtomV1) := none
  let mut idx : Nat := 0
  for atoms in query.disjuncts do
    if idx == row.disjunct then
      chosen := some atoms
    idx := idx + 1

  let some atoms := chosen
    | throw s!"disjunct out of bounds: {row.disjunct} (have {query.disjuncts.size})"

  if row.witnesses.size != atoms.size then
    throw s!"witness count mismatch: expected {atoms.size}, got {row.witnesses.size}"

  for (atom, witness) in Array.zip atoms row.witnesses do
    match atom, witness with
    | .type term typeId, .type entity typeId' => do
        if typeId != typeId' then
          throw s!"type witness mismatch: expected type_id={typeId}, got {typeId'}"
        let entity' ← resolveTerm bindings term
        if entity != entity' then
          throw s!"type witness mismatch: expected entity={entity'}, got {entity}"
        let some actual := entityType.get? entity
          | throw s!"missing entity_type fact for entity {entity}"
        if !isSubtypeV1 subtypes actual typeId then
          throw s!"entity_type mismatch for entity {entity}: expected type_id={typeId} (allowing subtypes), got {actual}"

    | .attrEq term keyId valueId, .attrEq entity keyId' valueId' => do
        if keyId != keyId' || valueId != valueId' then
          throw s!"attr witness mismatch: expected (key_id={keyId}, value_id={valueId}), got (key_id={keyId'}, value_id={valueId'})"
        let entity' ← resolveTerm bindings term
        if entity != entity' then
          throw s!"attr witness mismatch: expected entity={entity'}, got {entity}"
        let some actual := entityAttribute.get? (entity, keyId)
          | throw s!"missing entity_attribute fact for entity {entity} and key_id {keyId}"
        if actual != valueId then
          throw s!"entity_attribute mismatch for entity {entity} and key_id {keyId}: expected value_id={valueId}, got {actual}"

    | .path left regex right, .path proof => do
        let src ← resolveTerm bindings left
        let dst ← resolveTerm bindings right

        let res ← Reachability.verifyReachabilityProofV2Anchored relationInfo proof
        if res.start != src then
          throw s!"path witness start mismatch: expected {src}, got {res.start}"
        if res.end_ != dst then
          throw s!"path witness end mismatch: expected {dst}, got {res.end_}"

        match query.maxHops? with
        | none => pure ()
        | some maxHops =>
            if res.pathLen > maxHops then
              throw s!"path witness exceeds max_hops={maxHops} (got len={res.pathLen})"

        match query.minConfidence? with
        | none => pure ()
        | some minConf => ensureReachabilityMinConfidence proof minConf

        let labels := reachabilityRelTypes proof
        if labels.length != res.pathLen then
          throw s!"internal error: relTypes length {labels.length} != pathLen {res.pathLen}"

        let re := toRegularExpression regex
        if !(labels ∈ re.matches') then
          throw s!"path witness labels do not match RPQ (labels={labels})"

    | _, _ =>
        throw "atom/witness kind mismatch"

def verifyQueryResultProofV2Anchored
    (relationInfo : Std.HashMap Nat RelationInfoRow)
    (entityType : Std.HashMap Nat Nat)
    (entityAttribute : Std.HashMap (Nat × Nat) Nat)
    (internedString : Std.HashMap Nat String)
    (proof : QueryResultProofV2) : Except String QueryResultV2 := do
  let subtypes := buildSubtypeIndexV1 relationInfo entityAttribute internedString
  for row in proof.rows do
    verifyQueryRowV2Anchored relationInfo entityType entityAttribute subtypes proof.query row
  pure { rowCount := proof.rows.size, truncated := proof.truncated }

/-!
## `.axi`-anchored query checking (v3, name-based)

`query_result_v3` removes the dependency on `PathDBExportV1` snapshot tables by
anchoring reachability witnesses directly to canonical `.axi` tuple facts via
`axi_fact_id`.

This checker:

* builds a small index over the anchored `.axi` module (objects + tuples),
* validates each witness step against the corresponding tuple's fields, and
* checks RPQ label matching via `Mathlib.Computability.RegularExpressions`.
-/

open Axiograph.Axi.SchemaV1

structure TupleFactInfoV3 where
  schemaName : String
  instanceName : String
  relationName : String
  fields : Std.HashMap String String
  deriving Repr

structure ObjectInfoV3 where
  schemaName : String
  instanceName : String
  objectType : String
  deriving Repr

structure AxiQueryIndexV3 where
  moduleName : String
  schemas : Std.HashMap String SchemaV1Schema
  tupleFacts : Std.HashMap String TupleFactInfoV3
  objects : Std.HashMap String ObjectInfoV3
  deriving Repr

def factIdPrefixV1 : String := Axiograph.Util.Fnv1a.factIdPrefix

def stripFactPrefixV1 (s : String) : Option String :=
  if s.startsWith factIdPrefixV1 then
    some (s.drop factIdPrefixV1.length)
  else
    none

def schemaMapV3 (m : Axiograph.Axi.AxiV1.AxiV1Module) : Std.HashMap String SchemaV1Schema :=
  Id.run do
    let mut out : Std.HashMap String SchemaV1Schema := {}
    for s in m.schemas do
      out := out.insert s.name s
    out

def findRelationDecl (schema : SchemaV1Schema) (relationName : String) : Except String RelationDeclV1 := do
  let some rel := schema.relations.find? (fun r => r.name == relationName)
    | throw s!"unknown relation `{relationName}` in schema `{schema.name}`"
  pure rel

def tupleEntityTypeName (schema : SchemaV1Schema) (relationName : String) : String :=
  if schema.objects.contains relationName then
    relationName ++ "Fact"
  else
    relationName

/-!
## Subtyping (schema-level)

When checking `query_result_v3` certificates, Rust treats a type atom `?x : T`
as satisfied when `?x` has type `U` and `U <: T` in the schema’s subtyping
closure (not only when `U = T`).

This helper mirrors that behavior so Lean accepts witnesses that rely on
subtyping.
-/

def isSubtypeInSchemaFuel
    (schema : SchemaV1Schema)
    (fuel : Nat)
    (subType : String)
    (superType : String)
    (seen : Std.HashSet String) : Bool :=
  match fuel with
  | 0 => false
  | fuel + 1 =>
      if subType == superType then
        true
      else if seen.contains subType then
        false
      else
        let seen := seen.insert subType
        schema.subtypes.any (fun st =>
          st.sub == subType && isSubtypeInSchemaFuel schema fuel st.sup superType seen)

def isSubtypeInSchema (schema : SchemaV1Schema) (subType superType : String) : Bool :=
  isSubtypeInSchemaFuel schema (schema.objects.size + 1) subType superType {}

def deriveBinaryEndpointsV3 (decl : RelationDeclV1) (fields : Std.HashMap String String) :
    Option (String × String) :=
  -- Mirrors `rust/crates/axiograph-pathdb/src/axi_module_import.rs::derive_binary_endpoints`.
  if decl.fields.size == 2 then
    let f0 := decl.fields[0]!.field
    let f1 := decl.fields[1]!.field
    match fields.get? f0, fields.get? f1 with
    | some a, some b => some (a, b)
    | _, _ => none
  else
    let primary : Array String :=
      decl.fields
        |>.map (fun f => f.field)
        |>.filter (fun f => f != "ctx" && f != "time")
    if primary.size == 2 then
      match fields.get? primary[0]!, fields.get? primary[1]! with
      | some a, some b => some (a, b)
      | _, _ => none
    else
      let pairs : List (String × String) :=
        [ ("lhs", "rhs")
        , ("route1", "route2")
        , ("path1", "path2")
        , ("rel1", "rel2")
        , ("i1", "i2")
        , ("s1", "s2")
        , ("left", "right")
        , ("child", "parent")
        , ("from", "to")
        , ("source", "target")
        , ("src", "dst")
        ]
      pairs.findSome? (fun (src, dst) =>
        match fields.get? src, fields.get? dst with
        | some a, some b => some (a, b)
        | _, _ => none)

def buildAxiQueryIndexV3 (m : Axiograph.Axi.AxiV1.AxiV1Module) : Except String AxiQueryIndexV3 := do
  let schemas := schemaMapV3 m
  let mut objects : Std.HashMap String ObjectInfoV3 := {}
  let mut tupleFacts : Std.HashMap String TupleFactInfoV3 := {}

  for inst in m.instances do
    let some schema := schemas.get? inst.schema
      | throw s!"instance `{inst.name}` references unknown schema `{inst.schema}`"

    for a in inst.assignments do
      -- Object assignment: `T = {x, y, ...}`
      -- Relation assignment: `R = {(field=v, ...), ...}`
      for item in a.value.items do
        match item with
        | .ident name =>
            if schema.objects.contains a.name then
              match objects.get? name with
              | none =>
                  objects :=
                    objects.insert name { schemaName := schema.name, instanceName := inst.name, objectType := a.name }
              | some prev =>
                  if prev.objectType != a.name || prev.schemaName != schema.name || prev.instanceName != inst.name then
                    throw s!"ambiguous object name `{name}` across assignments (expected unique names for query_result_v3)"
                  else
                    pure ()
            else
              -- Not an object assignment in this schema; ignore (fail-closed behavior for non-canonical inputs).
              pure ()
        | .tuple fieldPairs =>
            let relDecl ← findRelationDecl schema a.name
            let mut fm : Std.HashMap String String := {}
            for (k, v) in fieldPairs do
              if fm.contains k then
                throw s!"duplicate field `{k}` in `{a.name}` tuple (instance `{inst.name}`)"
              fm := fm.insert k v
            -- Ensure all declared fields are present (fail-closed).
            for f in relDecl.fields do
              if !(fm.contains f.field) then
                throw s!"missing field `{f.field}` in `{a.name}` tuple (instance `{inst.name}`)"
            -- Canonicalize in schema-declared field order.
            let mut ordered : Array (String × String) := #[]
            for f in relDecl.fields do
              let some v := fm.get? f.field
                | throw s!"internal error: missing field `{f.field}` after presence check"
              ordered := ordered.push (f.field, v)
            let factId :=
              Axiograph.Util.Fnv1a.axiFactIdV1 m.moduleName schema.name inst.name a.name ordered
            tupleFacts := tupleFacts.insert factId { schemaName := schema.name, instanceName := inst.name, relationName := a.name, fields := fm }

  pure { moduleName := m.moduleName, schemas, tupleFacts, objects }

def resolveTermV3 (bindings : Std.HashMap String String) : QueryTermV3 → Except String String
  | .const entity => pure entity
  | .var name =>
      match bindings.get? name with
      | some entity => pure entity
      | none => throw s!"missing binding for variable `{name}`"

def toRegularExpressionV3 : QueryRegexV3 → RegularExpression String
  | .epsilon => (1 : RegularExpression String)
  | .rel rel => RegularExpression.char rel
  | .seq parts =>
      parts.foldl (fun acc p => acc * toRegularExpressionV3 p) (1 : RegularExpression String)
  | .alt parts =>
      parts.foldl (fun acc p => acc + toRegularExpressionV3 p) (0 : RegularExpression String)
  | .star inner =>
      RegularExpression.star (toRegularExpressionV3 inner)
  | .plus inner =>
      let re := toRegularExpressionV3 inner
      re * RegularExpression.star re
  | .opt inner =>
      (1 : RegularExpression String) + toRegularExpressionV3 inner

def reachabilityRelLabelsV3 : ReachabilityProofV3 → List String
  | .reflexive _ => []
  | .step _ rel _ _ _ rest => rel :: reachabilityRelLabelsV3 rest

partial def ensureReachabilityMinConfidenceV3
    (proof : ReachabilityProofV3)
    (minConfidence : Prob.VProb) : Except String Unit := do
  match proof with
  | .reflexive _ => pure ()
  | .step _ _ _ relConfidence _ rest => do
      if Prob.toNat relConfidence < Prob.toNat minConfidence then
        throw s!"reachability step below min_confidence_fp: got {Prob.toNat relConfidence}, expected ≥ {Prob.toNat minConfidence}"
      ensureReachabilityMinConfidenceV3 rest minConfidence

structure ReachabilityResultV3 where
  start : String
  end_ : String
  pathLen : Nat
  confidence : Prob.VProb
  deriving Repr

partial def verifyReachabilityProofV3Anchored
    (index : AxiQueryIndexV3)
    (proof : ReachabilityProofV3) : Except String ReachabilityResultV3 := do
  match proof with
  | .reflexive entity =>
      pure { start := entity, end_ := entity, pathLen := 0, confidence := Prob.vOne }
  | .step src rel dst relConfidence axiFactId rest => do
      if Prob.toNat relConfidence != Prob.toNat Prob.vOne then
        throw s!"reachability_v3: confidence mismatch (expected 1.0, got {Prob.toNat relConfidence})"
      let some tuple := index.tupleFacts.get? axiFactId
        | throw s!"reachability_v3: unknown axi_fact_id `{axiFactId}`"

      let fieldRel := if rel == "axi_fact_in_context" then "ctx" else rel

      if src == axiFactId then
        -- Tuple-field edge: `factId -field-> value`
        let some v := tuple.fields.get? fieldRel
          | throw s!"reachability_v3: tuple `{axiFactId}` has no field `{fieldRel}`"
        if v != dst then
          throw s!"reachability_v3: field edge mismatch for `{axiFactId}`.{fieldRel}: expected `{v}`, got `{dst}`"
      else
        -- Derived binary edge: `src -Relation-> dst`
        if tuple.relationName != rel then
          throw s!"reachability_v3: relation mismatch for `{axiFactId}`: expected `{tuple.relationName}`, got `{rel}`"
        let some schema := index.schemas.get? tuple.schemaName
          | throw s!"reachability_v3: missing schema `{tuple.schemaName}` (internal index error)"
        let relDecl ← findRelationDecl schema tuple.relationName
        let some (expectedSrc, expectedDst) := deriveBinaryEndpointsV3 relDecl tuple.fields
          | throw s!"reachability_v3: relation `{tuple.relationName}` has no canonical binary projection"
        if expectedSrc != src || expectedDst != dst then
          throw s!"reachability_v3: binary endpoints mismatch for `{axiFactId}`: expected ({expectedSrc},{expectedDst}), got ({src},{dst})"

      let restRes ← verifyReachabilityProofV3Anchored index rest
      if restRes.start != dst then
        throw s!"invalid proof chain: expected rest.start = {dst}, got {restRes.start}"
      pure {
        start := src
        end_ := restRes.end_
        pathLen := restRes.pathLen + 1
        confidence := Prob.vMult relConfidence restRes.confidence
      }

def derivedAttrV3 (index : AxiQueryIndexV3) (entity : String) (key : String) :
    Except String (Option String) := do
  if entity.startsWith factIdPrefixV1 then
    let some tuple := index.tupleFacts.get? entity
      | throw s!"unknown tuple fact id `{entity}`"
    match key with
    | "name" =>
        let some hex := stripFactPrefixV1 entity
          | throw "internal error: fact prefix mismatch"
        pure (some (tuple.relationName ++ "_fact_" ++ hex))
    | "axi_fact_id" => pure (some entity)
    | "axi_module" => pure (some index.moduleName)
    | "axi_schema" => pure (some tuple.schemaName)
    | "axi_instance" => pure (some tuple.instanceName)
    | "axi_relation" => pure (some tuple.relationName)
    | _ => pure none
  else
    let some obj := index.objects.get? entity
      | throw s!"unknown object/entity name `{entity}`"
    match key with
    | "name" => pure (some entity)
    | "axi_module" => pure (some index.moduleName)
    | "axi_schema" => pure (some obj.schemaName)
    | "axi_instance" => pure (some obj.instanceName)
    | _ => pure none

structure QueryResultV3 where
  rowCount : Nat
  truncated : Bool
  deriving Repr

def verifyQueryRowV3Anchored
    (index : AxiQueryIndexV3)
    (query : QueryV3)
    (row : QueryRowV3) : Except String Unit := do
  let mut bindings : Std.HashMap String String := {}
  for b in row.bindings do
    if bindings.contains b.var then
      throw s!"duplicate binding for variable `{b.var}`"
    bindings := bindings.insert b.var b.entity

  let mut chosen : Option (Array QueryAtomV3) := none
  let mut idx : Nat := 0
  for atoms in query.disjuncts do
    if idx == row.disjunct then
      chosen := some atoms
    idx := idx + 1

  let some atoms := chosen
    | throw s!"disjunct out of bounds: {row.disjunct} (have {query.disjuncts.size})"

  if row.witnesses.size != atoms.size then
    throw s!"witness count mismatch: expected {atoms.size}, got {row.witnesses.size}"

  for (atom, witness) in Array.zip atoms row.witnesses do
    match atom, witness with
    | .type term typeName, .type entity typeName' => do
        if typeName != typeName' then
          throw s!"type witness mismatch: expected type_name={typeName}, got {typeName'}"
        let entity' ← resolveTermV3 bindings term
        if entity != entity' then
          throw s!"type witness mismatch: expected entity={entity'}, got {entity}"

        if entity.startsWith factIdPrefixV1 then
          let some tuple := index.tupleFacts.get? entity
            | throw s!"unknown tuple fact id `{entity}`"
          let some schema := index.schemas.get? tuple.schemaName
            | throw s!"missing schema `{tuple.schemaName}` (internal index error)"
          let expectedType := tupleEntityTypeName schema tuple.relationName
          if !isSubtypeInSchema schema expectedType typeName then
            throw s!"tuple type mismatch for `{entity}`: expected `{typeName}` (allowing subtypes), got `{expectedType}`"
        else
          let some obj := index.objects.get? entity
            | throw s!"unknown object/entity name `{entity}`"
          let some schema := index.schemas.get? obj.schemaName
            | throw s!"missing schema `{obj.schemaName}` (internal index error)"
          if !isSubtypeInSchema schema obj.objectType typeName then
            throw s!"object type mismatch for `{entity}`: expected `{typeName}` (allowing subtypes), got `{obj.objectType}`"

    | .attrEq term key value, .attrEq entity key' value' => do
        if key != key' || value != value' then
          throw s!"attr witness mismatch: expected (key={key}, value={value}), got (key={key'}, value={value'})"
        let entity' ← resolveTermV3 bindings term
        if entity != entity' then
          throw s!"attr witness mismatch: expected entity={entity'}, got {entity}"
        let actual? ← derivedAttrV3 index entity key
        match actual? with
        | none => throw s!"unknown/unsupported derived attribute `{key}` for entity `{entity}`"
        | some actual =>
            if actual != value then
              throw s!"derived attribute mismatch for `{entity}`.{key}: expected `{value}`, got `{actual}`"

    | .path left regex right, .path proof => do
        let src ← resolveTermV3 bindings left
        let dst ← resolveTermV3 bindings right

        let res ← verifyReachabilityProofV3Anchored index proof
        if res.start != src then
          throw s!"path witness start mismatch: expected {src}, got {res.start}"
        if res.end_ != dst then
          throw s!"path witness end mismatch: expected {dst}, got {res.end_}"

        match query.maxHops? with
        | none => pure ()
        | some maxHops =>
            if res.pathLen > maxHops then
              throw s!"path witness exceeds max_hops={maxHops} (got len={res.pathLen})"

        match query.minConfidence? with
        | none => pure ()
        | some minConf => ensureReachabilityMinConfidenceV3 proof minConf

        let labels := reachabilityRelLabelsV3 proof
        if labels.length != res.pathLen then
          throw s!"internal error: labels length {labels.length} != pathLen {res.pathLen}"

        let re := toRegularExpressionV3 regex
        if !(labels ∈ re.matches') then
          throw s!"path witness labels do not match RPQ (labels={labels})"

    | _, _ =>
        throw "atom/witness kind mismatch"

def verifyQueryResultProofV3Anchored
    (digestV1 : String)
    (module : Axiograph.Axi.AxiV1.AxiV1Module)
    (proof : QueryResultProofV3) : Except String QueryResultV3 := do
  let index ← buildAxiQueryIndexV3 module
  for row in proof.rows do
    verifyQueryRowV3Anchored index proof.query row
  -- Optional: verify elaboration rewrite derivations, if present.
  for rw in proof.elaborationRewrites do
    let _ ← RewriteDerivation.verifyRewriteDerivationProofV3Anchored digestV1 module rw
  pure { rowCount := proof.rows.size, truncated := proof.truncated }

end Query

namespace AxiWellTyped

/-!
## `.axi` well-typedness checking (anchored)

The `axi_well_typed_v1` certificate kind is a trusted gate for canonical inputs.
Lean re-parses and re-checks the anchored `.axi` module AST and compares the
certificate summary (counts) against a checker-computed summary.
-/

def toProofSummary (s : Axiograph.Axi.TypeCheck.TypeCheckSummaryV1) : AxiWellTypedProofV1 :=
  {
    moduleName := s.moduleName
    schemaCount := s.schemaCount
    theoryCount := s.theoryCount
    instanceCount := s.instanceCount
    assignmentCount := s.assignmentCount
    tupleCount := s.tupleCount
  }

def verifyAxiWellTypedProofV1Anchored
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (proof : AxiWellTypedProofV1) : Except String AxiWellTypedProofV1 := do
  let summary ← Axiograph.Axi.TypeCheck.typecheckModule m
  let expected := toProofSummary summary
  if expected != proof then
    throw s!"axi_well_typed_v1 summary mismatch: expected={reprStr expected}, got={reprStr proof}"
  pure expected

end AxiWellTyped

namespace AxiConstraintsOk

/-!
## `.axi` core-constraint checking (anchored)

The `axi_constraints_ok_v1` certificate kind is a pragmatic ontology-engineering
gate: it checks a conservative subset of theory constraints (keys/functionals)
on the anchored canonical `.axi` module.

Lean re-runs the checker and compares the summary payload (counts) so Rust/Lean
stay in lockstep.
-/

def toProofSummary (s : Axiograph.Axi.ConstraintsCheck.ConstraintsCheckSummaryV1) :
    AxiConstraintsOkProofV1 :=
  {
    moduleName := s.moduleName
    constraintCount := s.constraintCount
    instanceCount := s.instanceCount
    checkCount := s.checkCount
  }

def verifyAxiConstraintsOkProofV1Anchored
    (m : Axiograph.Axi.AxiV1.AxiV1Module)
    (proof : AxiConstraintsOkProofV1) : Except String AxiConstraintsOkProofV1 := do
  let summary ← Axiograph.Axi.ConstraintsCheck.checkModule m
  let expected := toProofSummary summary
  if expected != proof then
    throw s!"axi_constraints_ok_v1 summary mismatch: expected={reprStr expected}, got={reprStr proof}"
  pure expected

end AxiConstraintsOk

namespace PathNormalization

def PathExprV2.isReflexive : PathExprV2 → Bool
  | .reflexive _ => true
  | _ => false

/-!
### Canonical normalization (free-groupoid word reduction)

For §3 of `docs/explanation/BOOK.md` we want a normalization procedure that matches the
intended *free groupoid on generators* semantics:

* `reflexive` acts as the identity,
* `trans` is composition,
* `inv` is formal inversion,
* adjacent inverse atoms cancel.

We normalize by:

1. Flattening an expression into a list of **atoms** (`step` or `inv (step ...)`),
2. Reducing that list by canceling adjacent inverse pairs,
3. Rebuilding a right-associated `trans` chain from the reduced atom list.

This yields a deterministic, decidable normal form suitable for certificate
checking.
-/

def startEntity : PathExprV2 → Nat
  | .reflexive entity => entity
  | .step src _ _ => src
  | .trans left _ => startEntity left
  | .inv path => endEntity path
where
  endEntity : PathExprV2 → Nat
    | .reflexive entity => entity
    | .step _ _ dst => dst
    | .trans _ right => endEntity right
    | .inv path => startEntity path

def atomStartEntity : PathExprV2 → Option Nat
  | .step src _ _ => some src
  | .inv (.step _ _ dst) => some dst
  | _ => none

partial def endpoints : PathExprV2 → Except String (Nat × Nat)
  | .reflexive entity => pure (entity, entity)
  | .step src _ dst => pure (src, dst)
  | .trans left right => do
      let (ls, le) ← endpoints left
      let (rs, re) ← endpoints right
      if le != rs then
        throw s!"invalid trans endpoints: left.end={le} right.start={rs}"
      pure (ls, re)
  | .inv path => do
      let (s, e) ← endpoints path
      pure (e, s)

def invertAtom : PathExprV2 → PathExprV2
  | .step src relType dst => .inv (.step src relType dst)
  | .inv (.step src relType dst) => .step src relType dst
  | other => .inv other

def atomsAreInverse : PathExprV2 → PathExprV2 → Bool
  | .step s r t, .inv (.step s' r' t') => s == s' && r == r' && t == t'
  | .inv (.step s r t), .step s' r' t' => s == s' && r == r' && t == t'
  | _, _ => false

partial def flattenAtoms : PathExprV2 → List PathExprV2
  | .reflexive _ => []
  | .step src relType dst => [.step src relType dst]
  | .trans left right => flattenAtoms left ++ flattenAtoms right
  | .inv path =>
      (flattenAtoms path).reverse.map invertAtom

def reduceAtoms : List PathExprV2 → List PathExprV2
  | atoms =>
      let stepFn : List PathExprV2 → PathExprV2 → List PathExprV2 :=
        fun acc atom =>
          match acc with
          | prev :: rest =>
              if atomsAreInverse prev atom then
                rest
              else
                atom :: acc
          | [] => [atom]
      (atoms.foldl stepFn []).reverse

def buildFromAtoms (start : Nat) : List PathExprV2 → PathExprV2
  | [] => .reflexive start
  | [a] => a
  | a :: rest => .trans a (buildFromAtoms start rest)

partial def normalize : PathExprV2 → PathExprV2
  | expr =>
      let atoms := reduceAtoms (flattenAtoms expr)
      buildFromAtoms (startEntity expr) atoms

partial def isNormalized : PathExprV2 → Bool
  | expr =>
      normalize expr == expr

def isAtom : PathExprV2 → Bool
  | .step _ _ _ => true
  | .inv (.step _ _ _) => true
  | _ => false

def applyRule (rule : PathRewriteRuleV2) (expr : PathExprV2) : Except String PathExprV2 :=
  match rule, expr with
  | .idLeft, .trans (.reflexive _) p => .ok p
  | .idLeft, _ => .error "id_left: expected `trans (reflexive _) p`"

  | .idRight, .trans p (.reflexive _) => .ok p
  | .idRight, _ => .error "id_right: expected `trans p (reflexive _)`"

  | .assocRight, .trans (.trans p q) r =>
      .ok (.trans p (.trans q r))
  | .assocRight, _ => .error "assoc_right: expected `trans (trans p q) r`"

  | .invRefl, .inv (.reflexive a) => .ok (.reflexive a)
  | .invRefl, _ => .error "inv_refl: expected `inv (reflexive a)`"

  | .invInv, .inv (.inv p) => .ok p
  | .invInv, _ => .error "inv_inv: expected `inv (inv p)`"

  | .invTrans, .inv (.trans p q) =>
      .ok (.trans (.inv q) (.inv p))
  | .invTrans, _ => .error "inv_trans: expected `inv (trans p q)`"

  | .cancelHead, .trans a (.trans b rest) =>
      if isAtom a && isAtom b && atomsAreInverse a b then
        .ok rest
      else
        .error "cancel_head: expected `trans atom (trans invAtom rest)` with matching inverse atoms"
  | .cancelHead, .trans a b =>
      if isAtom a && isAtom b && atomsAreInverse a b then
        match atomStartEntity a with
        | some start => .ok (.reflexive start)
        | none => .error "cancel_head: internal error (expected atom start entity)"
      else
        .error "cancel_head: expected `trans atom invAtom` with matching inverse atoms"
  | .cancelHead, _ =>
      .error "cancel_head: expected `trans atom (trans invAtom rest)` or `trans atom invAtom`"

def applyAt (pos : List Nat) (rule : PathRewriteRuleV2) (expr : PathExprV2) : Except String PathExprV2 :=
  match pos, expr with
  | [], _ => applyRule rule expr
  | 0 :: rest, .trans left right =>
      match applyAt rest rule left with
      | .error msg => .error msg
      | .ok left' => .ok (.trans left' right)
  | 1 :: rest, .trans left right =>
      match applyAt rest rule right with
      | .error msg => .error msg
      | .ok right' => .ok (.trans left right')
  | 2 :: rest, .inv path =>
      match applyAt rest rule path with
      | .error msg => .error msg
      | .ok path' => .ok (.inv path')
  | _, _ =>
      .error s!"invalid rewrite position {pos} for expression node"

  /-- Replay a derivation step-by-step, refusing any step that changes endpoints. -/
  def runDerivationCore (start end_ : Nat) (current : PathExprV2) :
      List PathRewriteStepV2 → Except String PathExprV2
    | [] => .ok current
    | step :: rest =>
        match applyAt step.pos.toList step.rule current with
        | .error msg => .error msg
        | .ok next =>
            match endpoints next with
            | .error msg => .error msg
            | .ok (s, e) =>
                if s != start || e != end_ then
                  .error "rewrite step changed endpoints"
                else
                  runDerivationCore start end_ next rest

  /-- Replay a derivation from an input expression. -/
  def runDerivation (input : PathExprV2) (steps : Array PathRewriteStepV2) : Except String PathExprV2 :=
    match endpoints input with
    | .error msg => .error msg
    | .ok (start, end_) => runDerivationCore start end_ input steps.toList

structure NormalizePathResultV2 where
  start : Nat
  end_ : Nat
  normalized : PathExprV2
  deriving Repr

def verifyNormalizePathProofV2 (proof : NormalizePathProofV2) : Except String NormalizePathResultV2 := do
  match endpoints proof.input with
  | .error msg => .error msg
  | .ok (inputStart, inputEnd) =>
      match endpoints proof.normalized with
      | .error msg => .error msg
      | .ok (normStart, normEnd) =>
          if inputStart != normStart || inputEnd != normEnd then
            .error s!"normalized endpoints mismatch: input=({inputStart},{inputEnd}) normalized=({normStart},{normEnd})"
          else
            -- Optional explicit derivation replay.
            match proof.derivation? with
            | none => finish inputStart inputEnd
            | some steps =>
                match runDerivation proof.input steps with
                | .error msg => .error msg
                | .ok derived =>
                    if derived != proof.normalized then
                      .error "rewrite derivation does not produce the claimed normalized expression"
                    else
                      finish inputStart inputEnd
where
  finish (inputStart inputEnd : Nat) : Except String NormalizePathResultV2 :=
    let expected := normalize proof.input
    if expected != proof.normalized then
      .error "normalized path does not match the checker-computed normalization"
    else if !(isNormalized proof.normalized) then
      .error "normalized path is not in normalized form"
    else
      .ok { start := inputStart, end_ := inputEnd, normalized := proof.normalized }

end PathNormalization

namespace RewriteDerivation

open PathNormalization

structure RewriteDerivationResultV2 where
  start : Nat
  end_ : Nat
  output : PathExprV2
  deriving Repr

def verifyRewriteDerivationProofV2 (proof : RewriteDerivationProofV2) : Except String RewriteDerivationResultV2 := do
  match endpoints proof.input with
  | .error msg => .error msg
  | .ok (inputStart, inputEnd) =>
      match endpoints proof.output with
      | .error msg => .error msg
      | .ok (outStart, outEnd) =>
          if inputStart != outStart || inputEnd != outEnd then
            .error s!"rewrite_derivation: endpoints mismatch: input=({inputStart},{inputEnd}) output=({outStart},{outEnd})"
          else
            match runDerivation proof.input proof.derivation with
            | .error msg => .error msg
            | .ok derived =>
                if derived != proof.output then
                  .error "rewrite_derivation: derivation does not produce the claimed output expression"
                else
                  .ok { start := inputStart, end_ := inputEnd, output := proof.output }

/-!
### v3 rewrite derivations (`rewrite_derivation_v3`)

This is the `.axi`-anchored successor to `rewrite_derivation_v2`.

Key differences from v2:

* Expressions are **name-based** (`Axiograph.Axi.SchemaV1.PathExprV3`) rather than id-based.
* Steps reference either:
  - `builtin:<tag>` (groupoid normalization kernel), or
  - `axi:<axi_digest_v1>:<theory>:<rule_name>` (rules declared in canonical `.axi`).

The trusted checker replays the derivation step-by-step, resolving `axi:` rules
against the anchored `.axi` module (provided to `axiograph_verify`).
-/

open Axiograph.Axi.SchemaV1

structure RewriteDerivationResultV3 where
  start : String
  end_ : String
  output : PathExprV3
  deriving Repr

structure MatchEnv where
  pathSubst : Std.HashMap String PathExprV3 := {}
  entitySubst : Std.HashMap String String := {}
  deriving Repr

def declaredVars (rule : RewriteRuleV1) : (Std.HashSet String × Std.HashSet String) :=
  Id.run do
    let mut entityVars : Std.HashSet String := {}
    let mut pathVars : Std.HashSet String := {}
    for v in rule.vars do
      match v.ty with
      | .object _ => entityVars := entityVars.insert v.name
      | .path _ _ => pathVars := pathVars.insert v.name
    pure (entityVars, pathVars)

partial def endpointsV3 : PathExprV3 → Except String (String × String)
  | .var name => throw s!"endpoints: unexpected path metavariable `{name}`"
  | .reflexive entity => pure (entity, entity)
  | .step src _rel dst => pure (src, dst)
  | .trans left right => do
      let (ls, le) ← endpointsV3 left
      let (rs, re) ← endpointsV3 right
      if le != rs then
        throw s!"endpoints: trans mismatch (left ends at {le}, right starts at {rs})"
      pure (ls, re)
  | .inv path => do
      let (s, e) ← endpointsV3 path
      pure (e, s)

def isAtomV3 (e : PathExprV3) : Bool :=
  match e with
  | .step .. => true
  | .inv (.step ..) => true
  | _ => false

def atomsAreInverseV3 (left right : PathExprV3) : Bool :=
  match left, right with
  | .step a r b, .inv (.step a2 r2 b2) => a == a2 && r == r2 && b == b2
  | .inv (.step a r b), .step a2 r2 b2 => a == a2 && r == r2 && b == b2
  | _, _ => false

def atomStartV3 (atom : PathExprV3) : Option String :=
  match atom with
  | .step src _ _ => some src
  | .inv (.step _ _ dst) => some dst
  | _ => none

def applyBuiltinRuleV3 (rule : PathRewriteRuleV2) (expr : PathExprV3) : Except String PathExprV3 := do
  match rule with
  | .idLeft =>
      match expr with
      | .trans (.reflexive _) p => pure p
      | _ => throw "id_left: expected `trans (reflexive _) p`"
  | .idRight =>
      match expr with
      | .trans p (.reflexive _) => pure p
      | _ => throw "id_right: expected `trans p (reflexive _)`"
  | .assocRight =>
      match expr with
      | .trans (.trans p q) r => pure (.trans p (.trans q r))
      | _ => throw "assoc_right: expected `trans (trans p q) r`"
  | .invRefl =>
      match expr with
      | .inv (.reflexive a) => pure (.reflexive a)
      | _ => throw "inv_refl: expected `inv (reflexive a)`"
  | .invInv =>
      match expr with
      | .inv (.inv p) => pure p
      | _ => throw "inv_inv: expected `inv (inv p)`"
  | .invTrans =>
      match expr with
      | .inv (.trans p q) => pure (.trans (.inv q) (.inv p))
      | _ => throw "inv_trans: expected `inv (trans p q)`"
  | .cancelHead =>
      match expr with
      | .trans left right =>
          match right with
          | .trans middle rest =>
              if isAtomV3 left && isAtomV3 middle && atomsAreInverseV3 left middle then
                pure rest
              else
                throw "cancel_head: expected `trans atom (trans invAtom rest)` with matching inverse atoms"
          | _ =>
              if isAtomV3 left && isAtomV3 right && atomsAreInverseV3 left right then
                match atomStartV3 left with
                | some start => pure (.reflexive start)
                | none => throw "cancel_head: internal error (expected atom start entity)"
              else
                throw "cancel_head: expected `trans atom (trans invAtom rest)` or `trans atom invAtom`"
      | _ =>
          throw "cancel_head: expected `trans atom (trans invAtom rest)` or `trans atom invAtom`"

partial def applyAtBuiltinV3 (pos : List Nat) (rule : PathRewriteRuleV2) (expr : PathExprV3) :
    Except String PathExprV3 := do
  match pos, expr with
  | [], _ => applyBuiltinRuleV3 rule expr
  | 0 :: rest, .trans left right =>
      pure (.trans (← applyAtBuiltinV3 rest rule left) right)
  | 1 :: rest, .trans left right =>
      pure (.trans left (← applyAtBuiltinV3 rest rule right))
  | 2 :: rest, .inv path =>
      pure (.inv (← applyAtBuiltinV3 rest rule path))
  | head :: _, _ =>
      throw s!"invalid rewrite position head: {head}"

def matchEntity
    (entityVars : Std.HashSet String)
    (patternName : String)
    (targetName : String)
    (env : MatchEnv) : Except String MatchEnv := do
  if entityVars.contains patternName then
    match env.entitySubst.get? patternName with
    | some bound =>
        if bound == targetName then
          pure env
        else
          throw s!"entity var `{patternName}` mismatch: expected `{bound}` got `{targetName}`"
    | none =>
        pure { env with entitySubst := env.entitySubst.insert patternName targetName }
  else
    if patternName == targetName then
      pure env
    else
      throw s!"expected entity `{patternName}`, got `{targetName}`"

partial def matchExpr
    (entityVars : Std.HashSet String)
    (pathVars : Std.HashSet String)
    (pattern : PathExprV3)
    (target : PathExprV3)
    (env : MatchEnv) : Except String MatchEnv := do
  match pattern with
  | .var name =>
      if !pathVars.contains name then
        throw s!"unknown path metavariable `{name}` (declare it in `vars:` as `name: Path(x,y)`)"
      match env.pathSubst.get? name with
      | some bound =>
          if bound == target then
            pure env
          else
            throw s!"path var `{name}` mismatch"
      | none =>
          pure { env with pathSubst := env.pathSubst.insert name target }
  | .reflexive a =>
      match target with
      | .reflexive b => matchEntity entityVars a b env
      | _ => throw "match failure: expected reflexive"
  | .step a rel b =>
      match target with
      | .step a2 rel2 b2 =>
          if rel != rel2 then
            throw s!"match failure: expected rel `{rel}`, got `{rel2}`"
          let env ← matchEntity entityVars a a2 env
          matchEntity entityVars b b2 env
      | _ => throw "match failure: expected step"
  | .trans p q =>
      match target with
      | .trans p2 q2 =>
          let env ← matchExpr entityVars pathVars p p2 env
          matchExpr entityVars pathVars q q2 env
      | _ => throw "match failure: expected trans"
  | .inv p =>
      match target with
      | .inv p2 => matchExpr entityVars pathVars p p2 env
      | _ => throw "match failure: expected inv"

partial def substExpr
    (entityVars : Std.HashSet String)
    (pathVars : Std.HashSet String)
    (template : PathExprV3)
    (env : MatchEnv) : Except String PathExprV3 := do
  match template with
  | .var name =>
      if !pathVars.contains name then
        throw s!"unknown path metavariable `{name}` (declare it in `vars:` as `name: Path(x,y)`)"
      match env.pathSubst.get? name with
      | some e => pure e
      | none => throw s!"unbound path metavariable `{name}`"
  | .reflexive a =>
      if entityVars.contains a then
        match env.entitySubst.get? a with
        | some v => pure (.reflexive v)
        | none => throw s!"unbound entity variable `{a}`"
      else
        pure (.reflexive a)
  | .step a rel b =>
      let a :=
        if entityVars.contains a then
          env.entitySubst.get? a |>.getD a
        else
          a
      let b :=
        if entityVars.contains b then
          env.entitySubst.get? b |>.getD b
        else
          b
      pure (.step a rel b)
  | .trans p q =>
      pure (.trans (← substExpr entityVars pathVars p env) (← substExpr entityVars pathVars q env))
  | .inv p =>
      pure (.inv (← substExpr entityVars pathVars p env))

def applyAxiRuleOnce (rule : RewriteRuleV1) (expr : PathExprV3) : Except String PathExprV3 := do
  let (entityVars, pathVars) := declaredVars rule

  let tryDir (lhs rhs : PathExprV3) : Except String PathExprV3 := do
    let env ← matchExpr entityVars pathVars lhs expr {}
    let replaced ← substExpr entityVars pathVars rhs env
    let (s1, e1) ← endpointsV3 expr
    let (s2, e2) ← endpointsV3 replaced
    if s1 != s2 || e1 != e2 then
      throw s!"rewrite rule `{rule.name}` does not preserve endpoints"
    pure replaced

  match rule.orientation with
  | .forward => tryDir rule.lhs rule.rhs
  | .backward => tryDir rule.rhs rule.lhs
  | .bidirectional =>
      match (tryDir rule.lhs rule.rhs).toOption, (tryDir rule.rhs rule.lhs).toOption with
      | some out, none => pure out
      | none, some out => pure out
      | none, none => throw s!"rewrite rule `{rule.name}` does not apply"
      | some out1, some out2 =>
          if out1 == out2 then
            pure out1
          else
            throw s!"rewrite rule `{rule.name}` is ambiguous in bidirectional mode"

partial def applyAtAxiRule (pos : List Nat) (rule : RewriteRuleV1) (expr : PathExprV3) :
    Except String PathExprV3 := do
  match pos, expr with
  | [], _ => applyAxiRuleOnce rule expr
  | 0 :: rest, .trans left right =>
      pure (.trans (← applyAtAxiRule rest rule left) right)
  | 1 :: rest, .trans left right =>
      pure (.trans left (← applyAtAxiRule rest rule right))
  | 2 :: rest, .inv path =>
      pure (.inv (← applyAtAxiRule rest rule path))
  | head :: _, _ =>
      throw s!"invalid rewrite position head: {head}"

def lookupAxiRule (m : Axiograph.Axi.AxiV1.AxiV1Module) (theoryName ruleName : String) :
    Except String RewriteRuleV1 := do
  let some theory := m.theories.find? (fun t => t.name == theoryName)
    | throw s!"unknown theory `{theoryName}`"
  let some rule := theory.rewriteRules.find? (fun r => r.name == ruleName)
    | throw s!"unknown rewrite rule `{ruleName}` in theory `{theoryName}`"
  pure rule

def parseRuleRefV3 (ruleRef : String) :
    Except String (Sum PathRewriteRuleV2 (String × String × String)) := do
  if ruleRef.startsWith "builtin:" then
    let tag := ruleRef.drop "builtin:".length |>.trim
    pure (.inl (← PathRewriteRuleV2.parse tag))
  else if ruleRef.startsWith "axi:" then
    let parts := ruleRef.splitOn ":"
    match parts with
    | ["axi", "fnv1a64", hex, theoryName, ruleName] =>
        let digest := s!"fnv1a64:{hex}"
        pure (.inr (digest, theoryName, ruleName))
    | _ =>
        throw s!"invalid axi rule_ref: `{ruleRef}` (expected `axi:fnv1a64:<hex>:<theory>:<rule>`)"
  else
    throw s!"unknown rule_ref prefix (expected builtin: or axi:): `{ruleRef}`"

partial def runDerivationV3Unanchored (input : PathExprV3) (steps : Array PathRewriteStepV3) :
    Except String PathExprV3 := do
  let mut current := input
  for s in steps do
    match (← parseRuleRefV3 s.ruleRef) with
    | .inl builtinRule =>
        current ← applyAtBuiltinV3 s.pos.toList builtinRule current
    | .inr _axiRef =>
        throw "rewrite_derivation_v3: axi: rules require an `.axi` anchor context"
  pure current

partial def runDerivationV3Anchored
    (digestV1 : String)
    (module : Axiograph.Axi.AxiV1.AxiV1Module)
    (input : PathExprV3)
    (steps : Array PathRewriteStepV3) :
    Except String PathExprV3 := do
  let mut current := input
  for s in steps do
    match (← parseRuleRefV3 s.ruleRef) with
    | .inl builtinRule =>
        current ← applyAtBuiltinV3 s.pos.toList builtinRule current
    | .inr (d, theoryName, ruleName) =>
        if d != digestV1 then
          throw s!"rewrite_derivation_v3: rule digest mismatch (step references {d}, anchor is {digestV1})"
        let rule ← lookupAxiRule module theoryName ruleName
        current ← applyAtAxiRule s.pos.toList rule current
  pure current

def verifyRewriteDerivationProofV3 (proof : RewriteDerivationProofV3) :
    Except String RewriteDerivationResultV3 := do
  let (inputStart, inputEnd) ← endpointsV3 proof.input
  let (outStart, outEnd) ← endpointsV3 proof.output
  if inputStart != outStart || inputEnd != outEnd then
    throw s!"rewrite_derivation_v3: endpoints mismatch: input=({inputStart},{inputEnd}) output=({outStart},{outEnd})"
  let derived ← runDerivationV3Unanchored proof.input proof.derivation
  if derived != proof.output then
    throw "rewrite_derivation_v3: derivation does not produce the claimed output expression"
  pure { start := inputStart, end_ := inputEnd, output := proof.output }

def verifyRewriteDerivationProofV3Anchored
    (digestV1 : String)
    (module : Axiograph.Axi.AxiV1.AxiV1Module)
    (proof : RewriteDerivationProofV3) :
    Except String RewriteDerivationResultV3 := do
  let (inputStart, inputEnd) ← endpointsV3 proof.input
  let (outStart, outEnd) ← endpointsV3 proof.output
  if inputStart != outStart || inputEnd != outEnd then
    throw s!"rewrite_derivation_v3: endpoints mismatch: input=({inputStart},{inputEnd}) output=({outStart},{outEnd})"
  let derived ← runDerivationV3Anchored digestV1 module proof.input proof.derivation
  if derived != proof.output then
    throw "rewrite_derivation_v3: derivation does not produce the claimed output expression"
  pure { start := inputStart, end_ := inputEnd, output := proof.output }

end RewriteDerivation

namespace PathEquivalence

open PathNormalization

structure PathEquivResultV2 where
  start : Nat
  end_ : Nat
  normalized : PathExprV2
  deriving Repr

def verifyPathEquivProofV2 (proof : PathEquivProofV2) : Except String PathEquivResultV2 := do
  let (leftStart, leftEnd) ← endpoints proof.left
  let (rightStart, rightEnd) ← endpoints proof.right
  if leftStart != rightStart || leftEnd != rightEnd then
    throw s!"path_equiv: endpoint mismatch: left=({leftStart},{leftEnd}) right=({rightStart},{rightEnd})"

  let (normStart, normEnd) ← endpoints proof.normalized
  if leftStart != normStart || leftEnd != normEnd then
    throw s!"path_equiv: normalized endpoints mismatch: input=({leftStart},{leftEnd}) normalized=({normStart},{normEnd})"

  if let some steps := proof.leftDerivation? then
    let derived ← runDerivation proof.left steps
    if derived != proof.normalized then
      throw "path_equiv: left_derivation does not produce the claimed normalized expression"

  if let some steps := proof.rightDerivation? then
    let derived ← runDerivation proof.right steps
    if derived != proof.normalized then
      throw "path_equiv: right_derivation does not produce the claimed normalized expression"

  let expectedLeft := normalize proof.left
  let expectedRight := normalize proof.right
  if expectedLeft != proof.normalized then
    throw "path_equiv: left normalization does not match the claimed common normal form"
  if expectedRight != proof.normalized then
    throw "path_equiv: right normalization does not match the claimed common normal form"
  if !(isNormalized proof.normalized) then
    throw "path_equiv: claimed normal form is not normalized"

  pure { start := leftStart, end_ := leftEnd, normalized := proof.normalized }

end PathEquivalence

namespace Migration

def SchemaMorphismV1.objectImage (morphism : SchemaMorphismV1) (sourceObject : String) :
    Option String :=
  morphism.objects.find? (fun m => m.sourceObject == sourceObject) |>.map (·.targetObject)

def SchemaMorphismV1.arrowImage (morphism : SchemaMorphismV1) (sourceArrow : String) :
    Option (Array String) :=
  morphism.arrows.find? (fun m => m.sourceArrow == sourceArrow) |>.map (·.targetPath)

def InstanceV1.objectElements? (inst : InstanceV1) (objectName : String) : Option (Array String) :=
  inst.objects.find? (fun o => o.obj == objectName) |>.map (·.elems)

def InstanceV1.arrowEntry? (inst : InstanceV1) (arrowName : String) : Option ArrowMapEntryV1 :=
  inst.arrows.find? (fun a => a.arrow == arrowName)

def ArrowMapEntryV1.imageOf? (entry : ArrowMapEntryV1) (src : String) : Option String :=
  entry.pairs.find? (fun p => p.fst == src) |>.map (·.snd)

def applyArrowPath (inst : InstanceV1) (start : String) (path : Array String) :
    Except String String := do
  let mut current := start
  for arrowName in path do
    let some entry := inst.arrowEntry? arrowName
      | throw s!"missing arrow function for `{arrowName}`"
    let some next := entry.imageOf? current
      | throw s!"arrow `{arrowName}` missing mapping for input element `{current}`"
    current := next
  pure current

def deltaFCompute (proof : DeltaFMigrationProofV1) : Except String InstanceV1 := do
  if proof.morphism.sourceSchema != proof.sourceSchema.name then
    throw s!"delta_f: morphism.source_schema={proof.morphism.sourceSchema} does not match source_schema.name={proof.sourceSchema.name}"
  if proof.morphism.targetSchema != proof.targetInstance.schema then
    throw s!"delta_f: morphism.target_schema={proof.morphism.targetSchema} does not match target_instance.schema={proof.targetInstance.schema}"

  let mut outputObjects : Array ObjElemsV1 := #[]
  for sourceObject in proof.sourceSchema.objects do
    let some targetObject := proof.morphism.objectImage sourceObject
      | throw s!"delta_f: missing object mapping for source object `{sourceObject}`"
    let some elems := proof.targetInstance.objectElements? targetObject
      | throw s!"delta_f: target instance missing elements for mapped object `{targetObject}`"
    outputObjects := outputObjects.push { obj := sourceObject, elems := elems }

  let mut outputArrows : Array ArrowMapEntryV1 := #[]
  let mut seenArrowNames : Array String := #[]

  let arrowLike : Array (String × String × String) :=
    proof.sourceSchema.arrows.map (fun a => (a.name, a.src, a.dst)) ++
      proof.sourceSchema.subtypes.map (fun st => (st.incl, st.sub, st.sup))

  for (sourceArrowName, sourceSrcObject, sourceDstObject) in arrowLike do
    if seenArrowNames.contains sourceArrowName then
      throw s!"delta_f: duplicate arrow name `{sourceArrowName}` in source schema"
    seenArrowNames := seenArrowNames.push sourceArrowName

    let some targetSrcObject := proof.morphism.objectImage sourceSrcObject
      | throw s!"delta_f: missing object mapping for source object `{sourceSrcObject}`"
    let some targetDstObject := proof.morphism.objectImage sourceDstObject
      | throw s!"delta_f: missing object mapping for source object `{sourceDstObject}`"

    let some targetPath := proof.morphism.arrowImage sourceArrowName
      | throw s!"delta_f: missing arrow mapping for source arrow `{sourceArrowName}`"

    let some domainElems := proof.targetInstance.objectElements? targetSrcObject
      | throw s!"delta_f: target instance missing elements for mapped object `{targetSrcObject}`"
    let some codomainElems := proof.targetInstance.objectElements? targetDstObject
      | throw s!"delta_f: target instance missing elements for mapped object `{targetDstObject}`"

    if targetPath.isEmpty && targetSrcObject != targetDstObject then
      throw s!"delta_f: arrow `{sourceArrowName}` maps to identity path but object images differ ({targetSrcObject} ≠ {targetDstObject})"

    let mut pairs : Array (String × String) := #[]
    for domainElem in domainElems do
      let image ← applyArrowPath proof.targetInstance domainElem targetPath
      if !(codomainElems.contains image) then
        throw s!"delta_f: arrow `{sourceArrowName}` maps `{domainElem}` to `{image}`, but `{image}` is not in the codomain object `{targetDstObject}`"
      pairs := pairs.push (domainElem, image)

    outputArrows := outputArrows.push { arrow := sourceArrowName, pairs := pairs }

  pure {
    name := proof.targetInstance.name ++ "_delta_f"
    schema := proof.sourceSchema.name
    objects := outputObjects
    arrows := outputArrows
  }

structure DeltaFMigrationResultV1 where
  pulledBack : InstanceV1
  deriving Repr

def verifyDeltaFMigrationProofV1 (proof : DeltaFMigrationProofV1) :
    Except String DeltaFMigrationResultV1 := do
  let expected ← deltaFCompute proof
  if expected != proof.pulledBackInstance then
    throw "delta_f: pulled_back_instance does not match checker-computed Δ_F result"
  pure { pulledBack := expected }

end Migration

inductive CertificateResult where
  | reachabilityV1 (res : Reachability.ReachabilityResult)
  | reachabilityV2 (res : Reachability.ReachabilityResultV2)
  | resolutionV2 (res : Resolution.ResolutionResultV2)
  | axiWellTypedV1 (res : AxiWellTypedProofV1)
  | axiConstraintsOkV1 (res : AxiConstraintsOkProofV1)
  | queryResultV1 (res : Query.QueryResultV1)
  | queryResultV2 (res : Query.QueryResultV2)
  | queryResultV3 (res : Query.QueryResultV3)
  | normalizePathV2 (res : PathNormalization.NormalizePathResultV2)
  | rewriteDerivationV2 (res : RewriteDerivation.RewriteDerivationResultV2)
  | rewriteDerivationV3 (res : RewriteDerivation.RewriteDerivationResultV3)
  | pathEquivV2 (res : PathEquivalence.PathEquivResultV2)
  | deltaFV1 (res : Migration.DeltaFMigrationResultV1)
  deriving Repr

def verifyCertificate : Certificate → Except String CertificateResult
  | .reachabilityV1 proof => do
      let res ← Reachability.verifyReachabilityProof proof
      pure (.reachabilityV1 res)
  | .reachabilityV2 proof => do
      let res ← Reachability.verifyReachabilityProofV2 proof
      pure (.reachabilityV2 res)
  | .resolutionV2 proof => do
      let res ← Resolution.verifyResolutionProofV2 proof
      pure (.resolutionV2 res)
  | .axiWellTypedV1 _ =>
      throw "axi_well_typed_v1 requires a `.axi` anchor context; run `axiograph_verify <anchor.axi> <certificate.json>`"
  | .axiConstraintsOkV1 _ =>
      throw "axi_constraints_ok_v1 requires a `.axi` anchor context; run `axiograph_verify <anchor.axi> <certificate.json>`"
  | .queryResultV1 _ =>
      throw "query_result_v1 requires a `.axi` anchor context; run `axiograph_verify <anchor.axi> <certificate.json>`"
  | .queryResultV2 _ =>
      throw "query_result_v2 requires a `.axi` anchor context; run `axiograph_verify <anchor.axi> <certificate.json>`"
  | .queryResultV3 _ =>
      throw "query_result_v3 requires a `.axi` anchor context; run `axiograph_verify <anchor.axi> <certificate.json>`"
  | .normalizePathV2 proof => do
      let res ← PathNormalization.verifyNormalizePathProofV2 proof
      pure (.normalizePathV2 res)
  | .rewriteDerivationV2 proof => do
      let res ← RewriteDerivation.verifyRewriteDerivationProofV2 proof
      pure (.rewriteDerivationV2 res)
  | .rewriteDerivationV3 proof => do
      let res ← RewriteDerivation.verifyRewriteDerivationProofV3 proof
      pure (.rewriteDerivationV3 res)
  | .pathEquivV2 proof => do
      let res ← PathEquivalence.verifyPathEquivProofV2 proof
      pure (.pathEquivV2 res)
  | .deltaFV1 proof => do
      let res ← Migration.verifyDeltaFMigrationProofV1 proof
      pure (.deltaFV1 res)

end Axiograph
