import Std
import Std.Internal.Parsec

/-!
# Canonical `.axi` schema/theory/instance surface backing `axi_v1`

This module defines the canonical `.axi` schema/theory/instance surface used by
the corpus and the Lean-side checker:

- `examples/economics/EconomicFlows.axi`
- `examples/ontology/SchemaEvolution.axi`

The module name stays `SchemaV1` because it is the concrete AST behind
`axi_v1`, but contributors should think in terms of one canonical `.axi`
surface rather than multiple end-user dialects.

## Design goals

1. **Readable and auditable**: the trusted checker should be easy to review.
2. **Stable**: parse the canonical corpus in a deterministic way.
3. **Lockstep with Rust**: the Rust parser lives at
   `rust/crates/axiograph-dsl/src/schema_v1.rs` and should stay structurally
   aligned with this Lean module.

This parser is intentionally conservative: it fails fast on unrecognized *section lines*.
For theory constraints, we preserve unsupported forms as explicit `ConstraintV1.unknown`
(or other opaque variants) so tooling can surface and repair them without breaking parsing.
-/

namespace Axiograph.Axi.SchemaV1

abbrev Name : Type := String

-- =============================================================================
-- AST
-- =============================================================================

inductive RefinementPredicateV1 where
  | equals (value : Name)
  | memberOf (values : Array Name)
  | cardinality (min max : Nat)
  | key (roles : Array Name)
  | enum (values : Array Name)
  | predicate (name : Name) (args : Array Name)
deriving Repr, DecidableEq

inductive TypeExprV1 where
  | object (name : Name)
  | relationObject (relation : Name)
  | indexed (base : TypeExprV1) (overRoles : Array Name)
  | refined (base : TypeExprV1) (predicates : Array RefinementPredicateV1)
deriving Repr, DecidableEq

def TypeExprV1.referencedName : TypeExprV1 → Name
  | .object name => name
  | .relationObject relation => relation
  | .indexed base _ => base.referencedName
  | .refined base _ => base.referencedName

def TypeExprV1.relationObjectName? : TypeExprV1 → Option Name
  | .object _ => none
  | .relationObject relation => some relation
  | .indexed base _ => base.relationObjectName?
  | .refined base _ => base.relationObjectName?

inductive RoleKindV1 where
  | data
  | context
  | world
  | temporal
  | parameter
  | evidence
deriving Repr, DecidableEq

structure FieldDeclV1 where
  field : Name
  ty : TypeExprV1
  kind : RoleKindV1 := .data
deriving Repr, DecidableEq

structure RelationDeclV1 where
  name : Name
  fields : Array FieldDeclV1
deriving Repr, DecidableEq

inductive GeneratorKindV1 where
  | aspect
  | function
deriving Repr, DecidableEq

structure GeneratorDeclV1 where
  name : Name
  source : Name
  target : Name
  kind : GeneratorKindV1
  reversible : Bool := false
deriving Repr, DecidableEq

structure SubtypeDeclV1 where
  sub : Name
  sup : Name
  /-- Optional explicit inclusion morphism name.

  Preserved for now because some lowering paths still carry it, but it is not
  part of the preferred canonical authoring style. -/
  inclusion : Option Name
deriving Repr, DecidableEq

structure SchemaV1Schema where
  name : Name
  objects : Array Name
  subtypes : Array SubtypeDeclV1
  relations : Array RelationDeclV1
  generators : Array GeneratorDeclV1
deriving Repr, DecidableEq

/-!
Carrier fields for closure-style constraints
-------------------------------------------

Some theory constraints (e.g. symmetry / transitivity) describe a closure on a
*binary* relation. In `.axi` we typically treat the first two relation fields
as that "carrier pair", but when relations have extra fields (context/time,
evidence, witnesses) it is useful to explicitly name which two fields are the
endpoints of the closure operation.

Canonical surface syntax:

* `constraint symmetric Rel on (from, to)`
* `constraint transitive Rel on (from, to)`
-/
structure CarrierFieldsV1 where
  leftField : Name
  rightField : Name
deriving Repr, DecidableEq

inductive ConstraintV1 where
  | functional (relation srcField dstField : Name)
  | atMost (relation srcField dstField : Name) (max : Nat) (params : Option (Array Name))
  /-- A first-class typing rule annotation for a relation (metadata today). -/
  | typing (relation : Name) (rule : Name)
  /-- Conditional symmetry: only enforce symmetry for tuples whose `field` value is in `values`. -/
  | symmetricWhereIn (relation field : Name) (values : Array Name) (carriers : Option CarrierFieldsV1) (params : Option (Array Name))
  | symmetric (relation : Name) (carriers : Option CarrierFieldsV1) (params : Option (Array Name))
  | transitive (relation : Name) (carriers : Option CarrierFieldsV1) (params : Option (Array Name))
  | key (relation : Name) (fields : Array Name)
  /-- An opaque, named constraint block (preserved as structured data). -/
  | namedBlock (name : Name) (body : Array String)
  | unknown (text : String)
deriving Repr, DecidableEq

structure EquationV1 where
  name : Name
  lhs : String
  rhs : String
deriving Repr, DecidableEq

/-!
## Rewrite rules (structured, typed)

The canonical `.axi` surface language supports **first-class rewrite rules**
inside `theory` blocks.

Why structured rules (instead of free-form strings)?

* Rust can compile rules into efficient optimizers / evaluators.
* Lean can check certificates by replaying derivations step-by-step.
* Rules become part of the canonical accepted semantics (auditable).

This is the minimal v1 form:

* `vars` introduces typed variables
  - object variables: `x : Agent`
  - path variables: `p : Path(x,y)` or `p : Path x y`
* `lhs` / `rhs` are small path expressions (HoTT/groupoid-style constructors):
  - `refl(x)`
  - `step(x, rel, y)`
  - `trans(p, q)`
  - `inv(p)`
  - and path metavariables (bare identifiers like `p`)

This mirrors the Rust implementation in
`rust/crates/axiograph-dsl/src/schema_v1.rs`.
-/

inductive RewriteOrientationV1 where
  | forward
  | backward
  | bidirectional
deriving Repr, DecidableEq

inductive RewriteVarTypeV1 where
  | object (ty : Name)
  | path (src : Name) (dst : Name)
deriving Repr, DecidableEq

structure RewriteVarDeclV1 where
  name : Name
  ty : RewriteVarTypeV1
deriving Repr, DecidableEq

inductive PathExprV3 where
  /-- Path metavariable (used in rewrite rule patterns). -/
  | var (name : Name)
  | reflexive (entity : Name)
  | step (src : Name) (rel : Name) (dst : Name)
  | trans (left : PathExprV3) (right : PathExprV3)
  | inv (path : PathExprV3)
deriving Repr, DecidableEq

structure RewriteRuleV1 where
  name : Name
  orientation : RewriteOrientationV1 := .forward
  vars : Array RewriteVarDeclV1
  lhs : PathExprV3
  rhs : PathExprV3
deriving Repr, DecidableEq

structure SchemaV1Theory where
  name : Name
  schema : Name
  constraints : Array ConstraintV1
  equations : Array EquationV1
  rewriteRules : Array RewriteRuleV1
deriving Repr, DecidableEq

inductive SetItemV1 where
  | ident (name : Name)
  | tuple (label : Option Name) (fields : Array (Name × Name))
deriving Repr, DecidableEq

structure SetLiteralV1 where
  items : Array SetItemV1
deriving Repr, DecidableEq

structure InstanceAssignmentV1 where
  name : Name
  value : SetLiteralV1
deriving Repr, DecidableEq

structure SchemaV1Instance where
  name : Name
  schema : Name
  assignments : Array InstanceAssignmentV1
deriving Repr, DecidableEq

structure SchemaV1Module where
  moduleName : Name
  imports : Array Name
  schemas : Array SchemaV1Schema
  theories : Array SchemaV1Theory
  instances : Array SchemaV1Instance
deriving Repr, DecidableEq

-- =============================================================================
-- Parser utilities
-- =============================================================================

structure ParseError where
  line : Nat
  message : String
deriving Repr, DecidableEq

inductive Section where
  | none
  | schema (index : Nat)
  | theory (index : Nat)
  | instance (index : Nat)
deriving Repr, DecidableEq

structure ParseState where
moduleAst : SchemaV1Module
currentSection : Section
moduleHeaderLine : Option Nat
deriving Repr

def emptyModule : SchemaV1Module :=
{ moduleName := "Unnamed", imports := #[], schemas := #[], theories := #[], instances := #[] }

def failAt {α : Type} (line : Nat) (message : String) : Except ParseError α :=
  throw { line, message }

def trimTrailingColon (s : String) : String :=
  let trimmed := s.trimAscii.toString
  match trimmed.toList.reverse with
  | ':' :: restRev => String.ofList restRev.reverse
  | _ => trimmed

def findCommentIndex (chars : List Char) : Option Nat :=
  let rec go (i : Nat) : List Char → Option Nat
    | [] => none
    | '#' :: _ => some i
    | '-' :: '-' :: _ => some i
    | _ :: rest => go (i + 1) rest
  go 0 chars

def stripComment (line : String) : String :=
  match findCommentIndex line.toList with
  | none => line
  | some idx => String.ofList (line.toList.take idx)

def splitOnceChar (s : String) (separator : Char) : Option (String × String) :=
  let rec go (accRev : List Char) : List Char → Option (String × String)
    | [] => none
    | c :: cs =>
        if c == separator then
          some (String.ofList accRev.reverse, String.ofList cs)
        else
          go (c :: accRev) cs
  go [] s.toList

def stripPrefix? (s : String) (prefixText : String) : Option String :=
  let sChars := s.toList
  let pChars := prefixText.toList
  if sChars.take pChars.length == pChars then
    some (String.ofList (sChars.drop pChars.length))
  else
    none

def startsWith (s : String) (prefixText : String) : Bool :=
  (stripPrefix? s prefixText).isSome

def updateAt? (arr : Array α) (index : Nat) (f : α → α) : Option (Array α) :=
  if h : index < arr.size then
    some (arr.set index (f (arr[index]'h)) h)
  else
    none

-- =============================================================================
-- Parsec helpers (Lean stdlib)
-- =============================================================================

/-!
We use Lean's built-in Parsec combinators (`Std.Internal.Parsec`) for the
non-trivial line grammars in `.axi`.

This is the same parsing framework Lean uses internally for e.g. JSON/XML, so
it's a good "best in class" choice that does not add extra dependencies.
-/

open Std.Internal.Parsec
open Std.Internal.Parsec.String

abbrev LineParser (α : Type) : Type := Std.Internal.Parsec.String.Parser α

def dropOffsetPrefix (s : String) : String :=
  match s.splitOn ": " with
  | [] => s
  | [_] => s
  | _prefix :: rest => String.intercalate ": " rest

def runLineParser (p : LineParser α) (input : String) : Except String α :=
  match Parser.run (p <* ws <* eof) input with
  | .ok v => .ok v
  | .error err => .error (dropOffsetPrefix err)

def ws1 : LineParser Unit := do
  let _ ← many1 (satisfy (fun c => c.isWhitespace))
  pure ()

def isIdentStart (c : Char) : Bool :=
  c.isAlpha || c == '_'

def isIdentContinue (c : Char) : Bool :=
  c.isAlphanum || c == '_'

def identifier : LineParser Name := do
  let first ← satisfy isIdentStart
  let rest ← many (satisfy isIdentContinue)
  pure <| String.ofList (first :: rest.toList)

def valueAtom : LineParser Name := do
  let chars ← many1 (satisfy (fun c =>
    !c.isWhitespace && c != ',' && c != '(' && c != ')' && c != '{' &&
      c != '}' && c != '=' && c != ':'))
  pure <| String.ofList chars.toList

def natLiteral : LineParser Nat := do
  let digits ← many1 (satisfy Char.isDigit)
  let s := String.ofList digits.toList
  match s.toNat? with
  | some n => pure n
  | none => fail "expected a natural number"

partial def sepBy1Core (p : LineParser α) (sep : LineParser Unit) (acc : Array α) : LineParser (Array α) :=
  (attempt do
    let _ ← sep
    let next ← p
    sepBy1Core p sep (acc.push next)) <|> pure acc

partial def sepBy1 (p : LineParser α) (sep : LineParser Unit) : LineParser (Array α) := do
  let first ← p
  sepBy1Core p sep #[first]

def sepBy (p : LineParser α) (sep : LineParser Unit) : LineParser (Array α) :=
  sepBy1 p sep <|> pure #[]

-- =============================================================================
-- Header parsers
-- =============================================================================

def parseTheoryHeader (rest : String) : Except String (Name × Name) := do
  let p : LineParser (Name × Name) := do
    ws
    let name ← identifier
    ws1
    skipString "on"
    ws1
    let schema ← identifier
    ws
    ((skipChar ':' *> pure ()) <|> pure ())
    pure (name, schema)
  match runLineParser p rest with
  | .ok v => pure v
  | .error _ => throw "theory header expects: `theory <Name> on <Schema>:`"

def parseInstanceHeader (rest : String) : Except String (Name × Name) := do
  let p : LineParser (Name × Name) := do
    ws
    let name ← identifier
    ws1
    skipString "of"
    ws1
    let schema ← identifier
    ws
    ((skipChar ':' *> pure ()) <|> pure ())
    pure (name, schema)
  match runLineParser p rest with
  | .ok v => pure v
  | .error _ => throw "instance header expects: `instance <Name> of <Schema>:`"

-- =============================================================================
-- Schema section parsers
-- =============================================================================

def parseSubtypeDecl (rest : String) : Except String SubtypeDeclV1 := do
  let p : LineParser SubtypeDeclV1 := do
    ws
    let sub ← identifier
    ws1
    (skipString "<:" <|> (skipChar '<' *> pure ()))
    ws1
    let sup ← identifier
    let inclusion ←
      (attempt do
        ws1
        skipString "as"
        ws1
        some <$> identifier) <|> pure none
    pure { sub, sup, inclusion }
  match runLineParser p rest with
  | .ok v => pure v
  | .error msg => throw msg

def commaParser : LineParser Unit := do
  ws
  skipChar ','
  ws

def pipeParser : LineParser Unit := do
  ws
  skipChar '|'
  ws

def semicolonParser : LineParser Unit := do
  ws
  skipChar ';'
  ws

partial def refinementPredicateParser : LineParser RefinementPredicateV1 :=
  (attempt do
    skipString "eq("
    let value ← identifier
    skipChar ')'
    pure (.equals value)) <|>
  (attempt do
    skipString "in("
    let values ← sepBy1 identifier pipeParser
    skipChar ')'
    pure (.memberOf values)) <|>
  (attempt do
    skipString "enum("
    let values ← sepBy1 identifier pipeParser
    skipChar ')'
    pure (.enum values)) <|>
  (attempt do
    skipString "key("
    let roles ← sepBy1 identifier pipeParser
    skipChar ')'
    pure (.key roles)) <|>
  (attempt do
    skipString "cardinality("
    let min ← natLiteral
    pipeParser
    let max ← natLiteral
    skipChar ')'
    if min ≤ max then pure (.cardinality min max)
    else fail "cardinality minimum exceeds maximum") <|>
  (attempt do
    skipString "predicate("
    let names ← sepBy1 identifier pipeParser
    skipChar ')'
    match names[0]? with
    | none => fail "predicate must name a supported predicate"
    | some name => pure (.predicate name (names.extract 1 names.size)))

partial def typeExprParser : LineParser TypeExprV1 :=
  (attempt do
    skipString "relation("
    let relation ← identifier
    skipChar ')'
    pure (.relationObject relation)) <|>
  (attempt do
    skipString "indexed("
    let base ← typeExprParser
    semicolonParser
    let roles ← sepBy1 identifier pipeParser
    skipChar ')'
    pure (.indexed base roles)) <|>
  (attempt do
    skipString "refined("
    let base ← typeExprParser
    semicolonParser
    let predicates ← sepBy1 refinementPredicateParser semicolonParser
    skipChar ')'
    pure (.refined base predicates)) <|>
  (.object <$> identifier)

def roleKindParser : LineParser RoleKindV1 := do
  skipChar '@'
  let annotation ← identifier
  match annotation with
  | "data" => pure .data
  | "context" => pure .context
  | "world" => pure .world
  | "temporal" => pure .temporal
  | "parameter" => pure .parameter
  | "evidence" => pure .evidence
  | _ => fail s!"unknown role annotation @{annotation}"

def parseRelationDecl (line : String) : Except String RelationDeclV1 := do
  let fieldDecl : LineParser FieldDeclV1 := do
    ws
    let field ← identifier
    ws
    skipChar ':'
    ws
    let ty ← typeExprParser
    let kind ← (attempt (ws1 *> roleKindParser)) <|> pure .data
    pure { field, ty, kind }

  let p : LineParser RelationDeclV1 := do
    ws
    skipString "relation"
    ws1
    let name ← identifier
    ws
    skipChar '('
    let fields ← sepBy1 fieldDecl commaParser
    ws
    skipChar ')'
    pure { name, fields }

  match runLineParser p line with
  | .ok v => pure v
  | .error _ => throw "relation expects exactly `relation Name(role: Type @kind, ...)`; relation-level axis shorthands are not canonical"

def parseGeneratorDecl (line : String) : Except String GeneratorDeclV1 := do
  let p : LineParser GeneratorDeclV1 := do
    ws
    let kind ←
      (skipString "aspect" *> pure GeneratorKindV1.aspect) <|>
      (skipString "function" *> pure GeneratorKindV1.function)
    ws1
    let name ← identifier
    ws
    skipChar ':'
    ws
    let source ← identifier
    ws
    skipString "->"
    ws
    let target ← identifier
    let reversible ←
      (attempt do ws1; skipString "@reversible"; pure true) <|> pure false
    pure { name, source, target, kind, reversible }
  runLineParser p line

-- =============================================================================
-- Theory section parsers
-- =============================================================================

def parseConstraint (rest : String) : Except String ConstraintV1 := do
  let trimmed := rest.trimAscii.toString

  let relField : LineParser (Name × Name) := do
    let rel ← identifier
    skipChar '.'
    let field ← identifier
    pure (rel, field)

  if startsWith trimmed "functional " then
    let p : LineParser ConstraintV1 := do
      skipString "functional"
      ws1
      let (rel1, srcField) ← relField
      ws
      skipString "->"
      ws
      let (rel2, dstField) ← relField
      if rel1 == rel2 then
        pure (.functional rel1 srcField dstField)
      else
        -- Keep parsing robust across dialect variations. Rust treats mismatched
        -- relation references as an unknown constraint instead of failing the
        -- entire module parse.
        pure (.unknown trimmed)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error _msg =>
        -- Some non-canonical `.axi` sources use more declarative forms like:
        --
        --   `constraint functional Rel(field0, field1, ...)`
        --   `constraint functional Rel(field0, ...) -> Rel.someOutput`
        --
        -- These are *not canonical* in axi_v1 today. Prefer:
        -- - `constraint functional Rel.field -> Rel.field` for unary FDs, and
        -- - `constraint key Rel(field0, field1, ...)` for multi-field determinism.
        --
        -- For now we keep parsing robust (and keep the text visible) without
        -- making these dialect forms part of the trusted core.
        return (.unknown trimmed)
  else if startsWith trimmed "at_most " then
    let comma : LineParser Unit := do
      ws
      skipChar ','
      ws
      pure ()
    let paramClauseP : LineParser (Array Name) := do
      ws1
      skipString "param"
      ws1
      skipChar '('
      ws
      let xs ← sepBy1 identifier comma
      ws
      skipChar ')'
      pure xs
    let p : LineParser ConstraintV1 := do
      skipString "at_most"
      ws1
      let max ← natLiteral
      ws1
      let (rel1, srcField) ← relField
      ws
      skipString "->"
      ws
      let (rel2, dstField) ← relField
      let params ← (attempt (some <$> paramClauseP)) <|> pure none
      if rel1 == rel2 then
        pure (.atMost rel1 srcField dstField max params)
      else
        pure (.unknown trimmed)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error _msg =>
        return (.unknown trimmed)
  else if startsWith trimmed "typing " then
    let p : LineParser ConstraintV1 := do
      skipString "typing"
      ws1
      let relation ← identifier
      ws
      skipChar ':'
      ws
      let rule ← identifier
      pure (.typing relation rule)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error _msg =>
        -- Keep parsing robust across dialect variations.
        return (.unknown trimmed)
  else if startsWith trimmed "symmetric " then
    let comma : LineParser Unit := do
      ws
      skipChar ','
      ws
      pure ()
    let nameSet : LineParser (Array Name) := do
      skipChar '{'
      ws
      let xs ← sepBy1 identifier comma
      ws
      skipChar '}'
      pure xs
    let onClauseP : LineParser CarrierFieldsV1 := do
      ws1
      skipString "on"
      ws1
      skipChar '('
      ws
      let left ← identifier
      ws
      skipChar ','
      ws
      let right ← identifier
      ws
      skipChar ')'
      pure { leftField := left, rightField := right }
    let paramClauseP : LineParser (Array Name) := do
      ws1
      skipString "param"
      ws1
      skipChar '('
      ws
      let xs ← sepBy1 identifier comma
      ws
      skipChar ')'
      pure xs
    let closureClausesP : LineParser (Option CarrierFieldsV1 × Option (Array Name)) := do
      let clause : LineParser (Sum CarrierFieldsV1 (Array Name)) :=
        (attempt do
            let on ← onClauseP
            pure (Sum.inl on))
        <|> (attempt do
            let ps ← paramClauseP
            pure (Sum.inr ps))
      let clauses ← many clause
      let mut carriers : Option CarrierFieldsV1 := none
      let mut params : Option (Array Name) := none
      for c in clauses do
        match c with
        | .inl on =>
            if carriers.isSome then
              fail "duplicate `on (...)` clause in constraint"
            carriers := some on
        | .inr ps =>
            if params.isSome then
              fail "duplicate `param (...)` clause in constraint"
            params := some ps
      pure (carriers, params)

    let p : LineParser ConstraintV1 := do
      skipString "symmetric"
      ws1
      let relation ← identifier
      -- Optional guard:
      --   `where Rel.field in {A, B, ...}` (canonical)
      --   `where field in {A, B, ...}` (shorthand; formatter expands)
      let guarded : LineParser ConstraintV1 := do
        ws1
        skipString "where"
        ws1
        let (rel2, field) ← (attempt relField) <|> (do
          let field ← identifier
          pure (relation, field))
        if rel2 != relation then
          pure (.unknown trimmed)
        else
          ws1
          skipString "in"
          ws1
          let values ← nameSet
          let (carriers, params) ← closureClausesP
          pure (.symmetricWhereIn relation field values carriers params)

      let unguarded : LineParser ConstraintV1 := do
        let (carriers, params) ← closureClausesP
        pure (.symmetric relation carriers params)

      (attempt guarded) <|> unguarded
    match runLineParser p trimmed with
    | .ok v => return v
    | .error _msg =>
        -- Keep parsing robust across dialect variations.
        return (.unknown trimmed)
  else if startsWith trimmed "transitive " then
    let comma : LineParser Unit := do
      ws
      skipChar ','
      ws
      pure ()
    let onClauseP : LineParser CarrierFieldsV1 := do
      ws1
      skipString "on"
      ws1
      skipChar '('
      ws
      let left ← identifier
      ws
      skipChar ','
      ws
      let right ← identifier
      ws
      skipChar ')'
      pure { leftField := left, rightField := right }
    let paramClauseP : LineParser (Array Name) := do
      ws1
      skipString "param"
      ws1
      skipChar '('
      ws
      let xs ← sepBy1 identifier comma
      ws
      skipChar ')'
      pure xs
    let closureClausesP : LineParser (Option CarrierFieldsV1 × Option (Array Name)) := do
      let clause : LineParser (Sum CarrierFieldsV1 (Array Name)) :=
        (attempt do
            let on ← onClauseP
            pure (Sum.inl on))
        <|> (attempt do
            let ps ← paramClauseP
            pure (Sum.inr ps))
      let clauses ← many clause
      let mut carriers : Option CarrierFieldsV1 := none
      let mut params : Option (Array Name) := none
      for c in clauses do
        match c with
        | .inl on =>
            if carriers.isSome then
              fail "duplicate `on (...)` clause in constraint"
            carriers := some on
        | .inr ps =>
            if params.isSome then
              fail "duplicate `param (...)` clause in constraint"
            params := some ps
      pure (carriers, params)
    let p : LineParser ConstraintV1 := do
      skipString "transitive"
      ws1
      let relation ← identifier
      let (carriers, params) ← closureClausesP
      pure (.transitive relation carriers params)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error _msg =>
        -- Keep parsing robust across dialect variations like:
        --
        --   `constraint transitive Rel where ...`
        --
        -- We keep the text visible as an unknown constraint so downstream tools
        -- can surface/repair it, without failing the entire module parse.
        return (.unknown trimmed)
  else if startsWith trimmed "key " then
    let comma : LineParser Unit := do
      ws
      skipChar ','
      ws
    let p : LineParser ConstraintV1 := do
      skipString "key"
      ws1
      let relation ← identifier
      ws
      skipChar '('
      let fieldNames ← sepBy1 identifier comma
      ws
      skipChar ')'
      pure (.key relation fieldNames)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error msg => throw msg
  else
    pure (.unknown trimmed)

-- =============================================================================
-- Block collectors
-- =============================================================================

def isTopLevelKeyword (trimmed : String) : Bool :=
  startsWith trimmed "schema "
    || startsWith trimmed "theory "
    || startsWith trimmed "instance "
    || startsWith trimmed "module "
    || startsWith trimmed "import "
    || startsWith trimmed "aspect "
    || startsWith trimmed "function "
    || startsWith trimmed "constraint "
    || startsWith trimmed "equation "
    || startsWith trimmed "rewrite "

def collectIndentedBlock (lines : Array String) (startIndex : Nat) : (String × Nat) :=
  Id.run do
    let mut out : Array String := #[]
    let mut i := startIndex
    while _h : i < lines.size do
      let trimmed := stripComment (lines[i]!) |>.trimAscii.toString
      if trimmed.isEmpty then
        i := i + 1
        continue
      if isTopLevelKeyword trimmed then
        break
      out := out.push trimmed
      i := i + 1
    pure (String.intercalate " " out.toList, i)

def collectIndentedBlockLines (lines : Array String) (startIndex : Nat) : (Array String × Nat) :=
  Id.run do
    let mut out : Array String := #[]
    let mut i := startIndex
    while _h : i < lines.size do
      let trimmed := stripComment (lines[i]!) |>.trimAscii.toString
      if trimmed.isEmpty then
        i := i + 1
        continue
      if isTopLevelKeyword trimmed then
        break
      out := out.push trimmed
      i := i + 1
    pure (out, i)

-- =============================================================================
-- Rewrite rule parsers (inline in theory blocks)
-- =============================================================================

def parseRewriteOrientation (s : String) : Except String RewriteOrientationV1 := do
  match s.trimAscii.toString with
  | "forward" => pure .forward
  | "backward" => pure .backward
  | "bidirectional" | "both" => pure .bidirectional
  | other => throw s!"unknown rewrite orientation `{other}` (expected forward|backward|bidirectional)"

def parseRewriteVarDeclList (line : String) : Except String (Array RewriteVarDeclV1) := do
  let comma : LineParser Unit := do
    ws
    skipChar ','
    ws

  let pathTypeParens : LineParser (Name × Name) := do
    skipChar '('
    ws
    let srcName ← identifier
    ws
    skipChar ','
    ws
    let dstName ← identifier
    ws
    skipChar ')'
    pure (srcName, dstName)

  let pathTypeWords : LineParser (Name × Name) := do
    ws1
    let srcName ← identifier
    ws1
    let dstName ← identifier
    pure (srcName, dstName)

  let varType : LineParser RewriteVarTypeV1 :=
    (attempt do
      skipString "Path"
      ws
      let (srcName, dstName) ← (attempt pathTypeParens) <|> pathTypeWords
      pure (.path srcName dstName)) <|> do
        let ty ← identifier
        pure (.object ty)

  let varDecl : LineParser RewriteVarDeclV1 := do
    ws
    let name ← identifier
    ws
    skipChar ':'
    ws
    let ty ← varType
    pure { name, ty }

  let p : LineParser (Array RewriteVarDeclV1) := do
    ws
    let decls ← sepBy1 varDecl comma <|> pure #[]
    pure decls

  match runLineParser p line with
  | .ok v => pure v
  | .error msg => throw msg

def commaWs : LineParser Unit := do
  ws
  skipChar ','
  ws

partial def pathExprV3Parser : LineParser PathExprV3 := do
  ws
  (attempt reflExpr) <|> (attempt stepExpr) <|> (attempt transExpr) <|> (attempt invExpr) <|> varExpr
where
  varExpr : LineParser PathExprV3 := do
    let name ← identifier
    pure (.var name)

  reflExpr : LineParser PathExprV3 := do
    (skipString "refl" <|> skipString "id")
    ws
    skipChar '('
    ws
    let entity ← identifier
    ws
    skipChar ')'
    pure (.reflexive entity)

  stepExpr : LineParser PathExprV3 := do
    skipString "step"
    ws
    skipChar '('
    ws
    let src ← identifier
    let _ ← commaWs
    let rel ← identifier
    let _ ← commaWs
    let dst ← identifier
    ws
    skipChar ')'
    pure (.step src rel dst)

  transExpr : LineParser PathExprV3 := do
    skipString "trans"
    ws
    skipChar '('
    let left ← pathExprV3Parser
    let _ ← commaWs
    let right ← pathExprV3Parser
    ws
    skipChar ')'
    pure (.trans left right)

  invExpr : LineParser PathExprV3 := do
    skipString "inv"
    ws
    skipChar '('
    let p ← pathExprV3Parser
    ws
    skipChar ')'
    pure (.inv p)

def parsePathExprV3FromString (text : String) : Except String PathExprV3 := do
  match runLineParser pathExprV3Parser text with
  | .ok v => pure v
  | .error msg => throw msg

inductive RewriteRuleBlockField where
  | none
  | vars
  | lhs
  | rhs
  | orientation
deriving Repr, DecidableEq

def parseRewriteRuleBlock (ruleName : Name) (lines : Array String) : Except String RewriteRuleV1 := do
  let mut current : RewriteRuleBlockField := .none
  let mut varsLines : Array String := #[]
  let mut lhsLines : Array String := #[]
  let mut rhsLines : Array String := #[]
  let mut orientation? : Option RewriteOrientationV1 := none

  for raw in lines do
    let line := raw.trimAscii.toString
    if line.isEmpty then
      continue

    if let some rest := stripPrefix? line "vars:" then
      current := .vars
      let rest := rest.trimAscii.toString
      if !rest.isEmpty then
        varsLines := varsLines.push rest
      continue

    if let some rest := stripPrefix? line "lhs:" then
      current := .lhs
      let rest := rest.trimAscii.toString
      if !rest.isEmpty then
        lhsLines := lhsLines.push rest
      continue

    if let some rest := stripPrefix? line "rhs:" then
      current := .rhs
      let rest := rest.trimAscii.toString
      if !rest.isEmpty then
        rhsLines := rhsLines.push rest
      continue

    if let some rest := stripPrefix? line "orientation:" then
      current := .orientation
      let rest := rest.trimAscii.toString
      if !rest.isEmpty then
        orientation? := some (← parseRewriteOrientation rest)
        current := .none
      continue

    match current with
    | .vars => varsLines := varsLines.push line
    | .lhs => lhsLines := lhsLines.push line
    | .rhs => rhsLines := rhsLines.push line
    | .orientation =>
        orientation? := some (← parseRewriteOrientation line)
        current := .none
    | .none =>
        throw s!"rewrite `{ruleName}`: unexpected line (expected vars/lhs/rhs): `{line}`"

  let mut vars : Array RewriteVarDeclV1 := #[]
  for vLine in varsLines do
    vars := vars ++ (← parseRewriteVarDeclList vLine)

  let lhsText := String.intercalate " " lhsLines.toList
  let rhsText := String.intercalate " " rhsLines.toList
  if lhsText.trimAscii.toString.isEmpty then
    throw s!"rewrite `{ruleName}`: missing `lhs:`"
  if rhsText.trimAscii.toString.isEmpty then
    throw s!"rewrite `{ruleName}`: missing `rhs:`"

  let lhs ← parsePathExprV3FromString lhsText
  let rhs ← parsePathExprV3FromString rhsText

  pure {
    name := ruleName
    orientation := orientation?.getD .forward
    vars := vars
    lhs := lhs
    rhs := rhs
  }

def splitEquation (equationText : String) : Except String (String × String) := do
  match splitOnceChar equationText '=' with
  | some (lhs, rhs) =>
      let lhs := lhs.trimAscii.toString
      let rhs := rhs.trimAscii.toString
      if lhs.isEmpty || rhs.isEmpty then
        throw "equation must have non-empty lhs and rhs"
      pure (lhs, rhs)
  | none =>
      throw "equation body must contain `=`"

def splitAssignment (line : String) : Option (String × String) :=
  match splitOnceChar line '=' with
  | some (lhs, rhs) =>
      let lhs := lhs.trimAscii.toString
      let rhs := rhs.trimAscii.toString
      if lhs.isEmpty || rhs.isEmpty then none else some (lhs, rhs)
  | none => none

def adjustParenDepth (depth : Int) (line : String) : Except String Int :=
  let rec go (depth : Int) : List Char → Except String Int
    | [] => pure depth
    | '(' :: cs => go (depth + 1) cs
    | ')' :: cs =>
        if depth <= 0 then
          throw "unbalanced `)`"
        else
          go (depth - 1) cs
    | _ :: cs => go depth cs
  go depth line.toList

partial def collectBalancedParens (lines : Array String) (startIndex : Nat) (keyword : String) :
    Except String (String × Nat) := do
  let mut depth : Int := 0
  let mut combined : Array String := #[]
  let mut i := startIndex

  while _h : i < lines.size do
    let line := stripComment (lines[i]!) |>.trimAscii.toString
    if line.isEmpty then
      i := i + 1
      continue
    if combined.isEmpty && !startsWith line keyword then
      throw s!"expected `{keyword}` declaration"

    combined := combined.push line
    depth ← adjustParenDepth depth line

    i := i + 1
    if depth == 0 && !combined.isEmpty then
      break

  if depth != 0 then
    throw "unclosed parenthesis block"
  pure (String.intercalate " " combined.toList, i)

def adjustBraceDepth (depth : Int) (line : String) : Except String Int :=
  let rec go (depth : Int) : List Char → Except String Int
    | [] => pure depth
    | '{' :: cs => go (depth + 1) cs
    | '}' :: cs =>
        if depth <= 0 then
          throw "unbalanced `}`"
        else
          go (depth - 1) cs
    | _ :: cs => go depth cs
  go depth line.toList

partial def collectBalancedBraces (lines : Array String) (startIndex : Nat) (firstRhs : String) :
    Except String (String × Nat) := do
  let mut combined : Array String := #[]
  let mut depth : Int := 0

  let rhs := stripComment firstRhs |>.trimAscii.toString
  combined := combined.push rhs
  depth ← adjustBraceDepth depth rhs

  let mut i := startIndex + 1
  while _h : i < lines.size do
    if depth <= 0 then
      break
    let line := stripComment (lines[i]!) |>.trimAscii.toString
    if !line.isEmpty then
      combined := combined.push line
      depth ← adjustBraceDepth depth line
    i := i + 1

  if depth != 0 then
    throw "unclosed `{ ... }` block"
  pure (String.intercalate " " combined.toList, i)

-- =============================================================================
-- Instance literals
-- =============================================================================

def parseSetLiteral (text : String) : Except String SetLiteralV1 := do
  let comma : LineParser Unit := do
    ws
    skipChar ','
    ws

  let tupleField : LineParser (Name × Name) := do
    ws
    let key ← identifier
    ws
    skipChar '='
    ws
    let value ← valueAtom
    pure (key, value)

  let tupleBody : LineParser (Array (Name × Name)) := do
    skipChar '('
    let fields ← sepBy1 tupleField comma
    ((attempt comma) <|> pure ())
    ws
    skipChar ')'
    pure fields

  let labeledTupleItem : LineParser SetItemV1 := do
    let label ← identifier
    ws
    skipChar ':'
    ws
    let fields ← tupleBody
    pure (.tuple (some label) fields)

  let tupleItem : LineParser SetItemV1 := do
    let fields ← tupleBody
    pure (.tuple none fields)

  let setItem : LineParser SetItemV1 :=
    (attempt labeledTupleItem) <|> (attempt tupleItem) <|>
    (attempt do let value ← natLiteral; pure (.ident (toString value))) <|> do
      let name ← valueAtom
      pure (.ident name)

  let p : LineParser SetLiteralV1 := do
    ws
    skipChar '{'
    ws
    let items ← sepBy setItem comma
    ((attempt comma) <|> pure ())
    ws
    skipChar '}'
    pure { items }

  match runLineParser p text with
  | .ok v => pure v
  | .error msg => throw msg

-- =============================================================================
-- Top-level parse
-- =============================================================================

partial def parseSchemaV1 (text : String) : Except ParseError SchemaV1Module := do
  let lines : Array String := text.splitOn "\n" |>.toArray
  let mut state : ParseState := {
    moduleAst := emptyModule
    currentSection := .none
    moduleHeaderLine := none
  }
  let mut i : Nat := 0

  while _h : i < lines.size do
    let lineNo := i + 1
    let line := stripComment (lines[i]!) |>.trimAscii.toString

    if line.isEmpty then
      i := i + 1
      continue

    -- ----------------------------------------------------------------------
    -- Section headers
    -- ----------------------------------------------------------------------
    if let some name := stripPrefix? line "module " then
      let moduleName ←
        match runLineParser identifier name.trimAscii.toString with
        | .ok value => pure value
        | .error _ => return (← failAt lineNo "module header expects exactly `module <Name>`")
      match state.moduleHeaderLine with
      | some firstLine =>
          return (← failAt lineNo s!"canonical .axi input requires exactly one module header; first header was on line {firstLine}")
      | none =>
          state := {
            state with
            moduleAst := { state.moduleAst with moduleName }
            moduleHeaderLine := some lineNo
            currentSection := .none
          }
      i := i + 1
      continue

    if let some importText := stripPrefix? line "import " then
      if state.moduleHeaderLine.isNone || state.currentSection != .none then
        return (← failAt lineNo "imports must follow the module header and precede all sections")
      let importName ←
        match runLineParser identifier importText.trimAscii.toString with
        | .ok value => pure value
        | .error _ => return (← failAt lineNo "import expects exactly `import <Module>`")
      if state.moduleAst.imports.contains importName then
        return (← failAt lineNo s!"duplicate import `{importName}`")
      state := { state with moduleAst := { state.moduleAst with imports := state.moduleAst.imports.push importName } }
      i := i + 1
      continue

    if let some rest := stripPrefix? line "schema " then
      let schemaName := trimTrailingColon rest
      if schemaName.isEmpty then
        return (← failAt lineNo "schema name missing")
      let schema : SchemaV1Schema := { name := schemaName, objects := #[], subtypes := #[], relations := #[], generators := #[] }
      let newIndex := state.moduleAst.schemas.size
      state :=
        { state with
          moduleAst := { state.moduleAst with schemas := state.moduleAst.schemas.push schema }
          currentSection := .schema newIndex }
      i := i + 1
      continue

    if let some rest := stripPrefix? line "theory " then
      let (name, schema) ←
        match parseTheoryHeader rest with
        | .ok v => pure v
        | .error msg => return (← failAt lineNo msg)
      let theory : SchemaV1Theory := { name, schema, constraints := #[], equations := #[], rewriteRules := #[] }
      let newIndex := state.moduleAst.theories.size
      state :=
        { state with
          moduleAst := { state.moduleAst with theories := state.moduleAst.theories.push theory }
          currentSection := .theory newIndex }
      i := i + 1
      continue

    if let some rest := stripPrefix? line "instance " then
      let (name, schema) ←
        match parseInstanceHeader rest with
        | .ok v => pure v
        | .error msg => return (← failAt lineNo msg)
      let instanceAst : SchemaV1Instance := { name, schema, assignments := #[] }
      let newIndex := state.moduleAst.instances.size
      state :=
        { state with
          moduleAst := { state.moduleAst with instances := state.moduleAst.instances.push instanceAst }
          currentSection := .instance newIndex }
      i := i + 1
      continue

    -- ----------------------------------------------------------------------
    -- Section bodies
    -- ----------------------------------------------------------------------
    match state.currentSection with
    | .none =>
        return (← failAt lineNo s!"line outside any section: {line}")

    | .schema schemaIndex =>
        if let some name := stripPrefix? line "object " then
          let objectName := name.trimAscii.toString
          if objectName.isEmpty then
            return (← failAt lineNo "object name missing")
          let some schemas :=
            updateAt? state.moduleAst.schemas schemaIndex (fun s =>
              { s with objects := s.objects.push objectName })
            | return (← failAt lineNo "internal error: schema index out of bounds")
          state := { state with moduleAst := { state.moduleAst with schemas } }
          i := i + 1
          continue

        if let some rest := stripPrefix? line "subtype " then
          let subtype ←
            match parseSubtypeDecl rest with
            | .ok v => pure v
            | .error msg => return (← failAt lineNo msg)
          let some schemas :=
            updateAt? state.moduleAst.schemas schemaIndex (fun s =>
              { s with subtypes := s.subtypes.push subtype })
            | return (← failAt lineNo "internal error: schema index out of bounds")
          state := { state with moduleAst := { state.moduleAst with schemas } }
          i := i + 1
          continue

        if startsWith line "relation " then
          let (combined, nextIndex) ←
            match collectBalancedParens lines i "relation" with
            | .ok v => pure v
            | .error msg => return (← failAt lineNo msg)
          let relation ←
            match parseRelationDecl combined with
            | .ok v => pure v
            | .error msg => return (← failAt lineNo msg)
          let some schemas :=
            updateAt? state.moduleAst.schemas schemaIndex (fun s =>
              { s with relations := s.relations.push relation })
            | return (← failAt lineNo "internal error: schema index out of bounds")
          state := { state with moduleAst := { state.moduleAst with schemas } }
          i := nextIndex
          continue

        if startsWith line "aspect " || startsWith line "function " then
          let generator ←
            match parseGeneratorDecl line with
            | .ok value => pure value
            | .error msg => return (← failAt lineNo msg)
          let some schemas :=
            updateAt? state.moduleAst.schemas schemaIndex (fun s =>
              { s with generators := s.generators.push generator })
            | return (← failAt lineNo "internal error: schema index out of bounds")
          state := { state with moduleAst := { state.moduleAst with schemas } }
          i := i + 1
          continue

        return (← failAt lineNo s!"unrecognized schema line: {line}")

    | .theory theoryIndex =>
        if let some rest := stripPrefix? line "constraint " then
          let restTrim := rest.trimAscii.toString
          if restTrim.endsWith ":" then
            let name := trimTrailingColon restTrim
            if name.isEmpty then
              return (← failAt lineNo "constraint name missing")
            let (bodyLines, nextIndex) := collectIndentedBlockLines lines (i + 1)
            let constraint : ConstraintV1 := .namedBlock name bodyLines
            let some theories :=
              updateAt? state.moduleAst.theories theoryIndex (fun t =>
                { t with constraints := t.constraints.push constraint })
              | return (← failAt lineNo "internal error: theory index out of bounds")
            state := { state with moduleAst := { state.moduleAst with theories } }
            i := nextIndex
            continue
          -- Support multi-line constraint “blocks” (e.g. `... where` followed by
          -- a few lines). We join the block and try to parse it as a known
          -- constraint; otherwise it remains `unknown` (visible to tooling).
          let (extra, nextIndex) := collectIndentedBlock lines (i + 1)
          let combined := if extra.isEmpty then restTrim else s!"{restTrim} {extra}".trimAscii.toString
          let constraint ←
            match parseConstraint combined with
            | .ok v => pure v
            | .error msg => return (← failAt lineNo msg)
          let some theories :=
            updateAt? state.moduleAst.theories theoryIndex (fun t =>
              { t with constraints := t.constraints.push constraint })
            | return (← failAt lineNo "internal error: theory index out of bounds")
          state := { state with moduleAst := { state.moduleAst with theories } }
          i := if extra.isEmpty then i + 1 else nextIndex
          continue

        if let some rest := stripPrefix? line "equation " then
          let equationName := trimTrailingColon rest
          if equationName.isEmpty then
            return (← failAt lineNo "equation name missing")
          let (equationText, nextIndex) := collectIndentedBlock lines (i + 1)
          let (lhs, rhs) ←
            match splitEquation equationText with
            | .ok v => pure v
            | .error msg => return (← failAt lineNo msg)
          let equation : EquationV1 := { name := equationName, lhs, rhs }
          let some theories :=
            updateAt? state.moduleAst.theories theoryIndex (fun t =>
              { t with equations := t.equations.push equation })
            | return (← failAt lineNo "internal error: theory index out of bounds")
          state := { state with moduleAst := { state.moduleAst with theories } }
          i := nextIndex
          continue

        if let some rest := stripPrefix? line "rewrite " then
          let ruleName := trimTrailingColon rest
          if ruleName.isEmpty then
            return (← failAt lineNo "rewrite rule name missing")
          let (blockLines, nextIndex) := collectIndentedBlockLines lines (i + 1)
          let rule ←
            match parseRewriteRuleBlock ruleName blockLines with
            | .ok v => pure v
            | .error msg => return (← failAt lineNo msg)
          let some theories :=
            updateAt? state.moduleAst.theories theoryIndex (fun t =>
              { t with rewriteRules := t.rewriteRules.push rule })
            | return (← failAt lineNo "internal error: theory index out of bounds")
          state := { state with moduleAst := { state.moduleAst with theories } }
          i := nextIndex
          continue

        return (← failAt lineNo s!"unrecognized theory line: {line}")

    | .instance instanceIndex =>
        match splitAssignment line with
        | some (lhs, rhs) =>
            let (setText, nextIndex) ←
              match collectBalancedBraces lines i rhs with
              | .ok v => pure v
              | .error msg => return (← failAt lineNo msg)
            let setLiteral ←
              match parseSetLiteral setText with
              | .ok v => pure v
              | .error msg => return (← failAt lineNo msg)
            let assignment : InstanceAssignmentV1 := { name := lhs, value := setLiteral }
            let some instances :=
              updateAt? state.moduleAst.instances instanceIndex (fun inst =>
                { inst with assignments := inst.assignments.push assignment })
              | return (← failAt lineNo "internal error: instance index out of bounds")
            state := { state with moduleAst := { state.moduleAst with instances } }
            i := nextIndex
            continue
        | none =>
            return (← failAt lineNo s!"unrecognized instance line: {line}")

  if state.moduleHeaderLine.isNone then
    return (← failAt 1 "canonical .axi input requires exactly one explicit `module <Name>` header")
  pure state.moduleAst

end Axiograph.Axi.SchemaV1
