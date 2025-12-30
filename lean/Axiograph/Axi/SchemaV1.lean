import Std
import Std.Internal.Parsec

/-!
# `.axi` dialect: `axi_schema_v1`

This module defines the **schema-oriented** `.axi` surface syntax used by the
canonical corpus:

- `examples/economics/EconomicFlows.axi`
- `examples/ontology/SchemaEvolution.axi`

During the migration we keep dialects explicit and versioned so we can support
multiple syntaxes without a flag day.

## Design goals

1. **Readable and auditable**: the trusted checker should be easy to review.
2. **Stable**: parse the canonical corpus in a deterministic way.
3. **Lockstep with Rust**: the Rust parser lives at
   `rust/crates/axiograph-dsl/src/schema_v1.rs` and should stay structurally
   aligned with this Lean module.

This parser is intentionally conservative: it fails fast on unrecognized lines.
-/

namespace Axiograph.Axi.SchemaV1

abbrev Name : Type := String

-- =============================================================================
-- AST
-- =============================================================================

structure FieldDeclV1 where
  field : Name
  ty : Name
deriving Repr, DecidableEq

structure RelationDeclV1 where
  name : Name
  fields : Array FieldDeclV1
deriving Repr, DecidableEq

structure SubtypeDeclV1 where
  sub : Name
  sup : Name
  /-- Optional explicit inclusion morphism name (legacy syntax). -/
  inclusion : Option Name
deriving Repr, DecidableEq

structure SchemaV1Schema where
  name : Name
  objects : Array Name
  subtypes : Array SubtypeDeclV1
  relations : Array RelationDeclV1
deriving Repr, DecidableEq

inductive ConstraintV1 where
  | functional (relation srcField dstField : Name)
  | symmetric (relation : Name)
  | transitive (relation : Name)
  | key (relation : Name) (fields : Array Name)
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
  | tuple (fields : Array (Name × Name))
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
deriving Repr

def emptyModule : SchemaV1Module :=
  { moduleName := "Unnamed", schemas := #[], theories := #[], instances := #[] }

def failAt {α : Type} (line : Nat) (message : String) : Except ParseError α :=
  throw { line, message }

def trimTrailingColon (s : String) : String :=
  let trimmed := s.trim
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

def parseRelationDecl (line : String) : Except String RelationDeclV1 := do
  let comma : LineParser Unit := do
    ws
    skipChar ','
    ws

  let fieldDecl : LineParser FieldDeclV1 := do
    ws
    let field ← identifier
    ws
    skipChar ':'
    ws
    let ty ← identifier
    pure { field, ty }

  /-
  Optional relation annotations.

  The canonical Rust parser supports legacy-ish surface forms like:

  ```
  relation Parent(child: Person, parent: Person) @context Context @temporal Time
  ```

  For now, we preserve this behavior by **expanding** a small set of
  annotations into explicit fields:

  - `@context Context` ⇒ adds a `ctx : Context` field (unless already present)
  - `@temporal Time`   ⇒ adds a `time : Time` field (unless already present)

  This keeps Rust/Lean parsing in lockstep while we continue to evolve the
  formal semantics (Lean) for contexts/worlds and time.
  -/
  let annotation : LineParser (Name × Name) := do
    ws1
    skipChar '@'
    let ann ← identifier
    ws1
    let ty ← identifier
    pure (ann, ty)

  let p : LineParser RelationDeclV1 := do
    ws
    skipString "relation"
    ws1
    let name ← identifier
    ws
    skipChar '('
    let fields ← sepBy1 fieldDecl comma
    ws
    skipChar ')'
    let annotations ← many annotation

    let expandedFields :=
      annotations.foldl (init := fields) (fun acc (ann, ty) =>
        match ann with
        | "context" =>
            if acc.any (fun f => f.field == "ctx") then
              acc
            else
              acc.push { field := "ctx", ty := ty }
        | "temporal" =>
            if acc.any (fun f => f.field == "time") then
              acc
            else
              acc.push { field := "time", ty := ty }
        | _ => acc)

    pure { name, fields := expandedFields }

  match runLineParser p line with
  | .ok v => pure v
  | .error msg => throw msg

-- =============================================================================
-- Theory section parsers
-- =============================================================================

def parseConstraint (rest : String) : Except String ConstraintV1 := do
  let trimmed := rest.trim

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
        -- Some examples use a more declarative form like:
        --
        --   `constraint functional Rel(field0, field1, ...)`
        --   `constraint functional Rel(field0, ...) -> Rel.someOutput`
        --
        -- The initial certified subset only understands
        -- `functional Rel.field -> Rel.field`. For now, keep these constraints
        -- parseable (and visible) without making them part of the trusted core.
        return (.unknown trimmed)
  else if startsWith trimmed "symmetric " then
    let p : LineParser ConstraintV1 := do
      skipString "symmetric"
      ws1
      let relation ← identifier
      pure (.symmetric relation)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error msg => throw msg
  else if startsWith trimmed "transitive " then
    let p : LineParser ConstraintV1 := do
      skipString "transitive"
      ws1
      let relation ← identifier
      pure (.transitive relation)
    match runLineParser p trimmed with
    | .ok v => return v
    | .error msg => throw msg
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
    || startsWith trimmed "constraint "
    || startsWith trimmed "equation "
    || startsWith trimmed "rewrite "

def collectIndentedBlock (lines : Array String) (startIndex : Nat) : (String × Nat) :=
  Id.run do
    let mut out : Array String := #[]
    let mut i := startIndex
    while _h : i < lines.size do
      let trimmed := stripComment (lines[i]!) |>.trim
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
      let trimmed := stripComment (lines[i]!) |>.trim
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
  match s.trim with
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
    let line := raw.trim
    if line.isEmpty then
      continue

    if let some rest := stripPrefix? line "vars:" then
      current := .vars
      let rest := rest.trim
      if !rest.isEmpty then
        varsLines := varsLines.push rest
      continue

    if let some rest := stripPrefix? line "lhs:" then
      current := .lhs
      let rest := rest.trim
      if !rest.isEmpty then
        lhsLines := lhsLines.push rest
      continue

    if let some rest := stripPrefix? line "rhs:" then
      current := .rhs
      let rest := rest.trim
      if !rest.isEmpty then
        rhsLines := rhsLines.push rest
      continue

    if let some rest := stripPrefix? line "orientation:" then
      current := .orientation
      let rest := rest.trim
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
  if lhsText.trim.isEmpty then
    throw s!"rewrite `{ruleName}`: missing `lhs:`"
  if rhsText.trim.isEmpty then
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
      let lhs := lhs.trim
      let rhs := rhs.trim
      if lhs.isEmpty || rhs.isEmpty then
        throw "equation must have non-empty lhs and rhs"
      pure (lhs, rhs)
  | none =>
      throw "equation body must contain `=`"

def splitAssignment (line : String) : Option (String × String) :=
  match splitOnceChar line '=' with
  | some (lhs, rhs) =>
      let lhs := lhs.trim
      let rhs := rhs.trim
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
    let line := stripComment (lines[i]!) |>.trim
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

  let rhs := stripComment firstRhs |>.trim
  combined := combined.push rhs
  depth ← adjustBraceDepth depth rhs

  let mut i := startIndex + 1
  while _h : i < lines.size do
    if depth <= 0 then
      break
    let line := stripComment (lines[i]!) |>.trim
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
    let value ← identifier
    pure (key, value)

  let tupleItem : LineParser SetItemV1 := do
    skipChar '('
    let fields ← sepBy1 tupleField comma
    ((attempt comma) <|> pure ())
    ws
    skipChar ')'
    pure (.tuple fields)

  let setItem : LineParser SetItemV1 :=
    (attempt tupleItem) <|> do
      let name ← identifier
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
  let mut state : ParseState := { moduleAst := emptyModule, currentSection := .none }
  let mut i : Nat := 0

  while _h : i < lines.size do
    let lineNo := i + 1
    let line := stripComment (lines[i]!) |>.trim

    if line.isEmpty then
      i := i + 1
      continue

    -- ----------------------------------------------------------------------
    -- Section headers
    -- ----------------------------------------------------------------------
    if let some name := stripPrefix? line "module " then
      let moduleName := name.trim
      let moduleName := if moduleName.isEmpty then "Unnamed" else moduleName
      state := { state with moduleAst := { state.moduleAst with moduleName } }
      i := i + 1
      continue

    if let some rest := stripPrefix? line "schema " then
      let schemaName := trimTrailingColon rest
      if schemaName.isEmpty then
        return (← failAt lineNo "schema name missing")
      let schema : SchemaV1Schema := { name := schemaName, objects := #[], subtypes := #[], relations := #[] }
      let newIndex := state.moduleAst.schemas.size
      state :=
        { moduleAst := { state.moduleAst with schemas := state.moduleAst.schemas.push schema }
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
        { moduleAst := { state.moduleAst with theories := state.moduleAst.theories.push theory }
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
        { moduleAst := { state.moduleAst with instances := state.moduleAst.instances.push instanceAst }
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
          let objectName := name.trim
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

        return (← failAt lineNo s!"unrecognized schema line: {line}")

    | .theory theoryIndex =>
        if let some rest := stripPrefix? line "constraint " then
          -- Support multi-line constraint “blocks” (e.g. richer guardrails with
          -- `message:` / `severity:` lines) by preserving them as `unknown`.
          -- This mirrors the Rust parser behavior in
          -- `rust/crates/axiograph-dsl/src/schema_v1.rs`.
          let (extra, nextIndex) := collectIndentedBlock lines (i + 1)
          let constraint ←
            if extra.isEmpty then
              match parseConstraint rest with
              | .ok v => pure v
              | .error msg => return (← failAt lineNo msg)
            else
              pure (.unknown s!"{rest.trim} {extra}".trim)
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

  pure state.moduleAst

end Axiograph.Axi.SchemaV1
