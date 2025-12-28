import Lean
import Axiograph.Prob.Verified
import Axiograph.Axi.SchemaV1

namespace Axiograph

open Lean

namespace FixedPointProbability

/-!
We keep certificate parsing for probabilities in the *trusted checker* strictly
fixed-point, using `Axiograph.Prob.VProb` as the canonical representation.

This avoids floating-point arithmetic in the checker (certificates remain
deterministic and stable across platforms).
-/

def parseVProb (j : Json) : Except String Prob.VProb := do
  let n ← j.getNat?
  match Prob.fromFixedPoint n with
  | some p => pure p
  | none => throw s!"probability numerator must be ≤ {Prob.Precision} (got {n})"

end FixedPointProbability

inductive ReachabilityProof where
  | reflexive (entity : Nat)
  | step (src : Nat) (relType : Nat) (dst : Nat) (relConfidence : Float) (rest : ReachabilityProof)
  deriving Repr

def ReachabilityProof.start : ReachabilityProof → Nat
  | .reflexive entity => entity
  | .step src .. => src

def ReachabilityProof.end_ : ReachabilityProof → Nat
  | .reflexive entity => entity
  | .step _ _ _ _ rest => rest.end_

def ReachabilityProof.pathLen : ReachabilityProof → Nat
  | .reflexive _ => 0
  | .step _ _ _ _ rest => rest.pathLen + 1

def ReachabilityProof.confidence : ReachabilityProof → Float
  | .reflexive _ => 1.0
  | .step _ _ _ relConfidence rest => relConfidence * rest.confidence

def ensureProb (value : Float) : Except String Float := do
  if value.isNaN then
    throw "probability must not be NaN"
  if value.isInf then
    throw "probability must be finite"
  if value < 0.0 then
    throw s!"probability must be in [0, 1] (got {value})"
  if value > 1.0 then
    throw s!"probability must be in [0, 1] (got {value})"
  pure value

partial def parseReachabilityProof (j : Json) : Except String ReachabilityProof := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "reflexive" =>
      let entity ← (← j.getObjVal? "entity").getNat?
      pure (.reflexive entity)
  | "step" =>
      let src ← (← j.getObjVal? "from").getNat?
      let relType ← (← j.getObjVal? "rel_type").getNat?
      let dst ← (← j.getObjVal? "to").getNat?
      let relConfidence : Float ← fromJson? (← j.getObjVal? "rel_confidence")
      let relConfidence ← ensureProb relConfidence
      let rest ← parseReachabilityProof (← j.getObjVal? "rest")
      pure (.step src relType dst relConfidence rest)
  | other =>
      throw s!"unknown reachability proof type: {other}"

/-!
`ReachabilityProofV2` is a versioned variant that replaces `Float` confidences
with fixed-point verified probabilities (`Prob.VProb`).

It additionally supports an optional `relationId?` field on each step so query
certificates can be anchored to canonical `.axi` snapshots:

* for `PathDBExportV1`, `relationId? = some n` refers to the snapshot fact
  `Relation_<n>` in the `relation_info` table.
-/
inductive ReachabilityProofV2 where
  | reflexive (entity : Nat)
  | step
      (src : Nat)
      (relType : Nat)
      (dst : Nat)
      (relConfidence : Prob.VProb)
      (relationId? : Option Nat)
      (rest : ReachabilityProofV2)
  deriving Repr

def ReachabilityProofV2.start : ReachabilityProofV2 → Nat
  | .reflexive entity => entity
  | .step src .. => src

def ReachabilityProofV2.end_ : ReachabilityProofV2 → Nat
  | .reflexive entity => entity
  | .step _ _ _ _ _ rest => rest.end_

def ReachabilityProofV2.pathLen : ReachabilityProofV2 → Nat
  | .reflexive _ => 0
  | .step _ _ _ _ _ rest => rest.pathLen + 1

def ReachabilityProofV2.confidence : ReachabilityProofV2 → Prob.VProb
  | .reflexive _ => Prob.vOne
  | .step _ _ _ relConfidence _ rest => Prob.vMult relConfidence rest.confidence

partial def parseReachabilityProofV2 (j : Json) : Except String ReachabilityProofV2 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "reflexive" =>
      let entity ← (← j.getObjVal? "entity").getNat?
      pure (.reflexive entity)
  | "step" =>
      let src ← (← j.getObjVal? "from").getNat?
      let relType ← (← j.getObjVal? "rel_type").getNat?
      let dst ← (← j.getObjVal? "to").getNat?
      let relConfidence ← FixedPointProbability.parseVProb (← j.getObjVal? "rel_confidence_fp")
      let relationId? : Option Nat ←
        match (j.getObjVal? "relation_id").toOption with
        | none => pure none
        | some ridJson => pure (some (← ridJson.getNat?))
      let rest ← parseReachabilityProofV2 (← j.getObjVal? "rest")
      pure (.step src relType dst relConfidence relationId? rest)
  | other =>
      throw s!"unknown reachability proof type: {other}"

/-!
## v2: reconciliation / resolution decisions

This is a minimal certificate that lets Rust claim a conflict-resolution decision
and Lean re-compute it using `Axiograph.Prob.decideResolution`.
-/

structure ResolutionProofV2 where
  firstConfidence : Prob.VProb
  secondConfidence : Prob.VProb
  threshold : Prob.VProb
  decision : Prob.Resolution
  deriving Repr

def parseResolutionDecisionV2 (j : Json) : Except String Prob.Resolution := do
  let tag ← (← j.getObjVal? "tag").getStr?
  match tag with
  | "choose_first" => pure .chooseFirst
  | "choose_second" => pure .chooseSecond
  | "need_review" => pure .needReview
  | "merge" =>
      let w1 ← FixedPointProbability.parseVProb (← j.getObjVal? "w1_fp")
      let w2 ← FixedPointProbability.parseVProb (← j.getObjVal? "w2_fp")
      pure (.merge w1 w2)
  | other =>
      throw s!"unknown resolution decision tag: {other}"

partial def parseResolutionProofV2 (j : Json) : Except String ResolutionProofV2 := do
  let firstConfidence ← FixedPointProbability.parseVProb (← j.getObjVal? "first_confidence_fp")
  let secondConfidence ← FixedPointProbability.parseVProb (← j.getObjVal? "second_confidence_fp")
  let threshold ← FixedPointProbability.parseVProb (← j.getObjVal? "threshold_fp")
  let decision ← parseResolutionDecisionV2 (← j.getObjVal? "decision")
  pure { firstConfidence, secondConfidence, threshold, decision }

/-!
## v2: path normalization (groupoid rewrite certificates)

This certificate kind supports §3 of `docs/explanation/BOOK.md` (“paths, groupoids, and rewriting”):

* Rust provides an input path expression (`input`).
* Rust provides the normalized form (`normalized`).
* Rust may also provide an explicit rewrite derivation (`derivation`) as a list of
  `(rule, position)` steps.

Lean always re-computes normalization and checks the claimed result, and additionally
replays the explicit derivation when present.

The expression language is intentionally small and mirrors the Idris constructors
(`KGRefl`, `KGRel`, `KGTrans`), with an added formal inverse constructor (`inv`).
-/

inductive PathExprV2 where
  | reflexive (entity : Nat)
  | step (src : Nat) (relType : Nat) (dst : Nat)
  | trans (left : PathExprV2) (right : PathExprV2)
  | inv (path : PathExprV2)
  deriving Repr, DecidableEq

partial def parsePathExprV2 (j : Json) : Except String PathExprV2 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "reflexive" =>
      let entity ← (← j.getObjVal? "entity").getNat?
      pure (.reflexive entity)
  | "step" =>
      let src ← (← j.getObjVal? "from").getNat?
      let relType ← (← j.getObjVal? "rel_type").getNat?
      let dst ← (← j.getObjVal? "to").getNat?
      pure (.step src relType dst)
  | "trans" =>
      let left ← parsePathExprV2 (← j.getObjVal? "left")
      let right ← parsePathExprV2 (← j.getObjVal? "right")
      pure (.trans left right)
  | "inv" =>
      let path ← parsePathExprV2 (← j.getObjVal? "path")
      pure (.inv path)
  | other =>
      throw s!"unknown path expression type: {other}"

/-!
### v2 rewrite steps (explicit derivations)

To support §3.3/§3.4 of `docs/explanation/BOOK.md`, normalization certificates can optionally
carry an *explicit rewrite derivation*:

* `rule` identifies the local rewrite rule,
* `pos` identifies **where** in the AST to apply it (congruence closure).

Positions are a path from the root to a subexpression:

* `0` = `.trans.left`
* `1` = `.trans.right`
* `2` = `.inv.path`
-/

inductive PathRewriteRuleV2 where
  | assocRight
  | idLeft
  | idRight
  | invRefl
  | invInv
  | invTrans
  | cancelHead
  deriving Repr, DecidableEq

def PathRewriteRuleV2.parse (s : String) : Except String PathRewriteRuleV2 := do
  match s with
  | "assoc_right" => pure .assocRight
  | "id_left" => pure .idLeft
  | "id_right" => pure .idRight
  | "inv_refl" => pure .invRefl
  | "inv_inv" => pure .invInv
  | "inv_trans" => pure .invTrans
  | "cancel_head" => pure .cancelHead
  | other => throw s!"unknown rewrite rule tag: {other}"

structure PathRewriteStepV2 where
  pos : Array Nat
  rule : PathRewriteRuleV2
  deriving Repr, DecidableEq

partial def parsePathRewriteStepV2 (j : Json) : Except String PathRewriteStepV2 := do
  let ruleTag ← (← j.getObjVal? "rule").getStr?
  let rule ← PathRewriteRuleV2.parse ruleTag
  let posJson ← j.getObjVal? "pos"
  let posArr ← posJson.getArr?
  let mut pos : Array Nat := #[]
  for p in posArr do
    pos := pos.push (← p.getNat?)
  pure { pos, rule }

structure NormalizePathProofV2 where
  input : PathExprV2
  normalized : PathExprV2
  /--
  Optional explicit rewrite derivation.

  When present, Lean can validate that `normalized` is reachable from `input`
  by applying the listed rewrite steps (congruence-aware via positions).

  When absent, Lean falls back to the original “recompute normalization and
  compare” behavior for backwards compatibility.
  -/
  derivation? : Option (Array PathRewriteStepV2)
  deriving Repr

partial def parseNormalizePathProofV2 (j : Json) : Except String NormalizePathProofV2 := do
  let input ← parsePathExprV2 (← j.getObjVal? "input")
  let normalized ← parsePathExprV2 (← j.getObjVal? "normalized")
  let derivation? : Option (Array PathRewriteStepV2) ←
    match (j.getObjVal? "derivation").toOption with
    | none => pure none
    | some d => do
        let arr ← d.getArr?
        let mut steps : Array PathRewriteStepV2 := #[]
        for s in arr do
          steps := steps.push (← parsePathRewriteStepV2 s)
        pure (some steps)
  pure { input, normalized, derivation? }

/-!
## v2: replayable rewrite derivations

This certificate kind generalizes the “rule + position” proof pattern used in
`normalize_path_v2`:

* provide an `input` expression,
* provide an `output` expression,
* provide a `derivation` (a list of rewrite steps to replay).

This is intended to be the common format for:

* domain rewrites (unit conversions, schema rewrites, etc.),
* reconciliation explanations (why two statements were merged/rewritten),
* and optimization traces (e-graph extractions, normalization passes).

For now the rule vocabulary is the groupoid/path rewrite rules (`PathRewriteRuleV2`).
Domain-specific rule vocabularies should be added as new, versioned kinds on top.
-/

structure RewriteDerivationProofV2 where
  input : PathExprV2
  output : PathExprV2
  derivation : Array PathRewriteStepV2
  deriving Repr

partial def parseRewriteDerivationProofV2 (j : Json) : Except String RewriteDerivationProofV2 := do
  let input ← parsePathExprV2 (← j.getObjVal? "input")
  let output ← parsePathExprV2 (← j.getObjVal? "output")
  let stepsJson ← (← j.getObjVal? "derivation").getArr?
  let mut steps : Array PathRewriteStepV2 := #[]
  for s in stepsJson do
    steps := steps.push (← parsePathRewriteStepV2 s)
  pure { input, output, derivation := steps }

/-!
## v3: rewrite derivations with first-class rule references

`rewrite_derivation_v2` uses a *fixed enum* (`PathRewriteRuleV2`) for rewrite rules
(the groupoid normalization kernel).

For ontology/domain semantics, we want **first-class rules**:

* rules are declared in canonical `.axi` theories,
* imported into PathDB's meta-plane, and
* referenced by certificates via a stable `(module_digest, theory, rule)` key.

This certificate kind keeps the replayable “rule + position” idea from v2 but
replaces the rule enum with a `rule_ref` string:

* `builtin:<tag>` where `<tag>` is a v2 builtin like `id_left`
* `axi:<axi_digest_v1>:<theory_name>:<rule_name>`

The trusted checker resolves `axi:...` rule refs against the anchored `.axi`
module provided to `axiograph_verify`.
-/

open Axiograph.Axi.SchemaV1

partial def parsePathExprV3 (j : Json) : Except String PathExprV3 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "var" =>
      let name ← (← j.getObjVal? "name").getStr?
      pure (.var name)
  | "reflexive" =>
      let entity ← (← j.getObjVal? "entity").getStr?
      pure (.reflexive entity)
  | "step" =>
      let src ← (← j.getObjVal? "from").getStr?
      let rel ← (← j.getObjVal? "rel").getStr?
      let dst ← (← j.getObjVal? "to").getStr?
      pure (.step src rel dst)
  | "trans" =>
      let left ← parsePathExprV3 (← j.getObjVal? "left")
      let right ← parsePathExprV3 (← j.getObjVal? "right")
      pure (.trans left right)
  | "inv" =>
      let path ← parsePathExprV3 (← j.getObjVal? "path")
      pure (.inv path)
  | other =>
      throw s!"unknown path expression type (v3): {other}"

structure PathRewriteStepV3 where
  pos : Array Nat
  ruleRef : String
  deriving Repr, DecidableEq

partial def parsePathRewriteStepV3 (j : Json) : Except String PathRewriteStepV3 := do
  let ruleRef ← (← j.getObjVal? "rule_ref").getStr?
  let posJson ← j.getObjVal? "pos"
  let posArr ← posJson.getArr?
  let mut pos : Array Nat := #[]
  for p in posArr do
    pos := pos.push (← p.getNat?)
  pure { pos, ruleRef }

structure RewriteDerivationProofV3 where
  input : PathExprV3
  output : PathExprV3
  derivation : Array PathRewriteStepV3
  deriving Repr

partial def parseRewriteDerivationProofV3 (j : Json) : Except String RewriteDerivationProofV3 := do
  let input ← parsePathExprV3 (← j.getObjVal? "input")
  let output ← parsePathExprV3 (← j.getObjVal? "output")
  let stepsJson ← (← j.getObjVal? "derivation").getArr?
  let mut steps : Array PathRewriteStepV3 := #[]
  for s in stepsJson do
    steps := steps.push (← parsePathRewriteStepV3 s)
  pure { input, output, derivation := steps }

/-!
## v2: path equivalence (groupoid rewrite / normalization)

This certificate kind is a reusable building block for §3 of `docs/explanation/BOOK.md`:

* Two path expressions are considered equivalent if they normalize to the same
  normal form.
* Rust may optionally attach explicit rewrite derivations showing:
  - `left  ↦ normalized`
  - `right ↦ normalized`

This shape is intentionally redundant at first: the trusted checker always
recomputes normalization, but derivations are useful to:

* audit *why* two derivations are equivalent,
* reuse the same mechanism for domain rewrites and reconciliation explanations,
* and eventually reduce trust in the normalization implementation itself (by
  proving rule soundness against mathlib’s free-groupoid denotation).
-/

structure PathEquivProofV2 where
  left : PathExprV2
  right : PathExprV2
  normalized : PathExprV2
  leftDerivation? : Option (Array PathRewriteStepV2)
  rightDerivation? : Option (Array PathRewriteStepV2)
  deriving Repr

partial def parsePathEquivProofV2 (j : Json) : Except String PathEquivProofV2 := do
  let left ← parsePathExprV2 (← j.getObjVal? "left")
  let right ← parsePathExprV2 (← j.getObjVal? "right")
  let normalized ← parsePathExprV2 (← j.getObjVal? "normalized")

  let leftDerivation? : Option (Array PathRewriteStepV2) ←
    match (j.getObjVal? "left_derivation").toOption with
    | none => pure none
    | some d => do
        let arr ← d.getArr?
        let mut steps : Array PathRewriteStepV2 := #[]
        for s in arr do
          steps := steps.push (← parsePathRewriteStepV2 s)
        pure (some steps)

  let rightDerivation? : Option (Array PathRewriteStepV2) ←
    match (j.getObjVal? "right_derivation").toOption with
    | none => pure none
    | some d => do
        let arr ← d.getArr?
        let mut steps : Array PathRewriteStepV2 := #[]
        for s in arr do
          steps := steps.push (← parsePathRewriteStepV2 s)
        pure (some steps)

  pure { left, right, normalized, leftDerivation?, rightDerivation? }

/-!
## v2: functorial data migration (Δ_F / pullback)

Appendix C of `docs/explanation/BOOK.md` highlights categorical databases / functorial data migration
as a key source of “best practice” semantics for schema evolution.

This certificate kind is the first step:

* Rust computes `Δ_F(I)` for a schema morphism `F` and target instance `I`.
* Rust emits a certificate containing:
  - the morphism `F`,
  - the source schema,
  - the target instance,
  - and the claimed pulled-back instance.
* Lean recomputes `Δ_F(I)` and checks it matches the claimed result.

The early checker is intentionally “recompute and compare”; later tightening can:

* move schemas/instances to `.axi`-anchored hashes,
* add explicit derivations/normal forms, and
* relate the implementation to a mathlib-based category semantics.
-/

namespace Migration

abbrev Name : Type := String

structure ObjectMappingV1 where
  sourceObject : Name
  targetObject : Name
  deriving Repr, DecidableEq

structure ArrowMappingV1 where
  sourceArrow : Name
  targetPath : Array Name
  deriving Repr, DecidableEq

structure SchemaMorphismV1 where
  sourceSchema : Name
  targetSchema : Name
  objects : Array ObjectMappingV1
  arrows : Array ArrowMappingV1
  deriving Repr, DecidableEq

structure ArrowDeclV1 where
  name : Name
  src : Name
  dst : Name
  deriving Repr, DecidableEq

structure SubtypeDeclV1 where
  sub : Name
  sup : Name
  incl : Name
  deriving Repr, DecidableEq

structure SchemaV1 where
  name : Name
  objects : Array Name
  arrows : Array ArrowDeclV1
  subtypes : Array SubtypeDeclV1
  deriving Repr, DecidableEq

structure ObjElemsV1 where
  obj : Name
  elems : Array Name
  deriving Repr, DecidableEq

structure ArrowMapEntryV1 where
  arrow : Name
  pairs : Array (Name × Name)
  deriving Repr, DecidableEq

structure InstanceV1 where
  name : Name
  schema : Name
  objects : Array ObjElemsV1
  arrows : Array ArrowMapEntryV1
  deriving Repr, DecidableEq

structure DeltaFMigrationProofV1 where
  morphism : SchemaMorphismV1
  sourceSchema : SchemaV1
  targetInstance : InstanceV1
  pulledBackInstance : InstanceV1
  deriving Repr

def parseObjectMappingV1 (j : Json) : Except String ObjectMappingV1 := do
  let sourceObject ← (← j.getObjVal? "source_object").getStr?
  let targetObject ← (← j.getObjVal? "target_object").getStr?
  pure { sourceObject, targetObject }

def parseArrowMappingV1 (j : Json) : Except String ArrowMappingV1 := do
  let sourceArrow ← (← j.getObjVal? "source_arrow").getStr?
  let pathJson ← (← j.getObjVal? "target_path").getArr?
  let mut targetPath : Array Name := #[]
  for p in pathJson do
    targetPath := targetPath.push (← p.getStr?)
  pure { sourceArrow, targetPath }

def parseSchemaMorphismV1 (j : Json) : Except String SchemaMorphismV1 := do
  let sourceSchema ← (← j.getObjVal? "source_schema").getStr?
  let targetSchema ← (← j.getObjVal? "target_schema").getStr?

  let objectsJson ← (← j.getObjVal? "objects").getArr?
  let mut objects : Array ObjectMappingV1 := #[]
  for o in objectsJson do
    objects := objects.push (← parseObjectMappingV1 o)

  let arrowsJson ← (← j.getObjVal? "arrows").getArr?
  let mut arrows : Array ArrowMappingV1 := #[]
  for a in arrowsJson do
    arrows := arrows.push (← parseArrowMappingV1 a)

  pure { sourceSchema, targetSchema, objects, arrows }

def parseArrowDeclV1 (j : Json) : Except String ArrowDeclV1 := do
  let name ← (← j.getObjVal? "name").getStr?
  let src ← (← j.getObjVal? "src").getStr?
  let dst ← (← j.getObjVal? "dst").getStr?
  pure { name, src, dst }

def parseSubtypeDeclV1 (j : Json) : Except String SubtypeDeclV1 := do
  let sub ← (← j.getObjVal? "sub").getStr?
  let sup ← (← j.getObjVal? "sup").getStr?
  let incl ← (← j.getObjVal? "incl").getStr?
  pure { sub, sup, incl }

def parseSchemaV1 (j : Json) : Except String SchemaV1 := do
  let name ← (← j.getObjVal? "name").getStr?

  let objectsJson ← (← j.getObjVal? "objects").getArr?
  let mut objects : Array Name := #[]
  for o in objectsJson do
    objects := objects.push (← o.getStr?)

  let arrowsJson ← (← j.getObjVal? "arrows").getArr?
  let mut arrows : Array ArrowDeclV1 := #[]
  for a in arrowsJson do
    arrows := arrows.push (← parseArrowDeclV1 a)

  let subtypesJson ← (← j.getObjVal? "subtypes").getArr?
  let mut subtypes : Array SubtypeDeclV1 := #[]
  for s in subtypesJson do
    subtypes := subtypes.push (← parseSubtypeDeclV1 s)

  pure { name, objects, arrows, subtypes }

def parseObjElemsV1 (j : Json) : Except String ObjElemsV1 := do
  let obj ← (← j.getObjVal? "obj").getStr?
  let elemsJson ← (← j.getObjVal? "elems").getArr?
  let mut elems : Array Name := #[]
  for e in elemsJson do
    elems := elems.push (← e.getStr?)
  pure { obj, elems }

def parseStringPair (j : Json) : Except String (Name × Name) := do
  let arr ← j.getArr?
  if arr.size != 2 then
    throw s!"expected a pair array of length 2 (got {arr.size})"
  match arr.toList with
  | [a, b] => pure (← a.getStr?, ← b.getStr?)
  | _ => throw "internal error: pair array size check failed"

def parseArrowMapEntryV1 (j : Json) : Except String ArrowMapEntryV1 := do
  let arrow ← (← j.getObjVal? "arrow").getStr?
  let pairsJson ← (← j.getObjVal? "pairs").getArr?
  let mut pairs : Array (Name × Name) := #[]
  for p in pairsJson do
    pairs := pairs.push (← parseStringPair p)
  pure { arrow, pairs }

def parseInstanceV1 (j : Json) : Except String InstanceV1 := do
  let name ← (← j.getObjVal? "name").getStr?
  let schema ← (← j.getObjVal? "schema").getStr?

  let objectsJson ← (← j.getObjVal? "objects").getArr?
  let mut objects : Array ObjElemsV1 := #[]
  for o in objectsJson do
    objects := objects.push (← parseObjElemsV1 o)

  let arrowsJson ← (← j.getObjVal? "arrows").getArr?
  let mut arrows : Array ArrowMapEntryV1 := #[]
  for a in arrowsJson do
    arrows := arrows.push (← parseArrowMapEntryV1 a)

  pure { name, schema, objects, arrows }

def parseDeltaFMigrationProofV1 (j : Json) : Except String DeltaFMigrationProofV1 := do
  let morphism ← parseSchemaMorphismV1 (← j.getObjVal? "morphism")
  let sourceSchema ← parseSchemaV1 (← j.getObjVal? "source_schema")
  let targetInstance ← parseInstanceV1 (← j.getObjVal? "target_instance")
  let pulledBackInstance ← parseInstanceV1 (← j.getObjVal? "pulled_back_instance")
  pure { morphism, sourceSchema, targetInstance, pulledBackInstance }

end Migration

/-!
## Query result certificates (conjunctive queries)

To support “Rust computes, Lean verifies” for *queries* (AxQL / SQL-ish), we
introduce a small **core query IR** intended for certificates.

Key idea:

* a query is a conjunction of atoms (type, attribute, and path constraints),
* a certificate provides, for each returned row, witnesses that each atom holds,
  anchored to a canonical `.axi` snapshot (via `relation_id` fact ids).

This is the “datalog-ish / conjunctive query” kernel that other surfaces compile
into.
-/

inductive QueryTermV1 where
  | var (name : String)
  | const (entity : Nat)
  deriving Repr

inductive QueryRegexV1 where
  | epsilon
  | rel (relTypeId : Nat)
  | seq (parts : Array QueryRegexV1)
  | alt (parts : Array QueryRegexV1)
  | star (inner : QueryRegexV1)
  | plus (inner : QueryRegexV1)
  | opt (inner : QueryRegexV1)
  deriving Repr

inductive QueryAtomV1 where
  | type (term : QueryTermV1) (typeId : Nat)
  | attrEq (term : QueryTermV1) (keyId : Nat) (valueId : Nat)
  | path (left : QueryTermV1) (regex : QueryRegexV1) (right : QueryTermV1)
  deriving Repr

structure QueryV1 where
  selectVars : Array String
  atoms : Array QueryAtomV1
  maxHops? : Option Nat
  minConfidence? : Option Prob.VProb
  deriving Repr

structure QueryBindingV1 where
  var : String
  entity : Nat
  deriving Repr

inductive QueryAtomWitnessV1 where
  | type (entity : Nat) (typeId : Nat)
  | attrEq (entity : Nat) (keyId : Nat) (valueId : Nat)
  | path (proof : ReachabilityProofV2)
  deriving Repr

structure QueryRowV1 where
  bindings : Array QueryBindingV1
  witnesses : Array QueryAtomWitnessV1
  deriving Repr

structure QueryResultProofV1 where
  query : QueryV1
  rows : Array QueryRowV1
  truncated : Bool
  deriving Repr

partial def parseQueryTermV1 (j : Json) : Except String QueryTermV1 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "var" =>
      let name ← (← j.getObjVal? "name").getStr?
      pure (.var name)
  | "const" =>
      let entity ← (← j.getObjVal? "entity").getNat?
      pure (.const entity)
  | other =>
      throw s!"unknown query term type: {other}"

partial def parseQueryRegexV1 (j : Json) : Except String QueryRegexV1 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "epsilon" => pure .epsilon
  | "rel" =>
      let relTypeId ← (← j.getObjVal? "rel_type_id").getNat?
      pure (.rel relTypeId)
  | "seq" =>
      let partsJson ← (← j.getObjVal? "parts").getArr?
      let mut parts : Array QueryRegexV1 := #[]
      for p in partsJson do
        parts := parts.push (← parseQueryRegexV1 p)
      pure (.seq parts)
  | "alt" =>
      let partsJson ← (← j.getObjVal? "parts").getArr?
      let mut parts : Array QueryRegexV1 := #[]
      for p in partsJson do
        parts := parts.push (← parseQueryRegexV1 p)
      pure (.alt parts)
  | "star" =>
      let inner ← parseQueryRegexV1 (← j.getObjVal? "inner")
      pure (.star inner)
  | "plus" =>
      let inner ← parseQueryRegexV1 (← j.getObjVal? "inner")
      pure (.plus inner)
  | "opt" =>
      let inner ← parseQueryRegexV1 (← j.getObjVal? "inner")
      pure (.opt inner)
  | other =>
      throw s!"unknown query regex type: {other}"

partial def parseQueryAtomV1 (j : Json) : Except String QueryAtomV1 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" =>
      let term ← parseQueryTermV1 (← j.getObjVal? "term")
      let typeId ← (← j.getObjVal? "type_id").getNat?
      pure (.type term typeId)
  | "attr_eq" =>
      let term ← parseQueryTermV1 (← j.getObjVal? "term")
      let keyId ← (← j.getObjVal? "key_id").getNat?
      let valueId ← (← j.getObjVal? "value_id").getNat?
      pure (.attrEq term keyId valueId)
  | "path" =>
      let left ← parseQueryTermV1 (← j.getObjVal? "left")
      let regex ← parseQueryRegexV1 (← j.getObjVal? "regex")
      let right ← parseQueryTermV1 (← j.getObjVal? "right")
      pure (.path left regex right)
  | other =>
      throw s!"unknown query atom type: {other}"

partial def parseQueryV1 (j : Json) : Except String QueryV1 := do
  let selectVarsJson ← (← j.getObjVal? "select_vars").getArr?
  let mut selectVars : Array String := #[]
  for v in selectVarsJson do
    selectVars := selectVars.push (← v.getStr?)

  let atomsJson ← (← j.getObjVal? "atoms").getArr?
  let mut atoms : Array QueryAtomV1 := #[]
  for a in atomsJson do
    atoms := atoms.push (← parseQueryAtomV1 a)

  let maxHops? : Option Nat ←
    match (j.getObjVal? "max_hops").toOption with
    | none => pure none
    | some mh => pure (some (← mh.getNat?))

  let minConfidence? : Option Prob.VProb ←
    match (j.getObjVal? "min_confidence_fp").toOption with
    | none => pure none
    | some mc => pure (some (← FixedPointProbability.parseVProb mc))

  pure { selectVars, atoms, maxHops?, minConfidence? }

partial def parseQueryBindingV1 (j : Json) : Except String QueryBindingV1 := do
  let var ← (← j.getObjVal? "var").getStr?
  let entity ← (← j.getObjVal? "entity").getNat?
  pure { var, entity }

partial def parseQueryAtomWitnessV1 (j : Json) : Except String QueryAtomWitnessV1 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" =>
      let entity ← (← j.getObjVal? "entity").getNat?
      let typeId ← (← j.getObjVal? "type_id").getNat?
      pure (.type entity typeId)
  | "attr_eq" =>
      let entity ← (← j.getObjVal? "entity").getNat?
      let keyId ← (← j.getObjVal? "key_id").getNat?
      let valueId ← (← j.getObjVal? "value_id").getNat?
      pure (.attrEq entity keyId valueId)
  | "path" =>
      let proof ← parseReachabilityProofV2 (← j.getObjVal? "proof")
      pure (.path proof)
  | other =>
      throw s!"unknown query witness type: {other}"

partial def parseQueryRowV1 (j : Json) : Except String QueryRowV1 := do
  let bindingsJson ← (← j.getObjVal? "bindings").getArr?
  let mut bindings : Array QueryBindingV1 := #[]
  for b in bindingsJson do
    bindings := bindings.push (← parseQueryBindingV1 b)

  let witnessesJson ← (← j.getObjVal? "witnesses").getArr?
  let mut witnesses : Array QueryAtomWitnessV1 := #[]
  for w in witnessesJson do
    witnesses := witnesses.push (← parseQueryAtomWitnessV1 w)

  pure { bindings, witnesses }

partial def parseQueryResultProofV1 (j : Json) : Except String QueryResultProofV1 := do
  let query ← parseQueryV1 (← j.getObjVal? "query")

  let rowsJson ← (← j.getObjVal? "rows").getArr?
  let mut rows : Array QueryRowV1 := #[]
  for r in rowsJson do
    rows := rows.push (← parseQueryRowV1 r)

  let truncated : Bool ← fromJson? (← j.getObjVal? "truncated")
  pure { query, rows, truncated }

/-!
### Disjunction: unions of conjunctive queries (UCQs)

The certified query kernel starts with conjunctive queries (CQs). The next
expressive step is **top-level disjunction**: a query is an OR of conjunctive
branches, and each returned row includes the chosen branch + witnesses.
-/

structure QueryV2 where
  selectVars : Array String
  /-- Disjuncts (OR-branches), each a conjunction of atoms. -/
  disjuncts : Array (Array QueryAtomV1)
  maxHops? : Option Nat
  minConfidence? : Option Prob.VProb
  deriving Repr

structure QueryRowV2 where
  /-- Index into `query.disjuncts`. -/
  disjunct : Nat
  bindings : Array QueryBindingV1
  witnesses : Array QueryAtomWitnessV1
  deriving Repr

structure QueryResultProofV2 where
  query : QueryV2
  rows : Array QueryRowV2
  truncated : Bool
  deriving Repr

partial def parseQueryV2 (j : Json) : Except String QueryV2 := do
  let selectVarsJson ← (← j.getObjVal? "select_vars").getArr?
  let mut selectVars : Array String := #[]
  for v in selectVarsJson do
    selectVars := selectVars.push (← v.getStr?)

  let disjunctsJson ← (← j.getObjVal? "disjuncts").getArr?
  let mut disjuncts : Array (Array QueryAtomV1) := #[]
  for d in disjunctsJson do
    let atomsJson ← d.getArr?
    let mut atoms : Array QueryAtomV1 := #[]
    for a in atomsJson do
      atoms := atoms.push (← parseQueryAtomV1 a)
    disjuncts := disjuncts.push atoms

  let maxHops? : Option Nat ←
    match (j.getObjVal? "max_hops").toOption with
    | none => pure none
    | some mh => pure (some (← mh.getNat?))

  let minConfidence? : Option Prob.VProb ←
    match (j.getObjVal? "min_confidence_fp").toOption with
    | none => pure none
    | some mc => pure (some (← FixedPointProbability.parseVProb mc))

  pure { selectVars, disjuncts, maxHops?, minConfidence? }

partial def parseQueryRowV2 (j : Json) : Except String QueryRowV2 := do
  let disjunct ← (← j.getObjVal? "disjunct").getNat?

  let bindingsJson ← (← j.getObjVal? "bindings").getArr?
  let mut bindings : Array QueryBindingV1 := #[]
  for b in bindingsJson do
    bindings := bindings.push (← parseQueryBindingV1 b)

  let witnessesJson ← (← j.getObjVal? "witnesses").getArr?
  let mut witnesses : Array QueryAtomWitnessV1 := #[]
  for w in witnessesJson do
    witnesses := witnesses.push (← parseQueryAtomWitnessV1 w)

  pure { disjunct, bindings, witnesses }

partial def parseQueryResultProofV2 (j : Json) : Except String QueryResultProofV2 := do
  let query ← parseQueryV2 (← j.getObjVal? "query")

  let rowsJson ← (← j.getObjVal? "rows").getArr?
  let mut rows : Array QueryRowV2 := #[]
  for r in rowsJson do
    rows := rows.push (← parseQueryRowV2 r)

  let truncated : Bool ← fromJson? (← j.getObjVal? "truncated")
  pure { query, rows, truncated }

/-!
### v3: name-based, `.axi`-anchored query certificates

`query_result_v3` is the `.axi`-anchored successor to `query_result_v2`.

Key differences:

* entities are referenced by stable **names** (and fact ids) rather than numeric ids,
* reachability witnesses are anchored to canonical tuple facts via `axi_fact_id`
  (so the checker can validate without requiring a `PathDBExportV1` snapshot table).
-/

inductive QueryTermV3 where
  | var (name : String)
  | const (entity : String)
  deriving Repr

inductive QueryRegexV3 where
  | epsilon
  | rel (rel : String)
  | seq (parts : Array QueryRegexV3)
  | alt (parts : Array QueryRegexV3)
  | star (inner : QueryRegexV3)
  | plus (inner : QueryRegexV3)
  | opt (inner : QueryRegexV3)
  deriving Repr

inductive QueryAtomV3 where
  | type (term : QueryTermV3) (typeName : String)
  | attrEq (term : QueryTermV3) (key : String) (value : String)
  | path (left : QueryTermV3) (regex : QueryRegexV3) (right : QueryTermV3)
  deriving Repr

structure QueryV3 where
  selectVars : Array String
  disjuncts : Array (Array QueryAtomV3)
  maxHops? : Option Nat
  minConfidence? : Option Prob.VProb
  deriving Repr

structure QueryBindingV3 where
  var : String
  entity : String
  deriving Repr

inductive ReachabilityProofV3 where
  | reflexive (entity : String)
  | step
      (src : String)
      (rel : String)
      (dst : String)
      (relConfidence : Prob.VProb)
      (axiFactId : String)
      (rest : ReachabilityProofV3)
  deriving Repr

def ReachabilityProofV3.start : ReachabilityProofV3 → String
  | .reflexive entity => entity
  | .step src .. => src

def ReachabilityProofV3.end_ : ReachabilityProofV3 → String
  | .reflexive entity => entity
  | .step _ _ _ _ _ rest => rest.end_

def ReachabilityProofV3.pathLen : ReachabilityProofV3 → Nat
  | .reflexive _ => 0
  | .step _ _ _ _ _ rest => rest.pathLen + 1

def ReachabilityProofV3.confidence : ReachabilityProofV3 → Prob.VProb
  | .reflexive _ => Prob.vOne
  | .step _ _ _ relConfidence _ rest => Prob.vMult relConfidence rest.confidence

partial def parseReachabilityProofV3 (j : Json) : Except String ReachabilityProofV3 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "reflexive" =>
      let entity ← (← j.getObjVal? "entity").getStr?
      pure (.reflexive entity)
  | "step" =>
      let src ← (← j.getObjVal? "from").getStr?
      let rel ← (← j.getObjVal? "rel").getStr?
      let dst ← (← j.getObjVal? "to").getStr?
      let relConfidence ← FixedPointProbability.parseVProb (← j.getObjVal? "rel_confidence_fp")
      let axiFactId ← (← j.getObjVal? "axi_fact_id").getStr?
      let rest ← parseReachabilityProofV3 (← j.getObjVal? "rest")
      pure (.step src rel dst relConfidence axiFactId rest)
  | other =>
      throw s!"unknown reachability_v3 proof type: {other}"

partial def parseQueryTermV3 (j : Json) : Except String QueryTermV3 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "var" =>
      let name ← (← j.getObjVal? "name").getStr?
      pure (.var name)
  | "const" =>
      let entity ← (← j.getObjVal? "entity").getStr?
      pure (.const entity)
  | other =>
      throw s!"unknown query term v3 type: {other}"

partial def parseQueryRegexV3 (j : Json) : Except String QueryRegexV3 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "epsilon" => pure .epsilon
  | "rel" =>
      let rel ← (← j.getObjVal? "rel").getStr?
      pure (.rel rel)
  | "seq" =>
      let partsJson ← (← j.getObjVal? "parts").getArr?
      let mut parts : Array QueryRegexV3 := #[]
      for p in partsJson do
        parts := parts.push (← parseQueryRegexV3 p)
      pure (.seq parts)
  | "alt" =>
      let partsJson ← (← j.getObjVal? "parts").getArr?
      let mut parts : Array QueryRegexV3 := #[]
      for p in partsJson do
        parts := parts.push (← parseQueryRegexV3 p)
      pure (.alt parts)
  | "star" =>
      let inner ← parseQueryRegexV3 (← j.getObjVal? "inner")
      pure (.star inner)
  | "plus" =>
      let inner ← parseQueryRegexV3 (← j.getObjVal? "inner")
      pure (.plus inner)
  | "opt" =>
      let inner ← parseQueryRegexV3 (← j.getObjVal? "inner")
      pure (.opt inner)
  | other =>
      throw s!"unknown query regex v3 type: {other}"

partial def parseQueryAtomV3 (j : Json) : Except String QueryAtomV3 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" =>
      let term ← parseQueryTermV3 (← j.getObjVal? "term")
      let typeName ← (← j.getObjVal? "type_name").getStr?
      pure (.type term typeName)
  | "attr_eq" =>
      let term ← parseQueryTermV3 (← j.getObjVal? "term")
      let key ← (← j.getObjVal? "key").getStr?
      let value ← (← j.getObjVal? "value").getStr?
      pure (.attrEq term key value)
  | "path" =>
      let left ← parseQueryTermV3 (← j.getObjVal? "left")
      let regex ← parseQueryRegexV3 (← j.getObjVal? "regex")
      let right ← parseQueryTermV3 (← j.getObjVal? "right")
      pure (.path left regex right)
  | other =>
      throw s!"unknown query atom v3 type: {other}"

partial def parseQueryV3 (j : Json) : Except String QueryV3 := do
  let selectVarsJson ← (← j.getObjVal? "select_vars").getArr?
  let mut selectVars : Array String := #[]
  for v in selectVarsJson do
    selectVars := selectVars.push (← v.getStr?)

  let disjunctsJson ← (← j.getObjVal? "disjuncts").getArr?
  let mut disjuncts : Array (Array QueryAtomV3) := #[]
  for d in disjunctsJson do
    let atomsJson ← d.getArr?
    let mut atoms : Array QueryAtomV3 := #[]
    for a in atomsJson do
      atoms := atoms.push (← parseQueryAtomV3 a)
    disjuncts := disjuncts.push atoms

  let maxHops? : Option Nat ←
    match (j.getObjVal? "max_hops").toOption with
    | none => pure none
    | some mh => pure (some (← mh.getNat?))

  let minConfidence? : Option Prob.VProb ←
    match (j.getObjVal? "min_confidence_fp").toOption with
    | none => pure none
    | some mc => pure (some (← FixedPointProbability.parseVProb mc))

  pure { selectVars, disjuncts, maxHops?, minConfidence? }

partial def parseQueryBindingV3 (j : Json) : Except String QueryBindingV3 := do
  let var ← (← j.getObjVal? "var").getStr?
  let entity ← (← j.getObjVal? "entity").getStr?
  pure { var, entity }

inductive QueryAtomWitnessV3 where
  | type (entity : String) (typeName : String)
  | attrEq (entity : String) (key : String) (value : String)
  | path (proof : ReachabilityProofV3)
  deriving Repr

partial def parseQueryAtomWitnessV3 (j : Json) : Except String QueryAtomWitnessV3 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" =>
      let entity ← (← j.getObjVal? "entity").getStr?
      let typeName ← (← j.getObjVal? "type_name").getStr?
      pure (.type entity typeName)
  | "attr_eq" =>
      let entity ← (← j.getObjVal? "entity").getStr?
      let key ← (← j.getObjVal? "key").getStr?
      let value ← (← j.getObjVal? "value").getStr?
      pure (.attrEq entity key value)
  | "path" =>
      let proof ← parseReachabilityProofV3 (← j.getObjVal? "proof")
      pure (.path proof)
  | other =>
      throw s!"unknown query witness v3 type: {other}"

structure QueryRowV3 where
  disjunct : Nat
  bindings : Array QueryBindingV3
  witnesses : Array QueryAtomWitnessV3
  deriving Repr

partial def parseQueryRowV3 (j : Json) : Except String QueryRowV3 := do
  let disjunct ← (← j.getObjVal? "disjunct").getNat?

  let bindingsJson ← (← j.getObjVal? "bindings").getArr?
  let mut bindings : Array QueryBindingV3 := #[]
  for b in bindingsJson do
    bindings := bindings.push (← parseQueryBindingV3 b)

  let witnessesJson ← (← j.getObjVal? "witnesses").getArr?
  let mut witnesses : Array QueryAtomWitnessV3 := #[]
  for w in witnessesJson do
    witnesses := witnesses.push (← parseQueryAtomWitnessV3 w)

  pure { disjunct, bindings, witnesses }

structure QueryResultProofV3 where
  query : QueryV3
  rows : Array QueryRowV3
  truncated : Bool
  elaborationRewrites : Array RewriteDerivationProofV3 := #[]
  deriving Repr

partial def parseQueryResultProofV3 (j : Json) : Except String QueryResultProofV3 := do
  let query ← parseQueryV3 (← j.getObjVal? "query")

  let rowsJson ← (← j.getObjVal? "rows").getArr?
  let mut rows : Array QueryRowV3 := #[]
  for r in rowsJson do
    rows := rows.push (← parseQueryRowV3 r)

  let truncated : Bool ← fromJson? (← j.getObjVal? "truncated")

  let elaborationRewritesJson? := (j.getObjVal? "elaboration_rewrites").toOption
  let mut elaborationRewrites : Array RewriteDerivationProofV3 := #[]
  match elaborationRewritesJson? with
  | none => pure ()
  | some arrJson =>
      let arr ← arrJson.getArr?
      for item in arr do
        elaborationRewrites := elaborationRewrites.push (← parseRewriteDerivationProofV3 item)

  pure { query, rows, truncated, elaborationRewrites }

/-!
## v2: `.axi` well-typedness (AST-level)

`axi_well_typed_v1` certificates are a small "trusted gate" for canonical
inputs:

* Rust emits an envelope anchored to the input module digest, and
* Lean re-parses + re-checks the module with a small decision procedure.

The proof payload is a lightweight summary (counts) to keep Rust/Lean
implementations in lockstep.
-/

structure AxiWellTypedProofV1 where
  moduleName : String
  schemaCount : Nat
  theoryCount : Nat
  instanceCount : Nat
  assignmentCount : Nat
  tupleCount : Nat
  deriving Repr, DecidableEq

partial def parseAxiWellTypedProofV1 (j : Json) : Except String AxiWellTypedProofV1 := do
  let moduleName ← (← j.getObjVal? "module_name").getStr?
  let schemaCount ← (← j.getObjVal? "schema_count").getNat?
  let theoryCount ← (← j.getObjVal? "theory_count").getNat?
  let instanceCount ← (← j.getObjVal? "instance_count").getNat?
  let assignmentCount ← (← j.getObjVal? "assignment_count").getNat?
  let tupleCount ← (← j.getObjVal? "tuple_count").getNat?
  pure { moduleName, schemaCount, theoryCount, instanceCount, assignmentCount, tupleCount }

/-!
`axi_constraints_ok_v1` is a conservative certificate kind that checks a small,
high-ROI subset of theory constraints:

* `key(...)`
* `functional Rel.field -> Rel.field`

The trusted checker re-runs the constraint checks on the anchored `.axi` module
and compares the summary payload (counts).
-/

structure AxiConstraintsOkProofV1 where
  moduleName : String
  constraintCount : Nat
  instanceCount : Nat
  checkCount : Nat
  deriving Repr, DecidableEq

partial def parseAxiConstraintsOkProofV1 (j : Json) : Except String AxiConstraintsOkProofV1 := do
  let moduleName ← (← j.getObjVal? "module_name").getStr?
  let constraintCount ← (← j.getObjVal? "constraint_count").getNat?
  let instanceCount ← (← j.getObjVal? "instance_count").getNat?
  let checkCount ← (← j.getObjVal? "check_count").getNat?
  pure { moduleName, constraintCount, instanceCount, checkCount }

inductive Certificate where
  | reachabilityV1 (proof : ReachabilityProof)
  | reachabilityV2 (proof : ReachabilityProofV2)
  | resolutionV2 (proof : ResolutionProofV2)
  | axiWellTypedV1 (proof : AxiWellTypedProofV1)
  | axiConstraintsOkV1 (proof : AxiConstraintsOkProofV1)
  | queryResultV1 (proof : QueryResultProofV1)
  | queryResultV2 (proof : QueryResultProofV2)
  | queryResultV3 (proof : QueryResultProofV3)
  | normalizePathV2 (proof : NormalizePathProofV2)
  | rewriteDerivationV2 (proof : RewriteDerivationProofV2)
  | rewriteDerivationV3 (proof : RewriteDerivationProofV3)
  | pathEquivV2 (proof : PathEquivProofV2)
  | deltaFV1 (proof : Migration.DeltaFMigrationProofV1)
  deriving Repr

/-!
## Certificate envelopes (optional `.axi` anchors)

The core `Certificate` inductive captures the *semantic payload* (kind + proof).

For end-to-end verification we also want an optional **anchor** that binds a
certificate to canonical `.axi` inputs (snapshot-scoped).

We keep this wrapper separate so:

* older fixtures remain valid (no anchors), and
* the trusted checker can opt into stronger checks when anchor contexts are
  provided (e.g. ensuring referenced fact IDs exist in the snapshot).
-/

structure CertificateAnchorV1 where
  /-- Stable digest for the `.axi` module this certificate is about. -/
  axiDigestV1 : String
  deriving Repr, DecidableEq

partial def parseCertificateAnchorV1 (j : Json) : Except String CertificateAnchorV1 := do
  let digest ← (← j.getObjVal? "axi_digest_v1").getStr?
  pure { axiDigestV1 := digest }

structure CertificateEnvelope where
  anchor? : Option CertificateAnchorV1
  certificate : Certificate
  deriving Repr

def parseCertificate (j : Json) : Except String Certificate := do
  let version ← (← j.getObjVal? "version").getNat?
  let kind ← (← j.getObjVal? "kind").getStr?
  match kind with
  | "reachability" =>
      if version != 1 then
        throw s!"unsupported reachability certificate version: {version}"
      let proof ← parseReachabilityProof (← j.getObjVal? "proof")
      pure (.reachabilityV1 proof)
  | "reachability_v2" =>
      if version != 2 then
        throw s!"unsupported reachability_v2 certificate version: {version}"
      let proof ← parseReachabilityProofV2 (← j.getObjVal? "proof")
      pure (.reachabilityV2 proof)
  | "resolution_v2" =>
      if version != 2 then
        throw s!"unsupported resolution_v2 certificate version: {version}"
      let proof ← parseResolutionProofV2 (← j.getObjVal? "proof")
      pure (.resolutionV2 proof)
  | "axi_well_typed_v1" =>
      if version != 2 then
        throw s!"unsupported axi_well_typed_v1 certificate version: {version}"
      let proof ← parseAxiWellTypedProofV1 (← j.getObjVal? "proof")
      pure (.axiWellTypedV1 proof)
  | "axi_constraints_ok_v1" =>
      if version != 2 then
        throw s!"unsupported axi_constraints_ok_v1 certificate version: {version}"
      let proof ← parseAxiConstraintsOkProofV1 (← j.getObjVal? "proof")
      pure (.axiConstraintsOkV1 proof)
  | "query_result_v1" =>
      if version != 2 then
        throw s!"unsupported query_result_v1 certificate version: {version}"
      let proof ← parseQueryResultProofV1 (← j.getObjVal? "proof")
      pure (.queryResultV1 proof)
  | "query_result_v2" =>
      if version != 2 then
        throw s!"unsupported query_result_v2 certificate version: {version}"
      let proof ← parseQueryResultProofV2 (← j.getObjVal? "proof")
      pure (.queryResultV2 proof)
  | "query_result_v3" =>
      if version != 2 then
        throw s!"unsupported query_result_v3 certificate version: {version}"
      let proof ← parseQueryResultProofV3 (← j.getObjVal? "proof")
      pure (.queryResultV3 proof)
  | "normalize_path_v2" =>
      if version != 2 then
        throw s!"unsupported normalize_path_v2 certificate version: {version}"
      let proof ← parseNormalizePathProofV2 (← j.getObjVal? "proof")
      pure (.normalizePathV2 proof)
  | "rewrite_derivation_v2" =>
      if version != 2 then
        throw s!"unsupported rewrite_derivation_v2 certificate version: {version}"
      let proof ← parseRewriteDerivationProofV2 (← j.getObjVal? "proof")
      pure (.rewriteDerivationV2 proof)
  | "rewrite_derivation_v3" =>
      if version != 2 then
        throw s!"unsupported rewrite_derivation_v3 certificate version: {version}"
      let proof ← parseRewriteDerivationProofV3 (← j.getObjVal? "proof")
      pure (.rewriteDerivationV3 proof)
  | "path_equiv_v2" =>
      if version != 2 then
        throw s!"unsupported path_equiv_v2 certificate version: {version}"
      let proof ← parsePathEquivProofV2 (← j.getObjVal? "proof")
      pure (.pathEquivV2 proof)
  | "delta_f_v1" =>
      if version != 2 then
        throw s!"unsupported delta_f_v1 certificate version: {version}"
      let proof ← Migration.parseDeltaFMigrationProofV1 (← j.getObjVal? "proof")
      pure (.deltaFV1 proof)
  | other =>
      throw s!"unknown certificate kind: {other}"

def parseCertificateEnvelope (j : Json) : Except String CertificateEnvelope := do
  let anchor? : Option CertificateAnchorV1 ←
    match (j.getObjVal? "anchor").toOption with
    | none => pure none
    | some a => pure (some (← parseCertificateAnchorV1 a))
  let cert ← parseCertificate j
  pure { anchor?, certificate := cert }

end Axiograph
