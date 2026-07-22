import Std
import Axiograph.Certificate.Format
import Axiograph.Axi.ConstraintsCheck
import Axiograph.Axi.TypeCheck
import Axiograph.Identity
import Axiograph.Util.Sha256
import Mathlib.Computability.RegularExpressions

namespace Axiograph

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

namespace RewriteDerivation

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
  else if ruleRef.startsWith "axi-rule-v2|" then
    match ruleRef.splitOn "|" with
    | ["axi-rule-v2", revisionDigest, theoryName, ruleName] =>
        if !Axiograph.Identity.validWireFor .revision revisionDigest then
          throw s!"invalid revision identity in rule_ref: `{revisionDigest}`"
        pure (.inr (revisionDigest, theoryName, ruleName))
    | _ =>
        throw s!"invalid axi rule_ref: `{ruleRef}` (expected `axi-rule-v2|<revision>|<theory>|<rule>`)"
  else
    throw s!"unknown rule_ref prefix (expected builtin: or axi-rule-v2|): `{ruleRef}`"

partial def runDerivationV3Unanchored (input : PathExprV3) (steps : Array PathRewriteStepV3) :
    Except String PathExprV3 := do
  let mut current := input
  for s in steps do
    match (← parseRuleRefV3 s.ruleRef) with
    | .inl builtinRule =>
        current ← applyAtBuiltinV3 s.pos.toList builtinRule current
    | .inr _axiRef =>
        throw "rewrite_derivation_v3: axi: rules require a canonical `.axi` module context"
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

namespace Query

open RegularExpression
/-!
## `.axi`-anchored exact finite query checking (v4)

`query_result_v4` anchors reachability witnesses directly to canonical `.axi`
tuple facts via `axi_fact_id`; derived query indexes are not checker inputs.

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
  /-- Declared object types for this identifier in the `.axi` instance.

  Canonical `.axi` instances often list the same identifier under both a subtype
  and a supertype (e.g. `Supplier` and `Node`) for readability.

  For certificate checking, we treat that as **one object** with multiple types,
  and accept a type atom `?x : T` when *any* declared type of `x` is a subtype
  of `T` (in the schema’s subtyping closure).
  -/
  objectTypes : Std.HashSet String
  deriving Repr

structure AxiFiniteQueryIndexV4 where
  moduleName : String
  schemas : Std.HashMap String SchemaV1Schema
  tupleFacts : Std.HashMap String TupleFactInfoV3
  objects : Std.HashMap String ObjectInfoV3
  deriving Repr

def sameStringMap (left right : Std.HashMap String String) : Bool :=
  left.size == right.size && left.toList.all (fun (key, value) => right.get? key == some value)

def factIdPrefixV2 : String := "axi:fact:v2:sha256:"

def stripFactPrefixV1 (s : String) : Option String :=
  if s.startsWith factIdPrefixV2 then
    some (s.drop factIdPrefixV2.length)
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

When checking `query_result_v4` certificates, a type atom `?x : T`
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
    match decl.fields.toList with
    | [f0Decl, f1Decl] =>
        let f0 := f0Decl.field
        let f1 := f1Decl.field
        match fields.get? f0, fields.get? f1 with
        | some a, some b => some (a, b)
        | _, _ => none
    | _ => none
  else
    let primary : Array String :=
      decl.fields
        |>.map (fun f => f.field)
        |>.filter (fun f => f != "ctx" && f != "time")
    if primary.size == 2 then
      match primary.toList with
      | [f0, f1] =>
          match fields.get? f0, fields.get? f1 with
          | some a, some b => some (a, b)
          | _, _ => none
      | _ => none
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

def buildAxiFiniteQueryIndexV4 (m : Axiograph.Axi.AxiV1.AxiV1Module) : Except String AxiFiniteQueryIndexV4 := do
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
                    objects.insert name {
                      schemaName := schema.name
                      instanceName := inst.name
                      objectTypes := ({} : Std.HashSet String).insert a.name
                    }
              | some prev =>
                  if prev.schemaName != schema.name || prev.instanceName != inst.name then
                    throw s!"ambiguous object name `{name}` across assignments (query_result_v4 requires unique canonical names)"
                  else
                    objects := objects.insert name { prev with objectTypes := prev.objectTypes.insert a.name }
            else
              -- Not an object assignment in this schema; ignore (fail-closed behavior for non-canonical inputs).
              pure ()
        | .tuple _ fieldPairs =>
            -- Finite query certificates range over relation facts. Canonical
            -- function/aspect assignments belong to the category instance and
            -- are checked by module formation, but they are not graph edges.
            if schema.generators.any (fun generator => generator.name == a.name) then
              continue
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
              Axiograph.Identity.runtimeFactIdV2 m.moduleName schema.name inst.name a.name ordered
            let candidate : TupleFactInfoV3 := {
              schemaName := schema.name
              instanceName := inst.name
              relationName := a.name
              fields := fm
            }
            match tupleFacts.get? factId with
            | none => tupleFacts := tupleFacts.insert factId candidate
            | some previous =>
                if previous.schemaName != candidate.schemaName ||
                    previous.instanceName != candidate.instanceName ||
                    previous.relationName != candidate.relationName ||
                    !sameStringMap previous.fields candidate.fields then
                  throw s!"ambiguous fact-id collision `{factId}`; query_result_v4 refuses promotion"
                else
                  pure ()

  pure { moduleName := m.moduleName, schemas, tupleFacts, objects }

def entityExistsV4 (index : AxiFiniteQueryIndexV4) (entity : String) : Bool :=
  index.objects.contains entity || index.tupleFacts.contains entity

def freeVarsTermV4 : FiniteQueryTermV4 → Std.HashSet String
  | .const _ => {}
  | .var name => ({} : Std.HashSet String).insert name

def freeVarsAtomV4 : FiniteQueryAtomV4 → Std.HashSet String
  | .type term _ => freeVarsTermV4 term
  | .attrEq term _ _ => freeVarsTermV4 term
  | .path left _ right =>
      (freeVarsTermV4 left).toList.foldl
        (fun vars name => vars.insert name)
        (freeVarsTermV4 right)

def freeVarsDisjunctV4 (atoms : Array FiniteQueryAtomV4) : Std.HashSet String :=
  atoms.foldl
    (fun vars atom =>
      (freeVarsAtomV4 atom).toList.foldl (fun acc name => acc.insert name) vars)
    {}

def validateFiniteQueryV4 (query : FiniteQueryV4) : Except String Unit := do
  let mut selected : Std.HashSet String := {}
  for name in query.selectVars do
    if selected.contains name then
      throw s!"duplicate select variable `{name}`"
    selected := selected.insert name

  for atoms in query.disjuncts do
    let freeVars := freeVarsDisjunctV4 atoms
    if atoms.isEmpty && (!query.selectVars.isEmpty || !freeVars.isEmpty) then
      throw "empty disjunct is only valid for a boolean query with no selected or free variables"
    for name in query.selectVars do
      if !(freeVars.contains name) then
        throw s!"selected variable `{name}` is not bound by every query disjunct"

def resolveFiniteTermV4 (bindings : Std.HashMap String String) : FiniteQueryTermV4 → Except String String
  | .const entity => pure entity
  | .var name =>
      match bindings.get? name with
      | some entity => pure entity
      | none => throw s!"missing binding for variable `{name}`"

def toFiniteRegularExpressionV4 : FiniteQueryRegexV4 → RegularExpression String
  | .epsilon => (1 : RegularExpression String)
  | .rel rel => RegularExpression.char rel
  | .seq parts =>
      parts.foldl (fun acc p => acc * toFiniteRegularExpressionV4 p) (1 : RegularExpression String)
  | .alt parts =>
      parts.foldl (fun acc p => acc + toFiniteRegularExpressionV4 p) (0 : RegularExpression String)
  | .star inner =>
      RegularExpression.star (toFiniteRegularExpressionV4 inner)
  | .plus inner =>
      let re := toFiniteRegularExpressionV4 inner
      re * RegularExpression.star re
  | .opt inner =>
      (1 : RegularExpression String) + toFiniteRegularExpressionV4 inner

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
    (index : AxiFiniteQueryIndexV4)
    (proof : ReachabilityProofV3) : Except String ReachabilityResultV3 := do
  match proof with
  | .reflexive entity =>
      if !(entityExistsV4 index entity) then
        throw s!"reachability_v3: unknown reflexive entity `{entity}`"
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

def derivedAttrV4 (index : AxiFiniteQueryIndexV4) (entity : String) (key : String) :
    Except String (Option String) := do
  if entity.startsWith factIdPrefixV2 then
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

def verifyFiniteQueryRowV4Anchored
    (index : AxiFiniteQueryIndexV4)
    (query : FiniteQueryV4)
    (row : FiniteQueryRowV4) : Except String Unit := do
  let mut bindings : Std.HashMap String String := {}
  for b in row.bindings do
    if bindings.contains b.var then
      throw s!"duplicate binding for variable `{b.var}`"
    bindings := bindings.insert b.var b.entity

  let mut chosen : Option (Array FiniteQueryAtomV4) := none
  let mut idx : Nat := 0
  for atoms in query.disjuncts do
    if idx == row.disjunct then
      chosen := some atoms
    idx := idx + 1

  let some atoms := chosen
    | throw s!"disjunct out of bounds: {row.disjunct} (have {query.disjuncts.size})"

  let freeVars := freeVarsDisjunctV4 atoms
  if bindings.size != freeVars.size then
    throw s!"binding domain mismatch for disjunct {row.disjunct}: expected {freeVars.size} variables, got {bindings.size}"
  for name in freeVars.toList do
    if !(bindings.contains name) then
      throw s!"missing binding for free variable `{name}`"
  for b in row.bindings do
    if !(freeVars.contains b.var) then
      throw s!"extra binding for variable `{b.var}`"
    if !(entityExistsV4 index b.entity) then
      throw s!"binding for `{b.var}` references unknown entity `{b.entity}`"

  if row.witnesses.size != atoms.size then
    throw s!"witness count mismatch: expected {atoms.size}, got {row.witnesses.size}"

  for (atom, witness) in Array.zip atoms row.witnesses do
    match atom, witness with
    | .type term typeName, .type entity typeName' => do
        if typeName != typeName' then
          throw s!"type witness mismatch: expected type_name={typeName}, got {typeName'}"
        let entity' ← resolveFiniteTermV4 bindings term
        if entity != entity' then
          throw s!"type witness mismatch: expected entity={entity'}, got {entity}"

        if entity.startsWith factIdPrefixV2 then
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
          let ok :=
            obj.objectTypes.toList.any (fun actualType => isSubtypeInSchema schema actualType typeName)
          if !ok then
            throw s!"object type mismatch for `{entity}`: expected `{typeName}` (allowing subtypes), got one of {obj.objectTypes.toList}"

    | .attrEq term key value, .attrEq entity key' value' => do
        if key != key' || value != value' then
          throw s!"attr witness mismatch: expected (key={key}, value={value}), got (key={key'}, value={value'})"
        let entity' ← resolveFiniteTermV4 bindings term
        if entity != entity' then
          throw s!"attr witness mismatch: expected entity={entity'}, got {entity}"
        let actual? ← derivedAttrV4 index entity key
        match actual? with
        | none => throw s!"unknown/unsupported derived attribute `{key}` for entity `{entity}`"
        | some actual =>
            if actual != value then
              throw s!"derived attribute mismatch for `{entity}`.{key}: expected `{value}`, got `{actual}`"

    | .path left regex right, .path proof => do
        let src ← resolveFiniteTermV4 bindings left
        let dst ← resolveFiniteTermV4 bindings right

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

        let re := toFiniteRegularExpressionV4 regex
        if !(labels ∈ re.matches') then
          throw s!"path witness labels do not match RPQ (labels={labels})"

    | _, _ =>
        throw "atom/witness kind mismatch"

/-!
## Exact finite decidable query denotation

The V4 checker does not infer global ontology closure. It evaluates the finite
canonical object/fact universe in the accepted module, under explicit syntax,
assignment, regex, and hop bounds. Acceptance requires equality between that
denotation and the certificate rows, in addition to row-witness soundness.
-/

structure FiniteEdgeV4 where
  source : String
  label : String
  target : String
  deriving Repr

structure FiniteRowAssignmentV4 where
  disjunct : Nat
  bindings : Array FiniteQueryBindingV4
  deriving Repr

def finiteQueryMaxDisjuncts : Nat := 16
def finiteQueryMaxAtomsPerDisjunct : Nat := 64
def finiteQueryMaxRegexNodes : Nat := 128
def finiteQueryMaxHops : Nat := 32
def finiteQueryMaxAssignments : Nat := 1000000

def finiteEntityUniverseV4 (index : AxiFiniteQueryIndexV4) : Array String :=
  Id.run do
    let mut entities : Array String := #[]
    for (name, _) in index.objects.toList do
      entities := entities.push name
    for (factId, _) in index.tupleFacts.toList do
      entities := entities.push factId
    return entities

def buildFiniteEdgesV4 (index : AxiFiniteQueryIndexV4) : Except String (Array FiniteEdgeV4) := do
  let mut edges : Array FiniteEdgeV4 := #[]
  for (factId, tuple) in index.tupleFacts.toList do
    for (field, value) in tuple.fields.toList do
      edges := edges.push { source := factId, label := field, target := value }
      if field == "ctx" then
        edges := edges.push {
          source := factId
          label := "axi_fact_in_context"
          target := value
        }
    let some schema := index.schemas.get? tuple.schemaName
      | throw s!"missing schema `{tuple.schemaName}` while building finite query edges"
    let relation ← findRelationDecl schema tuple.relationName
    match deriveBinaryEndpointsV3 relation tuple.fields with
    | some (source, target) =>
        edges := edges.push { source, label := tuple.relationName, target }
    | none => pure ()
  pure edges

partial def regexProfileV4 : FiniteQueryRegexV4 → Nat × Bool
  | .epsilon | .rel _ => (1, false)
  | .seq parts | .alt parts =>
      parts.foldl
        (fun profile part =>
          let nested := regexProfileV4 part
          (profile.1 + nested.1, profile.2 || nested.2))
        (1, false)
  | .star inner | .plus inner =>
      let nested := regexProfileV4 inner
      (nested.1 + 1, true)
  | .opt inner =>
      let nested := regexProfileV4 inner
      (nested.1 + 1, nested.2)

partial def regexFiniteMaxLengthV4 : FiniteQueryRegexV4 → Option Nat
  | .epsilon => some 0
  | .rel _ => some 1
  | .seq parts =>
      parts.foldl
        (fun total part =>
          match total, regexFiniteMaxLengthV4 part with
          | some left, some right => some (left + right)
          | _, _ => none)
        (some 0)
  | .alt parts =>
      parts.foldl
        (fun longest part =>
          match longest, regexFiniteMaxLengthV4 part with
          | some left, some right => some (max left right)
          | _, _ => none)
        (some 0)
  | .star _ | .plus _ => none
  | .opt inner => regexFiniteMaxLengthV4 inner

def finitePathBoundV4 (query : FiniteQueryV4) (regex : FiniteQueryRegexV4) : Except String Nat :=
  match query.maxHops? with
  | some hops => pure hops
  | none =>
      match regexFiniteMaxLengthV4 regex with
      | some length => pure length
      | none => throw "finite exact path repetition requires explicit max_hops"

def matchingPathAuxV4
    (edges : Array FiniteEdgeV4)
    (expression : RegularExpression String)
    (target : String) : Nat → String → List String → Bool
  | 0, current, labels => current == target && labels ∈ expression.matches'
  | fuel + 1, current, labels =>
      (current == target && labels ∈ expression.matches') ||
        edges.any (fun edge =>
          edge.source == current &&
            matchingPathAuxV4 edges expression target fuel edge.target
              (labels ++ [edge.label]))

def finitePathExistsV4
    (edges : Array FiniteEdgeV4)
    (query : FiniteQueryV4)
    (source : String)
    (regex : FiniteQueryRegexV4)
    (target : String) : Except String Bool := do
  let bound ← finitePathBoundV4 query regex
  pure (matchingPathAuxV4 edges (toFiniteRegularExpressionV4 regex) target bound source [])

def entityHasTypeV4
    (index : AxiFiniteQueryIndexV4)
    (entity typeName : String) : Except String Bool := do
  if entity.startsWith factIdPrefixV2 then
    let some tuple := index.tupleFacts.get? entity
      | throw s!"unknown tuple fact id `{entity}`"
    let some schema := index.schemas.get? tuple.schemaName
      | throw s!"missing schema `{tuple.schemaName}` in finite query index"
    pure (isSubtypeInSchema schema (tupleEntityTypeName schema tuple.relationName) typeName)
  else
    let some object := index.objects.get? entity
      | throw s!"unknown object/entity name `{entity}`"
    let some schema := index.schemas.get? object.schemaName
      | throw s!"missing schema `{object.schemaName}` in finite query index"
    pure (object.objectTypes.toList.any (fun actual => isSubtypeInSchema schema actual typeName))

def unaryCandidateEntitiesV4
    (index : AxiFiniteQueryIndexV4)
    (atoms : Array FiniteQueryAtomV4)
    (varName : String) : Except String (Array String) := do
  let mut candidates : Array String := #[]
  for entity in finiteEntityUniverseV4 index do
    let mut accepted := true
    for atom in atoms do
      match atom with
      | .type (.var name) typeName =>
          if name == varName && !(← entityHasTypeV4 index entity typeName) then
            accepted := false
      | .attrEq (.var name) key value =>
          if name == varName && (← derivedAttrV4 index entity key) != some value then
            accepted := false
      | _ => pure ()
    if accepted then
      candidates := candidates.push entity
  pure candidates

def satisfiesFiniteAtomV4
    (index : AxiFiniteQueryIndexV4)
    (edges : Array FiniteEdgeV4)
    (query : FiniteQueryV4)
    (bindings : Std.HashMap String String) : FiniteQueryAtomV4 → Except String Bool
  | .type term typeName => do
      let entity ← resolveFiniteTermV4 bindings term
      entityHasTypeV4 index entity typeName
  | .attrEq term key value => do
      let entity ← resolveFiniteTermV4 bindings term
      pure ((← derivedAttrV4 index entity key) == some value)
  | .path left regex right => do
      let source ← resolveFiniteTermV4 bindings left
      let target ← resolveFiniteTermV4 bindings right
      finitePathExistsV4 edges query source regex target

def satisfiesFiniteDisjunctV4
    (index : AxiFiniteQueryIndexV4)
    (edges : Array FiniteEdgeV4)
    (query : FiniteQueryV4)
    (bindings : Std.HashMap String String)
    (atoms : Array FiniteQueryAtomV4) : Except String Bool := do
  for atom in atoms do
    if !(← satisfiesFiniteAtomV4 index edges query bindings atom) then
      return false
  return true

def enumerateBindingsV4
    (domains : List (String × Array String)) : Array (Array FiniteQueryBindingV4) :=
  match domains with
  | [] => #[#[]]
  | (varName, entities) :: rest =>
      let tails := enumerateBindingsV4 rest
      Id.run do
        let mut out : Array (Array FiniteQueryBindingV4) := #[]
        for entity in entities do
          for tail in tails do
            out := out.push (tail.push { var := varName, entity })
        return out

def bindingMapV4 (bindings : Array FiniteQueryBindingV4) : Std.HashMap String String :=
  bindings.foldl (fun out binding => out.insert binding.var binding.entity) {}

def validateFiniteExactFragmentV4
    (index : AxiFiniteQueryIndexV4)
    (binding : PreparedQueryBindingV1) : Except String Unit := do
  if binding.claimKind != "finite_exact_complete" then
    throw "query_result_v4 requires claim_kind=finite_exact_complete"
  if binding.query.disjuncts.isEmpty || binding.query.disjuncts.size > finiteQueryMaxDisjuncts then
    throw s!"finite exact query requires 1..{finiteQueryMaxDisjuncts} disjuncts"
  match binding.query.maxHops? with
  | some hops =>
      if hops > finiteQueryMaxHops then
        throw s!"finite exact query max_hops exceeds {finiteQueryMaxHops}"
  | none => pure ()

  let mut assignmentCount := 0
  for atoms in binding.query.disjuncts do
    if atoms.size > finiteQueryMaxAtomsPerDisjunct then
      throw s!"finite exact query disjunct exceeds {finiteQueryMaxAtomsPerDisjunct} atoms"
    for atom in atoms do
      match atom with
      | .path _ regex _ =>
          let profile := regexProfileV4 regex
          if profile.1 > finiteQueryMaxRegexNodes then
            throw s!"finite exact query regex exceeds {finiteQueryMaxRegexNodes} nodes"
          if profile.2 && binding.query.maxHops?.isNone then
            throw "finite exact query repetition requires explicit max_hops"
      | _ => pure ()
    let mut disjunctAssignments := 1
    for varName in (freeVarsDisjunctV4 atoms).toList do
      let domain ← unaryCandidateEntitiesV4 index atoms varName
      disjunctAssignments := disjunctAssignments * domain.size
    assignmentCount := assignmentCount + disjunctAssignments
    if assignmentCount > finiteQueryMaxAssignments then
      throw s!"finite exact query assignment universe exceeds {finiteQueryMaxAssignments}"

def finiteQueryDenotationV4
    (index : AxiFiniteQueryIndexV4)
    (binding : PreparedQueryBindingV1) : Except String (Array FiniteRowAssignmentV4) := do
  validateFiniteExactFragmentV4 index binding
  let edges ← buildFiniteEdgesV4 index
  let mut denotation : Array FiniteRowAssignmentV4 := #[]
  let mut disjunctIndex := 0
  for atoms in binding.query.disjuncts do
    let mut domains : List (String × Array String) := []
    for varName in (freeVarsDisjunctV4 atoms).toList do
      domains := (varName, ← unaryCandidateEntitiesV4 index atoms varName) :: domains
    for bindings in enumerateBindingsV4 domains do
      if ← satisfiesFiniteDisjunctV4 index edges binding.query (bindingMapV4 bindings) atoms then
        denotation := denotation.push { disjunct := disjunctIndex, bindings }
    disjunctIndex := disjunctIndex + 1
  pure denotation

def sameBindingsV4
    (left right : Array FiniteQueryBindingV4) : Bool :=
  left.size == right.size &&
    left.all (fun binding =>
      right.any (fun candidate =>
        candidate.var == binding.var && candidate.entity == binding.entity))

def rowMatchesAssignmentV4
    (row : FiniteQueryRowV4)
    (assignment : FiniteRowAssignmentV4) : Bool :=
  row.disjunct == assignment.disjunct && sameBindingsV4 row.bindings assignment.bindings

def rowsExactlyDenotationV4
    (expected : Array FiniteRowAssignmentV4)
    (rows : Array FiniteQueryRowV4) : Bool :=
  expected.size == rows.size &&
    expected.all (fun assignment => rows.any (fun row => rowMatchesAssignmentV4 row assignment)) &&
    rows.all (fun row => expected.any (fun assignment => rowMatchesAssignmentV4 row assignment))

def finiteExactCompleteV4
    (index : AxiFiniteQueryIndexV4)
    (binding : PreparedQueryBindingV1)
    (rows : Array FiniteQueryRowV4) : Bool :=
  match finiteQueryDenotationV4 index binding with
  | .ok expected => rowsExactlyDenotationV4 expected rows
  | .error _ => false

def ExactFiniteCompletenessV4
    (index : AxiFiniteQueryIndexV4)
    (binding : PreparedQueryBindingV1)
    (rows : Array FiniteQueryRowV4) : Prop :=
  finiteExactCompleteV4 index binding rows = true

def ensureExactFiniteCompletenessV4
    (index : AxiFiniteQueryIndexV4)
    (binding : PreparedQueryBindingV1)
    (rows : Array FiniteQueryRowV4) : Except String Unit :=
  if finiteExactCompleteV4 index binding rows then
    pure ()
  else
    throw "query_result_v4 rows are not exactly equal to the bounded finite denotation"

theorem ensureExactFiniteCompletenessV4_sound
    (index : AxiFiniteQueryIndexV4)
    (binding : PreparedQueryBindingV1)
    (rows : Array FiniteQueryRowV4)
    (accepted : ensureExactFiniteCompletenessV4 index binding rows = .ok ()) :
    ExactFiniteCompletenessV4 index binding rows := by
  unfold ensureExactFiniteCompletenessV4 at accepted
  unfold ExactFiniteCompletenessV4
  cases exactEq : finiteExactCompleteV4 index binding rows with
  | false => simp [exactEq] at accepted
  | true => rfl

private def pushTextField (fields : Array ByteArray) (value : String) : Array ByteArray :=
  fields.push value.toUTF8

private def pushTermFieldsV1 (fields : Array ByteArray) : FiniteQueryTermV4 → Array ByteArray
  | .var name => pushTextField (pushTextField fields "term_var") name
  | .const entity => pushTextField (pushTextField fields "term_const") entity

partial def pushRegexFieldsV1 (fields : Array ByteArray) : FiniteQueryRegexV4 → Array ByteArray
  | .epsilon => pushTextField fields "regex_epsilon"
  | .rel rel => pushTextField (pushTextField fields "regex_rel") rel
  | .seq parts =>
      parts.foldl pushRegexFieldsV1
        (pushTextField (pushTextField fields "regex_seq") parts.size.repr)
  | .alt parts =>
      parts.foldl pushRegexFieldsV1
        (pushTextField (pushTextField fields "regex_alt") parts.size.repr)
  | .star inner => pushRegexFieldsV1 (pushTextField fields "regex_star") inner
  | .plus inner => pushRegexFieldsV1 (pushTextField fields "regex_plus") inner
  | .opt inner => pushRegexFieldsV1 (pushTextField fields "regex_opt") inner

partial def pushAtomFieldsV1 (fields : Array ByteArray) : FiniteQueryAtomV4 → Array ByteArray
  | .type term typeName =>
      pushTextField (pushTermFieldsV1 (pushTextField fields "atom_type") term) typeName
  | .attrEq term key value =>
      pushTextField
        (pushTextField (pushTermFieldsV1 (pushTextField fields "atom_attr_eq") term) key)
        value
  | .path left regex right =>
      pushTermFieldsV1
        (pushRegexFieldsV1 (pushTermFieldsV1 (pushTextField fields "atom_path") left) regex)
        right

def preparedQueryDigestFieldsV1 (binding : PreparedQueryBindingV1) : Array ByteArray :=
  Id.run do
    let mut fields : Array ByteArray := #[]
    fields := pushTextField fields "prepared_query_binding_v1"
    fields := pushTextField fields "1"
    fields := pushTextField fields "finite_exact_complete"
    fields := pushTextField fields "select_count"
    fields := pushTextField fields binding.query.selectVars.size.repr
    for selected in binding.query.selectVars do
      fields := pushTextField fields "select"
      fields := pushTextField fields selected
    fields := pushTextField fields "disjunct_count"
    fields := pushTextField fields binding.query.disjuncts.size.repr
    for index in [0:binding.query.disjuncts.size] do
      let disjunct := binding.query.disjuncts[index]!
      fields := pushTextField fields "disjunct"
      fields := pushTextField fields index.repr
      fields := pushTextField fields disjunct.size.repr
      for atom in disjunct do
        fields := pushAtomFieldsV1 fields atom
    match binding.query.maxHops? with
    | some maxHops =>
        fields := pushTextField fields "max_hops_some"
        fields := pushTextField fields maxHops.repr
    | none => fields := pushTextField fields "max_hops_none"
    match binding.query.minConfidence? with
    | some minConfidence =>
        fields := pushTextField fields "min_confidence_some"
        fields := pushTextField fields (Prob.toNat minConfidence).repr
    | none => fields := pushTextField fields "min_confidence_none"
    fields := pushTextField fields "row_limit"
    fields := pushTextField fields binding.rowLimit.repr
    return fields

def preparedQueryDigestV1 (binding : PreparedQueryBindingV1) : Except String String :=
  Axiograph.Identity.derive .query (preparedQueryDigestFieldsV1 binding)

def selectedProjectionV1 (query : FiniteQueryV4) (row : FiniteQueryRowV4) : Except String (Array FiniteQueryBindingV4) := do
  let mut projection : Array FiniteQueryBindingV4 := #[]
  for selected in query.selectVars do
    let matching := row.bindings.filter (fun binding => binding.var == selected)
    if matching.size != 1 then
      throw s!"selected projection requires exactly one `{selected}` binding"
    let some selectedBinding := matching[0]?
      | throw s!"selected projection is missing `{selected}`"
    projection := projection.push selectedBinding
  pure projection

def answerDigestV1
    (binding : PreparedQueryBindingV1)
    (preparedQueryDigest : String)
    (rows : Array FiniteQueryRowV4)
    (runtimeTruncated : Bool) : Except String String := do
  let mut fields : Array ByteArray := #[]
  fields := pushTextField fields "query_answer_v1"
  fields := pushTextField fields preparedQueryDigest
  fields := pushTextField fields "select_count"
  fields := pushTextField fields binding.query.selectVars.size.repr
  for selected in binding.query.selectVars do
    fields := pushTextField fields selected
  fields := pushTextField fields "row_count"
  fields := pushTextField fields rows.size.repr
  let mut rowIndex := 0
  for row in rows do
    fields := pushTextField fields "row"
    fields := pushTextField fields rowIndex.repr
    for projection in (← selectedProjectionV1 binding.query row) do
      fields := pushTextField fields projection.var
      fields := pushTextField fields projection.entity
    rowIndex := rowIndex + 1
  fields := pushTextField fields <|
    if runtimeTruncated then "runtime_truncated_true" else "runtime_truncated_false"
  Axiograph.Identity.derive .answer fields

def isLowerHex (value : String) : Bool :=
  value.toList.all (fun char =>
    ('0' ≤ char && char ≤ '9') || ('a' ≤ char && char ≤ 'f'))

def isWellFormedV2Identity (domain : Axiograph.Identity.Domain) (value : String) : Bool :=
  Axiograph.Identity.validWireFor domain value

structure QueryResultV4 where
  rowCount : Nat
  exactComplete : Bool
  preparedQueryDigest : String
  answerDigest : String
  claimKind : String
  deriving Repr

def verifyQueryResultProofV4Anchored
    (module : Axiograph.Axi.AxiV1.AxiV1Module)
    (expectedPreparedQueryDigest expectedAnswerDigest : String)
    (proof : QueryResultProofV4) : Except String QueryResultV4 := do
  if !isWellFormedV2Identity .query expectedPreparedQueryDigest then
    throw "expected prepared-query digest is malformed"
  if !isWellFormedV2Identity .answer expectedAnswerDigest then
    throw "expected answer digest is malformed"
  if proof.runtimeTruncated then
    throw "query_result_v4 exact finite completeness rejects truncated answers"
  validateFiniteQueryV4 proof.binding.query
  let preparedQueryDigest ← preparedQueryDigestV1 proof.binding
  if proof.preparedQueryDigest != preparedQueryDigest ||
      expectedPreparedQueryDigest != preparedQueryDigest then
    throw "prepared-query digest mismatch"
  if proof.rows.size > proof.binding.rowLimit then
    throw s!"certificate rows exceed row_limit: {proof.rows.size} > {proof.binding.rowLimit}"
  let index ← buildAxiFiniteQueryIndexV4 module
  validateFiniteExactFragmentV4 index proof.binding
  for row in proof.rows do
    verifyFiniteQueryRowV4Anchored index proof.binding.query row
  ensureExactFiniteCompletenessV4 index proof.binding proof.rows
  let answerDigest ← answerDigestV1
    proof.binding preparedQueryDigest proof.rows proof.runtimeTruncated
  if proof.answerDigest != answerDigest || expectedAnswerDigest != answerDigest then
    throw "answer digest mismatch"
  pure {
    rowCount := proof.rows.size
    exactComplete := true
    preparedQueryDigest
    answerDigest
    claimKind := proof.binding.claimKind
  }

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

namespace CategoryKernelCertificate

structure ResultV3 where
  schemaName : String
  objectCount : Nat
  arrowCount : Nat
  equationCount : Nat
  congruenceCertificateCount : Nat
  reachabilityEntryCount : Nat
  lifecycle : Theory.Finite.CheckedLifecycleState
  deriving Repr

def verifyV3 (module : Axiograph.Axi.AxiV1.AxiV1Module)
    (proof : CategoryKernelProofV3) :
    Except String ResultV3 := do
  let schemas := module.schemas.filter (fun schema => schema.name == proof.schemaName)
  if schemas.size != 1 then
    throw s!"category_kernel_v3: anchored module must contain exactly one schema `{proof.schemaName}`"
  let schema ←
    match schemas[0]? with
    | some schema => pure schema
    | none =>
        throw s!"category_kernel_v3: internal schema selection failure for `{proof.schemaName}`"
  let presentation ←
    match Theory.Finite.compileAxiSchemaPresentation module schema with
    | .ok presentation => pure presentation
    | .error residuals =>
        throw s!"category_kernel_v3: category formation failed: {repr residuals}"
  let expected ←
    match Theory.Finite.categoryKernelPresentationV3 presentation with
    | .ok expected => pure expected
    | .error residuals =>
        throw s!"category_kernel_v3: presentation export failed: {repr residuals}"
  if proof.presentation != expected then
    throw "category_kernel_v3: compiler presentation does not equal exact-byte Lean formation"
  match Theory.Finite.verifyCategoryKernelCongruenceV3
      expected proof.congruenceCertificates with
  | .ok _ => pure ()
  | .error residual =>
      throw s!"category_kernel_v3: congruence replay failed: {repr residual}"
  let checked ←
    match Theory.Finite.checkSaturationCertificate presentation proof.certificate with
    | .ok checked => pure checked
    | .error residual =>
        throw s!"category_kernel_v3: explanation replay failed: {repr residual}"
  pure {
    schemaName := proof.schemaName
    objectCount := presentation.core.objectNames.size
    arrowCount := presentation.core.arrows.size
    equationCount := presentation.equations.size
    congruenceCertificateCount := proof.congruenceCertificates.size
    reachabilityEntryCount := checked.certificate.entries.size
    lifecycle := checked.lifecycle
  }

end CategoryKernelCertificate

inductive CertificateResult where
  | reachabilityV3 (res : Query.ReachabilityResultV3)
  | categoryKernelV3 (res : CategoryKernelCertificate.ResultV3)
  | resolutionV2 (res : Resolution.ResolutionResultV2)
  | axiWellTypedV1 (res : AxiWellTypedProofV1)
  | axiConstraintsOkV1 (res : AxiConstraintsOkProofV1)
  | queryResultV4 (res : Query.QueryResultV4)
  | normalizePathV2 (res : PathNormalization.NormalizePathResultV2)
  | rewriteDerivationV2 (res : RewriteDerivation.RewriteDerivationResultV2)
  | rewriteDerivationV3 (res : RewriteDerivation.RewriteDerivationResultV3)
  | pathEquivV2 (res : PathEquivalence.PathEquivResultV2)
  | deltaFV1 (res : Migration.DeltaFMigrationResultV1)
  deriving Repr

def verifyCertificate : Certificate → Except String CertificateResult
  | .reachabilityV3 _ =>
      throw "reachability_v3 requires a canonical `.axi` module context; run `axiograph_verify <module.axi> <certificate.json>`"
  | .categoryKernelV3 _ =>
      throw "category_kernel_v3 requires an exact canonical `.axi` module anchor"
  | .resolutionV2 proof => do
      let res ← Resolution.verifyResolutionProofV2 proof
      pure (.resolutionV2 res)
  | .axiWellTypedV1 _ =>
      throw "axi_well_typed_v1 requires a canonical `.axi` module context; run `axiograph_verify <module.axi> <certificate.json>`"
  | .axiConstraintsOkV1 _ =>
      throw "axi_constraints_ok_v1 requires a canonical `.axi` module context; run `axiograph_verify <module.axi> <certificate.json>`"
  | .queryResultV4 _ =>
      throw "query_result_v4 requires a canonical `.axi` module plus caller-supplied prepared-query and answer expectations"
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
