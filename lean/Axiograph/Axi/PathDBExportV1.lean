import Std
import Axiograph.Axi.SchemaV1
import Axiograph.Prob.Verified

/-!
# `Axiograph.Axi.PathDBExportV1`

`PathDBExportV1` is the reversible `.axi` snapshot schema used to round-trip
PathDB state for auditability and verification:

* export: `.axpd → .axi` (schema `PathDBExportV1`)
* import: `.axi → .axpd`

For certificate anchoring we need to *interpret* just enough of a snapshot to
check that a certificate’s referenced fact IDs exist in the snapshot.

This module is intentionally minimal and focused:

* extract the `relation_info` table into a lookup map keyed by `Relation_<id>`,
* treat relation IDs and entity IDs as stable fact IDs within the snapshot.
-/

namespace Axiograph.Axi.PathDBExportV1

open Axiograph.Axi.SchemaV1
open Axiograph.Prob

def schemaName : String := "PathDBExportV1"
def instanceName : String := "SnapshotV1"

def prefixEntity : String := "Entity_"
def prefixRelation : String := "Relation_"
def prefixStringId : String := "StringId_"
def prefixF32Hex : String := "F32Hex_"

structure RelationInfoRow where
  relTypeId : Nat
  source : Nat
  target : Nat
  confidence : Prob.VProb
  deriving Repr, DecidableEq

def stripPrefix? (s : String) (prefixText : String) : Option String :=
  if s.startsWith prefixText then
    some (s.drop prefixText.length)
  else
    none

def parseNatToken (prefixText : String) (token : String) : Except String Nat := do
  let some rest := stripPrefix? token prefixText
    | throw s!"expected token prefix `{prefixText}`, got `{token}`"
  match rest.toNat? with
  | some n => pure n
  | none => throw s!"expected Nat after `{prefixText}`, got `{token}`"

def hexDigitValue? : Char → Option Nat
  | '0' => some 0
  | '1' => some 1
  | '2' => some 2
  | '3' => some 3
  | '4' => some 4
  | '5' => some 5
  | '6' => some 6
  | '7' => some 7
  | '8' => some 8
  | '9' => some 9
  | 'a' => some 10
  | 'b' => some 11
  | 'c' => some 12
  | 'd' => some 13
  | 'e' => some 14
  | 'f' => some 15
  | 'A' => some 10
  | 'B' => some 11
  | 'C' => some 12
  | 'D' => some 13
  | 'E' => some 14
  | 'F' => some 15
  | _ => none

def parseHexNat (hex : String) : Except String Nat := do
  let mut acc : Nat := 0
  for c in hex.toList do
    let some v := hexDigitValue? c
      | throw s!"invalid hex digit `{c}` in `{hex}`"
    acc := (acc * 16) + v
  pure acc

def pow2 (n : Nat) : Nat := Nat.pow 2 n

def bitsSlice (bits : Nat) (start : Nat) (width : Nat) : Nat :=
  (bits / pow2 start) % pow2 width

def roundDivPow2 (n : Nat) (k : Nat) : Nat :=
  match k with
  | 0 => n
  | k + 1 =>
      -- `round(n / 2^(k+1))` with ties rounded up (away from zero).
      (n + pow2 k) / pow2 (k + 1)

/-!
Convert an IEEE754 binary32 token (`F32Hex_...`) into the fixed-point probability
representation used by certificates (`Prob.VProb`).

Why do this in the snapshot interpreter?

* PathDB snapshots store confidences as **exact float bits** for round-tripping.
* Certificates store confidences as **fixed-point numerators** to avoid floats in
  the trusted checker.
* For anchored certificates we must ensure each witness step’s confidence matches
  the snapshot’s edge confidence (otherwise `min_confidence_fp` can be bypassed).
-/
def f32HexTokenToVProb (token : String) : Except String Prob.VProb := do
  let some hex := stripPrefix? token prefixF32Hex
    | throw s!"expected token prefix `{prefixF32Hex}`, got `{token}`"
  if hex.length != 8 then
    throw s!"expected 8 hex digits after `{prefixF32Hex}`, got `{hex}`"

  let bits := (← parseHexNat hex)
  -- We interpret the 32 bits using the IEEE754 binary32 layout.
  let sign := bitsSlice bits 31 1
  let exp := bitsSlice bits 23 8
  let frac := bitsSlice bits 0 23

  if sign == 1 then
    throw s!"confidence must be non-negative (got `{token}`)"

  -- Reject NaN/Inf early: PathDB should only export finite probabilities.
  if exp == 255 then
    if frac == 0 then
      throw s!"confidence must be finite (got +Inf `{token}`)"
    else
      throw s!"confidence must not be NaN (`{token}`)"

  -- Clamp to `[0, 1]` in fixed-point form.
  let scaled : Nat :=
    if exp == 0 then
      -- Subnormal (or zero):  value = frac * 2^(-149)
      -- Scale:               value * Precision = frac*Precision / 2^149
      roundDivPow2 (frac * Prob.Precision) 149
    else if exp >= 127 then
      -- `exp = 127` with `frac = 0` is exactly 1.0; anything larger clamps to 1.
      Prob.Precision
    else
      -- Normal: value = (2^23 + frac) * 2^(exp - 150)
      -- For `exp < 127`, this is strictly < 1.0, so we only need right shifts:
      --   value * Precision = (2^23 + frac)*Precision / 2^(150-exp)
      let mantissa := pow2 23 + frac
      let k := 150 - exp
      roundDivPow2 (mantissa * Prob.Precision) k

  let scaled := Nat.min scaled Prob.Precision
  let some p := Prob.fromFixedPoint scaled
    | throw s!"internal error: fixed-point numerator out of bounds (got {scaled})"
  pure p

def findInstance (m : SchemaV1Module) : Except String SchemaV1Instance := do
  let some inst := m.instances.find? (fun i => i.schema == schemaName ∧ i.name == instanceName)
    | throw s!"expected instance `{instanceName} of {schemaName}`"
  pure inst

def findAssignment (inst : SchemaV1Instance) (name : String) : Except String InstanceAssignmentV1 := do
  let some a := inst.assignments.find? (fun a => a.name == name)
    | throw s!"missing assignment `{name}` in PathDBExportV1 instance"
  pure a

def tupleField? (fields : Array (String × String)) (key : String) : Option String :=
  Id.run do
    for (k, v) in fields do
      if k == key then
        return some v
    return none

def requireTuple (item : SetItemV1) : Except String (Array (String × String)) := do
  match item with
  | .tuple fields => pure fields
  | .ident _ => throw "expected tuple item in PathDBExportV1 relation table"

def extractRelationInfo (m : SchemaV1Module) : Except String (Std.HashMap Nat RelationInfoRow) := do
  let inst ← findInstance m
  let a ← findAssignment inst "relation_info"

  let mut relInfo : Std.HashMap Nat RelationInfoRow := {}
  for item in a.value.items do
    let fields ← requireTuple item

    let some relTok := tupleField? fields "relation"
      | throw "relation_info tuple missing field `relation`"
    let some relTypeTok := tupleField? fields "rel_type_id"
      | throw "relation_info tuple missing field `rel_type_id`"
    let some srcTok := tupleField? fields "source"
      | throw "relation_info tuple missing field `source`"
    let some dstTok := tupleField? fields "target"
      | throw "relation_info tuple missing field `target`"
    let some confTok := tupleField? fields "confidence"
      | throw "relation_info tuple missing field `confidence`"

    let relId ← parseNatToken prefixRelation relTok
    let relTypeId ← parseNatToken prefixStringId relTypeTok
    let src ← parseNatToken prefixEntity srcTok
    let dst ← parseNatToken prefixEntity dstTok
    let conf ← f32HexTokenToVProb confTok

    if relInfo.contains relId then
      throw s!"duplicate relation_info row for `{prefixRelation}{relId}`"

    relInfo := relInfo.insert relId { relTypeId, source := src, target := dst, confidence := conf }

  pure relInfo

/-!
## Extracting entity facts (for query certificate anchoring)

For certified querying we need to interpret more of a snapshot than just
`relation_info`:

* `entity_type` witnesses type constraints (`?x : Type`)
* `entity_attribute` witnesses attribute constraints (`attr(?x, key, value)`)

We intentionally extract these tables into **simple lookup maps** keyed by the
snapshot’s stable ids (`Entity_<n>`, `StringId_<n>`).
-/

def extractEntityType (m : SchemaV1Module) : Except String (Std.HashMap Nat Nat) := do
  let inst ← findInstance m
  let a ← findAssignment inst "entity_type"

  let mut entityType : Std.HashMap Nat Nat := {}
  for item in a.value.items do
    let fields ← requireTuple item

    let some entityTok := tupleField? fields "entity"
      | throw "entity_type tuple missing field `entity`"
    let some typeTok := tupleField? fields "type_id"
      | throw "entity_type tuple missing field `type_id`"

    let entityId ← parseNatToken prefixEntity entityTok
    let typeId ← parseNatToken prefixStringId typeTok

    if entityType.contains entityId then
      throw s!"duplicate entity_type row for `{prefixEntity}{entityId}`"

    entityType := entityType.insert entityId typeId

  pure entityType

def extractEntityAttribute (m : SchemaV1Module) : Except String (Std.HashMap (Nat × Nat) Nat) := do
  let inst ← findInstance m
  let a ← findAssignment inst "entity_attribute"

  let mut entityAttr : Std.HashMap (Nat × Nat) Nat := {}
  for item in a.value.items do
    let fields ← requireTuple item

    let some entityTok := tupleField? fields "entity"
      | throw "entity_attribute tuple missing field `entity`"
    let some keyTok := tupleField? fields "key_id"
      | throw "entity_attribute tuple missing field `key_id`"
    let some valueTok := tupleField? fields "value_id"
      | throw "entity_attribute tuple missing field `value_id`"

    let entityId ← parseNatToken prefixEntity entityTok
    let keyId ← parseNatToken prefixStringId keyTok
    let valueId ← parseNatToken prefixStringId valueTok

    let k : (Nat × Nat) := (entityId, keyId)
    if entityAttr.contains k then
      throw s!"duplicate entity_attribute row for `{prefixEntity}{entityId}` and `{prefixStringId}{keyId}`"

    entityAttr := entityAttr.insert k valueId

  pure entityAttr

end Axiograph.Axi.PathDBExportV1
