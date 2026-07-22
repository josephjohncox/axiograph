import Lean
import Axiograph.Prob.Verified
import Axiograph.Axi.SchemaV1
import Axiograph.Theory.Finite

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

The expression language is intentionally small: identity, generator edge,
composition, and formal inverse.
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

  When absent, Lean uses the current compact payload mode: recompute
  normalization and compare.
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
### Canonical finite query witness syntax

`query_result_v4` is the only query certificate family. Its finite syntax is
named V4 throughout Rust and Lean; there is no compatibility alias.

Key differences:

* entities are referenced by stable **names** (and fact ids) rather than numeric ids,
* reachability witnesses are anchored to canonical tuple facts via `axi_fact_id`
  so the checker never trusts a derived query image.

For `query_result_v4`, witness replay establishes row soundness and the finite
denotation checker establishes exact completeness for the declared bounded
fragment. Truncation is rejected rather than treated as replay metadata.
-/

inductive FiniteQueryTermV4 where
  | var (name : String)
  | const (entity : String)
  deriving Repr

inductive FiniteQueryRegexV4 where
  | epsilon
  | rel (rel : String)
  | seq (parts : Array FiniteQueryRegexV4)
  | alt (parts : Array FiniteQueryRegexV4)
  | star (inner : FiniteQueryRegexV4)
  | plus (inner : FiniteQueryRegexV4)
  | opt (inner : FiniteQueryRegexV4)
  deriving Repr

inductive FiniteQueryAtomV4 where
  | type (term : FiniteQueryTermV4) (typeName : String)
  | attrEq (term : FiniteQueryTermV4) (key : String) (value : String)
  | path (left : FiniteQueryTermV4) (regex : FiniteQueryRegexV4) (right : FiniteQueryTermV4)
  deriving Repr

structure FiniteQueryV4 where
  selectVars : Array String
  disjuncts : Array (Array FiniteQueryAtomV4)
  maxHops? : Option Nat
  minConfidence? : Option Prob.VProb
  deriving Repr

structure FiniteQueryBindingV4 where
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

partial def parseFiniteQueryTermV4 (j : Json) : Except String FiniteQueryTermV4 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "var" =>
      let name ← (← j.getObjVal? "name").getStr?
      pure (.var name)
  | "const" =>
      let entity ← (← j.getObjVal? "entity").getStr?
      pure (.const entity)
  | other =>
      throw s!"unknown finite query term v4 type: {other}"

partial def parseFiniteQueryRegexV4 (j : Json) : Except String FiniteQueryRegexV4 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "epsilon" => pure .epsilon
  | "rel" =>
      let rel ← (← j.getObjVal? "rel").getStr?
      pure (.rel rel)
  | "seq" =>
      let partsJson ← (← j.getObjVal? "parts").getArr?
      let mut parts : Array FiniteQueryRegexV4 := #[]
      for p in partsJson do
        parts := parts.push (← parseFiniteQueryRegexV4 p)
      pure (.seq parts)
  | "alt" =>
      let partsJson ← (← j.getObjVal? "parts").getArr?
      let mut parts : Array FiniteQueryRegexV4 := #[]
      for p in partsJson do
        parts := parts.push (← parseFiniteQueryRegexV4 p)
      pure (.alt parts)
  | "star" =>
      let inner ← parseFiniteQueryRegexV4 (← j.getObjVal? "inner")
      pure (.star inner)
  | "plus" =>
      let inner ← parseFiniteQueryRegexV4 (← j.getObjVal? "inner")
      pure (.plus inner)
  | "opt" =>
      let inner ← parseFiniteQueryRegexV4 (← j.getObjVal? "inner")
      pure (.opt inner)
  | other =>
      throw s!"unknown finite query regex v4 type: {other}"

partial def parseFiniteQueryAtomV4 (j : Json) : Except String FiniteQueryAtomV4 := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" =>
      let term ← parseFiniteQueryTermV4 (← j.getObjVal? "term")
      let typeName ← (← j.getObjVal? "type_name").getStr?
      pure (.type term typeName)
  | "attr_eq" =>
      let term ← parseFiniteQueryTermV4 (← j.getObjVal? "term")
      let key ← (← j.getObjVal? "key").getStr?
      let value ← (← j.getObjVal? "value").getStr?
      pure (.attrEq term key value)
  | "path" =>
      let left ← parseFiniteQueryTermV4 (← j.getObjVal? "left")
      let regex ← parseFiniteQueryRegexV4 (← j.getObjVal? "regex")
      let right ← parseFiniteQueryTermV4 (← j.getObjVal? "right")
      pure (.path left regex right)
  | other =>
      throw s!"unknown finite query atom v4 type: {other}"

partial def parseFiniteQueryV4 (j : Json) : Except String FiniteQueryV4 := do
  let selectVarsJson ← (← j.getObjVal? "select_vars").getArr?
  let mut selectVars : Array String := #[]
  for v in selectVarsJson do
    selectVars := selectVars.push (← v.getStr?)

  let disjunctsJson ← (← j.getObjVal? "disjuncts").getArr?
  let mut disjuncts : Array (Array FiniteQueryAtomV4) := #[]
  for d in disjunctsJson do
    let atomsJson ← d.getArr?
    let mut atoms : Array FiniteQueryAtomV4 := #[]
    for a in atomsJson do
      atoms := atoms.push (← parseFiniteQueryAtomV4 a)
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

partial def parseFiniteQueryBindingV4 (j : Json) : Except String FiniteQueryBindingV4 := do
  let var ← (← j.getObjVal? "var").getStr?
  let entity ← (← j.getObjVal? "entity").getStr?
  pure { var, entity }

inductive FiniteQueryAtomWitnessV4 where
  | type (entity : String) (typeName : String)
  | attrEq (entity : String) (key : String) (value : String)
  | path (proof : ReachabilityProofV3)
  deriving Repr

partial def parseFiniteQueryAtomWitnessV4 (j : Json) : Except String FiniteQueryAtomWitnessV4 := do
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
      throw s!"unknown finite query witness v4 type: {other}"

structure FiniteQueryRowV4 where
  disjunct : Nat
  bindings : Array FiniteQueryBindingV4
  witnesses : Array FiniteQueryAtomWitnessV4
  deriving Repr

partial def parseFiniteQueryRowV4 (j : Json) : Except String FiniteQueryRowV4 := do
  let disjunct ← (← j.getObjVal? "disjunct").getNat?

  let bindingsJson ← (← j.getObjVal? "bindings").getArr?
  let mut bindings : Array FiniteQueryBindingV4 := #[]
  for b in bindingsJson do
    bindings := bindings.push (← parseFiniteQueryBindingV4 b)

  let witnessesJson ← (← j.getObjVal? "witnesses").getArr?
  let mut witnesses : Array FiniteQueryAtomWitnessV4 := #[]
  for w in witnessesJson do
    witnesses := witnesses.push (← parseFiniteQueryAtomWitnessV4 w)

  pure { disjunct, bindings, witnesses }

/-!
### v4: prepared-query and returned-answer binding

V4 is accepted only inside certificate envelope V3. It uses the canonical
query/row witness syntax plus a cryptographic prepared binding, an explicit row
limit, a finite-exact-completeness claim kind, and an answer digest. Unknown
fields reject throughout the V4 payload so an older checker cannot silently
accept an upgrade by ignoring new data.
-/

def requireExactFields (j : Json) (allowed required : List String) : Except String Unit := do
  match j with
  | .obj fields =>
      for (field, _) in fields.toList do
        if !allowed.contains field then
          throw s!"unknown query_result_v4 field `{field}`"
      for field in required do
        if !(fields.contains field) then
          throw s!"missing query_result_v4 field `{field}`"
  | _ => throw "query_result_v4 value must be a JSON object"

partial def requireStrictFiniteQueryTermV4Json (j : Json) : Except String Unit := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "var" => requireExactFields j ["type", "name"] ["type", "name"]
  | "const" => requireExactFields j ["type", "entity"] ["type", "entity"]
  | other => throw s!"unknown strict query term type: {other}"

partial def requireStrictFiniteQueryRegexV4Json (j : Json) : Except String Unit := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "epsilon" => requireExactFields j ["type"] ["type"]
  | "rel" => requireExactFields j ["type", "rel"] ["type", "rel"]
  | "seq" | "alt" => do
      requireExactFields j ["type", "parts"] ["type", "parts"]
      for part in (← (← j.getObjVal? "parts").getArr?) do
        requireStrictFiniteQueryRegexV4Json part
  | "star" | "plus" | "opt" => do
      requireExactFields j ["type", "inner"] ["type", "inner"]
      requireStrictFiniteQueryRegexV4Json (← j.getObjVal? "inner")
  | other => throw s!"unknown strict query regex type: {other}"

partial def requireStrictFiniteQueryAtomV4Json (j : Json) : Except String Unit := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" => do
      requireExactFields j ["type", "term", "type_name"] ["type", "term", "type_name"]
      requireStrictFiniteQueryTermV4Json (← j.getObjVal? "term")
  | "attr_eq" => do
      requireExactFields j ["type", "term", "key", "value"] ["type", "term", "key", "value"]
      requireStrictFiniteQueryTermV4Json (← j.getObjVal? "term")
  | "path" => do
      requireExactFields j ["type", "left", "regex", "right"] ["type", "left", "regex", "right"]
      requireStrictFiniteQueryTermV4Json (← j.getObjVal? "left")
      requireStrictFiniteQueryRegexV4Json (← j.getObjVal? "regex")
      requireStrictFiniteQueryTermV4Json (← j.getObjVal? "right")
  | other => throw s!"unknown strict query atom type: {other}"

partial def requireStrictFiniteQueryV4Json (j : Json) : Except String Unit := do
  requireExactFields j
    ["select_vars", "disjuncts", "max_hops", "min_confidence_fp"]
    ["select_vars", "disjuncts"]
  for disjunct in (← (← j.getObjVal? "disjuncts").getArr?) do
    for atom in (← disjunct.getArr?) do
      requireStrictFiniteQueryAtomV4Json atom

partial def requireStrictReachabilityProofV3Json (j : Json) : Except String Unit := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "reflexive" =>
      requireExactFields j ["type", "entity"] ["type", "entity"]
  | "step" => do
      requireExactFields j
        ["type", "from", "rel", "to", "rel_confidence_fp", "axi_fact_id", "rest"]
        ["type", "from", "rel", "to", "rel_confidence_fp", "axi_fact_id", "rest"]
      requireStrictReachabilityProofV3Json (← j.getObjVal? "rest")
  | other => throw s!"unknown strict reachability proof type: {other}"

partial def requireStrictFiniteQueryWitnessV4Json (j : Json) : Except String Unit := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "type" =>
      requireExactFields j ["type", "entity", "type_name"] ["type", "entity", "type_name"]
  | "attr_eq" =>
      requireExactFields j ["type", "entity", "key", "value"] ["type", "entity", "key", "value"]
  | "path" => do
      requireExactFields j ["type", "proof"] ["type", "proof"]
      requireStrictReachabilityProofV3Json (← j.getObjVal? "proof")
  | other => throw s!"unknown strict query witness type: {other}"

partial def requireStrictFiniteQueryRowV4Json (j : Json) : Except String Unit := do
  requireExactFields j ["disjunct", "bindings", "witnesses"] ["disjunct", "bindings", "witnesses"]
  for binding in (← (← j.getObjVal? "bindings").getArr?) do
    requireExactFields binding ["var", "entity"] ["var", "entity"]
  for witness in (← (← j.getObjVal? "witnesses").getArr?) do
    requireStrictFiniteQueryWitnessV4Json witness

structure PreparedQueryBindingV1 where
  version : Nat
  query : FiniteQueryV4
  rowLimit : Nat
  claimKind : String
  deriving Repr

partial def parsePreparedQueryBindingV1 (j : Json) : Except String PreparedQueryBindingV1 := do
  requireExactFields j ["version", "query", "row_limit", "claim_kind"]
    ["version", "query", "row_limit", "claim_kind"]
  let version ← (← j.getObjVal? "version").getNat?
  if version != 1 then
    throw s!"unsupported prepared query binding version: {version}"
  let queryJson ← j.getObjVal? "query"
  requireStrictFiniteQueryV4Json queryJson
  let query ← parseFiniteQueryV4 queryJson
  let rowLimit ← (← j.getObjVal? "row_limit").getNat?
  let claimKind ← (← j.getObjVal? "claim_kind").getStr?
  if claimKind != "finite_exact_complete" then
    throw s!"unsupported prepared query claim kind: {claimKind}"
  pure { version, query, rowLimit, claimKind }

structure QueryResultProofV4 where
  binding : PreparedQueryBindingV1
  preparedQueryDigest : String
  rows : Array FiniteQueryRowV4
  runtimeTruncated : Bool
  answerDigest : String
  deriving Repr

partial def parseQueryResultProofV4 (j : Json) : Except String QueryResultProofV4 := do
  requireExactFields j
    ["binding", "prepared_query_digest_v1", "rows", "runtime_truncated", "answer_digest_v1"]
    ["binding", "prepared_query_digest_v1", "rows", "runtime_truncated", "answer_digest_v1"]
  let binding ← parsePreparedQueryBindingV1 (← j.getObjVal? "binding")
  let preparedQueryDigest ← (← j.getObjVal? "prepared_query_digest_v1").getStr?
  let rowsJson ← (← j.getObjVal? "rows").getArr?
  let mut rows : Array FiniteQueryRowV4 := #[]
  for rowJson in rowsJson do
    requireStrictFiniteQueryRowV4Json rowJson
    rows := rows.push (← parseFiniteQueryRowV4 rowJson)
  let runtimeTruncated : Bool ← fromJson? (← j.getObjVal? "runtime_truncated")
  let answerDigest ← (← j.getObjVal? "answer_digest_v1").getStr?
  pure { binding, preparedQueryDigest, rows, runtimeTruncated, answerDigest }

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
* `symmetric Rel` / `symmetric Rel where ...`
* `transitive Rel` (closure-compatibility for keys/functionals on carrier fields)
* `typing Rel: rule_name` (small builtin rule set)

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

/-- Exact-byte-anchored finite category-kernel proof. The checker
independently reconstructs the complete supported presentation before replaying
congruence and bounded reachability evidence. -/
structure CategoryKernelProofV3 where
  schemaName : String
  presentation : Theory.Finite.CategoryKernelPresentationV3
  congruenceCertificates : Array Theory.Finite.CategoryKernelCongruenceCertificateV3
  certificate : Theory.Finite.SaturationCertificate
  deriving Repr

private def parseJsonBool (j : Json) : Except String Bool :=
  match j with
  | .bool value => pure value
  | _ => throw "expected JSON boolean"

private def parseCategoryKernelArrowKindV3 (value : String) :
    Except String Theory.Finite.ArrowKind :=
  match value with
  | "role_projection" => pure .projection
  | "subtype_inclusion" => pure .subtypeInclusion
  | "aspect" => pure .aspect
  | "function" => pure .function
  | other => throw s!"unknown category-kernel arrow kind: {other}"

private def parseCategoryKernelRoleKindV3 (value : String) :
    Except String Theory.Finite.RoleKind :=
  match value with
  | "data" => pure .data
  | "context" => pure .context
  | "world" => pure .world
  | "temporal" => pure .temporal
  | "parameter" => pure .parameter
  | "evidence" => pure .evidence
  | other => throw s!"unknown category-kernel role kind: {other}"

private def parseCategoryKernelDirectionV3 (value : String) :
    Except String Theory.Finite.EquationDirection :=
  match value with
  | "forward" => pure .forward
  | "reverse" => pure .reverse
  | other => throw s!"unknown category-kernel equation direction: {other}"

partial def parseCategoryKernelPathV3 (j : Json) :
    Except String Theory.Finite.CategoryKernelPathV3 := do
  requireExactFields j ["source", "target", "arrows"] ["source", "target", "arrows"]
  let mut arrows : Array Nat := #[]
  for arrow in (← (← j.getObjVal? "arrows").getArr?) do
    arrows := arrows.push (← arrow.getNat?)
  pure {
    source := ← (← j.getObjVal? "source").getNat?
    target := ← (← j.getObjVal? "target").getNat?
    arrows
  }

partial def parseCategoryKernelPresentationV3 (j : Json) :
    Except String Theory.Finite.CategoryKernelPresentationV3 := do
  requireExactFields j
    ["object_names", "arrows", "relations", "identity_objects", "equations"]
    ["object_names", "arrows", "relations", "identity_objects", "equations"]
  let mut objectNames : Array String := #[]
  for name in (← (← j.getObjVal? "object_names").getArr?) do
    objectNames := objectNames.push (← name.getStr?)
  let mut arrows : Array Theory.Finite.CategoryKernelArrowV3 := #[]
  for arrow in (← (← j.getObjVal? "arrows").getArr?) do
    requireExactFields arrow ["name", "source", "target", "kind", "reversible"]
      ["name", "source", "target", "kind", "reversible"]
    arrows := arrows.push {
      name := ← (← arrow.getObjVal? "name").getStr?
      source := ← (← arrow.getObjVal? "source").getNat?
      target := ← (← arrow.getObjVal? "target").getNat?
      kind := ← parseCategoryKernelArrowKindV3 (← (← arrow.getObjVal? "kind").getStr?)
      reversible := ← parseJsonBool (← arrow.getObjVal? "reversible")
    }
  let mut relations : Array Theory.Finite.CategoryKernelRelationV3 := #[]
  for relation in (← (← j.getObjVal? "relations").getArr?) do
    requireExactFields relation ["name", "object", "roles"] ["name", "object", "roles"]
    let mut roles : Array Theory.Finite.CategoryKernelRoleV3 := #[]
    for role in (← (← relation.getObjVal? "roles").getArr?) do
      requireExactFields role ["name", "target", "projection", "declared_order", "kind"]
        ["name", "target", "projection", "declared_order", "kind"]
      roles := roles.push {
        name := ← (← role.getObjVal? "name").getStr?
        target := ← (← role.getObjVal? "target").getNat?
        projection := ← (← role.getObjVal? "projection").getNat?
        declaredOrder := ← (← role.getObjVal? "declared_order").getNat?
        kind := ← parseCategoryKernelRoleKindV3 (← (← role.getObjVal? "kind").getStr?)
      }
    relations := relations.push {
      name := ← (← relation.getObjVal? "name").getStr?
      object := ← (← relation.getObjVal? "object").getNat?
      roles
    }
  let mut identityObjects : Array Nat := #[]
  for object in (← (← j.getObjVal? "identity_objects").getArr?) do
    identityObjects := identityObjects.push (← object.getNat?)
  let mut equations : Array Theory.Finite.CategoryKernelEquationV3 := #[]
  for equation in (← (← j.getObjVal? "equations").getArr?) do
    requireExactFields equation ["name", "lhs", "rhs"] ["name", "lhs", "rhs"]
    equations := equations.push {
      name := ← (← equation.getObjVal? "name").getStr?
      lhs := ← parseCategoryKernelPathV3 (← equation.getObjVal? "lhs")
      rhs := ← parseCategoryKernelPathV3 (← equation.getObjVal? "rhs")
    }
  pure { objectNames, arrows, relations, identityObjects, equations }

partial def parseCategoryKernelCongruenceV3 (j : Json) :
    Except String Theory.Finite.CategoryKernelCongruenceCertificateV3 := do
  requireExactFields j ["input", "steps", "output"] ["input", "steps", "output"]
  let mut steps : Array Theory.Finite.CategoryKernelCongruenceStepV3 := #[]
  for step in (← (← j.getObjVal? "steps").getArr?) do
    requireExactFields step ["equation", "direction", "offset"]
      ["equation", "direction", "offset"]
    steps := steps.push {
      equation := ← (← step.getObjVal? "equation").getNat?
      direction := ← parseCategoryKernelDirectionV3
        (← (← step.getObjVal? "direction").getStr?)
      offset := ← (← step.getObjVal? "offset").getNat?
    }
  pure {
    input := ← parseCategoryKernelPathV3 (← j.getObjVal? "input")
    steps
    output := ← parseCategoryKernelPathV3 (← j.getObjVal? "output")
  }

partial def parseCategoryKernelExplanationV3 (j : Json) :
    Except String Theory.Finite.Explanation := do
  let ty ← (← j.getObjVal? "type").getStr?
  match ty with
  | "identity" =>
      requireExactFields j ["type", "object"] ["type", "object"]
      pure (.identity (← (← j.getObjVal? "object").getNat?))
  | "generator" =>
      requireExactFields j ["type", "arrow"] ["type", "arrow"]
      pure (.generator (← (← j.getObjVal? "arrow").getNat?))
  | "trans" =>
      requireExactFields j ["type", "left", "right"] ["type", "left", "right"]
      pure (.trans
        (← parseCategoryKernelExplanationV3 (← j.getObjVal? "left"))
        (← parseCategoryKernelExplanationV3 (← j.getObjVal? "right")))
  | other => throw s!"unknown category-kernel explanation type: {other}"

partial def parseCategoryKernelProofV3 (j : Json) : Except String CategoryKernelProofV3 := do
  requireExactFields j ["schema_name", "presentation", "congruence_certificates", "certificate"]
    ["schema_name", "presentation", "congruence_certificates", "certificate"]
  let schemaName ← (← j.getObjVal? "schema_name").getStr?
  let presentation ← parseCategoryKernelPresentationV3 (← j.getObjVal? "presentation")
  let mut congruenceCertificates :
      Array Theory.Finite.CategoryKernelCongruenceCertificateV3 := #[]
  for certificate in (← (← j.getObjVal? "congruence_certificates").getArr?) do
    congruenceCertificates := congruenceCertificates.push
      (← parseCategoryKernelCongruenceV3 certificate)
  let certificateJson ← j.getObjVal? "certificate"
  requireExactFields certificateJson
    ["presentation_object_count", "presentation_arrow_count", "entries", "algorithm"]
    ["presentation_object_count", "presentation_arrow_count", "entries", "algorithm"]
  let presentationObjectCount ←
    (← certificateJson.getObjVal? "presentation_object_count").getNat?
  let presentationArrowCount ←
    (← certificateJson.getObjVal? "presentation_arrow_count").getNat?
  let algorithm ← (← certificateJson.getObjVal? "algorithm").getStr?
  let mut entries : Array Theory.Finite.ReachabilityEntry := #[]
  for entryJson in (← (← certificateJson.getObjVal? "entries").getArr?) do
    requireExactFields entryJson ["source", "target", "explanation"]
      ["source", "target", "explanation"]
    entries := entries.push {
      source := ← (← entryJson.getObjVal? "source").getNat?
      target := ← (← entryJson.getObjVal? "target").getNat?
      explanation := ← parseCategoryKernelExplanationV3
        (← entryJson.getObjVal? "explanation")
    }
  pure {
    schemaName
    presentation
    congruenceCertificates
    certificate := { presentationObjectCount, presentationArrowCount, entries, algorithm }
  }

inductive Certificate where
  | reachabilityV3 (proof : ReachabilityProofV3)
  | categoryKernelV3 (proof : CategoryKernelProofV3)
  | resolutionV2 (proof : ResolutionProofV2)
  | axiWellTypedV1 (proof : AxiWellTypedProofV1)
  | axiConstraintsOkV1 (proof : AxiConstraintsOkProofV1)
  | queryResultV4 (proof : QueryResultProofV4)
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

* fixtures without anchors remain valid, and
* the trusted checker can opt into stronger checks when anchor contexts are
  provided (e.g. ensuring referenced fact IDs exist in the snapshot).
-/

structure CertificateAnchorV1 where
  /-- Exact accepted-byte revision identity for the `.axi` module. -/
  revisionDigestV2 : String
  deriving Repr, DecidableEq

partial def parseCertificateAnchorV1 (j : Json) : Except String CertificateAnchorV1 := do
  requireExactFields j ["revision_digest_v2"] ["revision_digest_v2"]
  let digest ← (← j.getObjVal? "revision_digest_v2").getStr?
  pure { revisionDigestV2 := digest }

structure CertificateAnchorV2 where
  /-- Exact accepted-byte revision identity recomputed by the checker. -/
  revisionDigestV2 : String
  deriving Repr, DecidableEq

partial def parseCertificateAnchorV2 (j : Json) : Except String CertificateAnchorV2 := do
  requireExactFields j ["revision_digest_v2"] ["revision_digest_v2"]
  let digest ← (← j.getObjVal? "revision_digest_v2").getStr?
  pure { revisionDigestV2 := digest }

inductive CertificateAnchor where
  | v1 (anchor : CertificateAnchorV1)
  | v2 (anchor : CertificateAnchorV2)
  deriving Repr

structure CertificateEnvelope where
  anchor? : Option CertificateAnchor
  certificate : Certificate
  deriving Repr

def parseCertificate (j : Json) : Except String Certificate := do
  let version ← (← j.getObjVal? "version").getNat?
  let kind ← (← j.getObjVal? "kind").getStr?
  match kind with
  | "reachability_v3" =>
      if version != 2 then
        throw s!"unsupported reachability_v3 certificate version: {version}"
      let proof ← parseReachabilityProofV3 (← j.getObjVal? "proof")
      pure (.reachabilityV3 proof)
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
  | "query_result_v4" =>
      if version != 3 then
        throw s!"unsupported query_result_v4 certificate version: {version}"
      let proof ← parseQueryResultProofV4 (← j.getObjVal? "proof")
      pure (.queryResultV4 proof)
  | "category_kernel_v3" =>
      if version != 3 then
        throw s!"unsupported category_kernel_v3 certificate version: {version}"
      let proof ← parseCategoryKernelProofV3 (← j.getObjVal? "proof")
      pure (.categoryKernelV3 proof)
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
  let version ← (← j.getObjVal? "version").getNat?
  let anchor? : Option CertificateAnchor ←
    match (j.getObjVal? "anchor").toOption with
    | none => pure none
    | some anchorJson =>
        if version == 3 then
          pure (some (CertificateAnchor.v2 (← parseCertificateAnchorV2 anchorJson)))
        else
          pure (some (CertificateAnchor.v1 (← parseCertificateAnchorV1 anchorJson)))
  if version == 3 then
    requireExactFields j ["version", "kind", "anchor", "proof"]
      ["version", "kind", "anchor", "proof"]
  let cert ← parseCertificate j
  match version, cert, anchor? with
  | 3, .queryResultV4 _, some (CertificateAnchor.v2 _) => pure ()
  | 3, .categoryKernelV3 _, some (CertificateAnchor.v2 _) => pure ()
  | 3, _, _ =>
      throw "certificate envelope V3 requires an anchored query_result_v4 or category_kernel_v3"
  | _, .queryResultV4 _, _ => throw "query_result_v4 requires certificate envelope V3"
  | _, _, _ => pure ()
  pure { anchor? := anchor?, certificate := cert }

end Axiograph
