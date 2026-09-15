import Axiograph.Axi.SchemaV1
import Lean.Data.Json

/-!
# Unified `.axi` entrypoint: `axi_v1`

`axi_v1` is the **single canonical** `.axi` surface language entrypoint used by
the Rust runtime and the Lean checker.

The concrete schema/theory/instance parser currently lives in
`Axiograph.Axi.SchemaV1`, but that is an internal module boundary rather than a
second end-user dialect.

The intent is one canonical `.axi` authoring surface so certificates,
import/export, and parsing parity stay centered on one AST and one grammar.
-/

namespace Axiograph.Axi.AxiV1

open Axiograph.Axi

abbrev AxiV1Module : Type := SchemaV1.SchemaV1Module
abbrev ParseError : Type := SchemaV1.ParseError

def parseAxiV1 (text : String) : Except ParseError AxiV1Module :=
  SchemaV1.parseSchemaV1 text

/-!
`contractAstJsonV1` is a comparison view for parser conformance tests. It keeps
all source array order and constructor distinctions. It is not accepted source,
compiled IR, a revision anchor, or input to the checker. Lean constructs this
view from the AST produced by its own parser.
-/

private def jsonArray (values : Array α) (render : α → Lean.Json) : Lean.Json :=
  .arr (values.map render)

private def jsonNames (values : Array String) : Lean.Json :=
  jsonArray values Lean.Json.str

private def jsonOptional (value : Option α) (render : α → Lean.Json) : Lean.Json :=
  match value with
  | none => .null
  | some item => render item

private def roleKindJson : SchemaV1.RoleKindV1 → Lean.Json
  | .data => .str "data"
  | .context => .str "context"
  | .world => .str "world"
  | .temporal => .str "temporal"
  | .parameter => .str "parameter"
  | .evidence => .str "evidence"

private def refinementJson : SchemaV1.RefinementPredicateV1 → Lean.Json
  | .equals value => Lean.Json.mkObj [("kind", .str "equals"), ("value", .str value)]
  | .memberOf values => Lean.Json.mkObj [("kind", .str "member_of"), ("values", jsonNames values)]
  | .cardinality min max => Lean.Json.mkObj
      [("kind", .str "cardinality"), ("min", .num min.toNat), ("max", .num max.toNat)]
  | .key roles => Lean.Json.mkObj [("kind", .str "key"), ("roles", jsonNames roles)]
  | .enum values => Lean.Json.mkObj [("kind", .str "enum"), ("values", jsonNames values)]
  | .predicate name args => Lean.Json.mkObj
      [("kind", .str "predicate"), ("name", .str name), ("args", jsonNames args)]

private partial def typeExprJson : SchemaV1.TypeExprV1 → Lean.Json
  | .object name => Lean.Json.mkObj [("kind", .str "object"), ("name", .str name)]
  | .relationObject relation => Lean.Json.mkObj
      [("kind", .str "relation_object"), ("relation", .str relation)]
  | .indexed base overRoles => Lean.Json.mkObj
      [("kind", .str "indexed"), ("base", typeExprJson base),
       ("over_roles", jsonNames overRoles)]
  | .refined base predicates => Lean.Json.mkObj
      [("kind", .str "refined"), ("base", typeExprJson base),
       ("predicates", jsonArray predicates refinementJson)]

private def fieldJson (field : SchemaV1.FieldDeclV1) : Lean.Json :=
  Lean.Json.mkObj
    [("field", .str field.field), ("ty", typeExprJson field.ty),
     ("kind", roleKindJson field.kind)]

private def relationJson (relation : SchemaV1.RelationDeclV1) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str relation.name), ("fields", jsonArray relation.fields fieldJson)]

private def generatorKindJson : SchemaV1.GeneratorKindV1 → Lean.Json
  | .aspect => .str "aspect"
  | .function => .str "function"

private def generatorJson (generator : SchemaV1.GeneratorDeclV1) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str generator.name), ("source", .str generator.source),
     ("target", .str generator.target), ("kind", generatorKindJson generator.kind),
     ("reversible", .bool generator.reversible)]

private def subtypeJson (subtype : SchemaV1.SubtypeDeclV1) : Lean.Json :=
  Lean.Json.mkObj
    [("sub", .str subtype.sub), ("sup", .str subtype.sup),
     ("inclusion", jsonOptional subtype.inclusion Lean.Json.str)]

private def carrierJson (carrier : SchemaV1.CarrierFieldsV1) : Lean.Json :=
  Lean.Json.mkObj
    [("left_field", .str carrier.leftField), ("right_field", .str carrier.rightField)]

private def optionalField (name : String) (value : Option α)
    (render : α → Lean.Json) : List (String × Lean.Json) :=
  match value with
  | none => []
  | some item => [(name, render item)]

private def constraintJson : SchemaV1.ConstraintV1 → Lean.Json
  | .functional relation srcField dstField => Lean.Json.mkObj
      [("tag", .str "functional"), ("relation", .str relation),
       ("src_field", .str srcField), ("dst_field", .str dstField)]
  | .atMost relation srcField dstField max params => Lean.Json.mkObj <|
      [("tag", .str "at_most"), ("relation", .str relation),
       ("src_field", .str srcField), ("dst_field", .str dstField), ("max", .num max.toNat)] ++
      optionalField "params" params jsonNames
  | .typing relation rule => Lean.Json.mkObj
      [("tag", .str "typing"), ("relation", .str relation), ("rule", .str rule)]
  | .symmetricWhereIn relation field values carriers params => Lean.Json.mkObj <|
      [("tag", .str "symmetric_where_in"), ("relation", .str relation),
       ("field", .str field), ("values", jsonNames values)] ++
      optionalField "carriers" carriers carrierJson ++
      optionalField "params" params jsonNames
  | .symmetric relation carriers params => Lean.Json.mkObj <|
      [("tag", .str "symmetric"), ("relation", .str relation)] ++
      optionalField "carriers" carriers carrierJson ++
      optionalField "params" params jsonNames
  | .transitive relation carriers params => Lean.Json.mkObj <|
      [("tag", .str "transitive"), ("relation", .str relation)] ++
      optionalField "carriers" carriers carrierJson ++
      optionalField "params" params jsonNames
  | .key relation fields => Lean.Json.mkObj
      [("tag", .str "key"), ("relation", .str relation), ("fields", jsonNames fields)]
  | .namedBlock name body => Lean.Json.mkObj
      [("tag", .str "named_block"), ("name", .str name), ("body", jsonNames body)]
  | .unknown text => Lean.Json.mkObj [("tag", .str "unknown"), ("text", .str text)]

private def equationJson (equation : SchemaV1.EquationV1) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str equation.name), ("lhs", .str equation.lhs), ("rhs", .str equation.rhs)]

private def rewriteOrientationJson : SchemaV1.RewriteOrientationV1 → Lean.Json
  | .forward => .str "forward"
  | .backward => .str "backward"
  | .bidirectional => .str "bidirectional"

private def rewriteVarTypeJson : SchemaV1.RewriteVarTypeV1 → Lean.Json
  | .object ty => Lean.Json.mkObj [("tag", .str "object"), ("ty", .str ty)]
  | .path src dst => Lean.Json.mkObj
      [("tag", .str "path"), ("from", .str src), ("to", .str dst)]

private def rewriteVarJson (rewriteVar : SchemaV1.RewriteVarDeclV1) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str rewriteVar.name), ("ty", rewriteVarTypeJson rewriteVar.ty)]

private partial def pathExprJson : SchemaV1.PathExprV3 → Lean.Json
  | .var name => Lean.Json.mkObj [("type", .str "var"), ("name", .str name)]
  | .reflexive entity => Lean.Json.mkObj
      [("type", .str "reflexive"), ("entity", .str entity)]
  | .step src rel dst => Lean.Json.mkObj
      [("type", .str "step"), ("from", .str src), ("rel", .str rel), ("to", .str dst)]
  | .trans left right => Lean.Json.mkObj
      [("type", .str "trans"), ("left", pathExprJson left), ("right", pathExprJson right)]
  | .inv path => Lean.Json.mkObj [("type", .str "inv"), ("path", pathExprJson path)]

private def rewriteJson (rule : SchemaV1.RewriteRuleV1) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str rule.name), ("orientation", rewriteOrientationJson rule.orientation),
     ("vars", jsonArray rule.vars rewriteVarJson), ("lhs", pathExprJson rule.lhs),
     ("rhs", pathExprJson rule.rhs)]

private def theoryJson (theory : SchemaV1.SchemaV1Theory) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str theory.name), ("schema", .str theory.schema),
     ("constraints", jsonArray theory.constraints constraintJson),
     ("equations", jsonArray theory.equations equationJson),
     ("rewrite_rules", jsonArray theory.rewriteRules rewriteJson)]

private def setItemJson : SchemaV1.SetItemV1 → Lean.Json
  | .ident name => Lean.Json.mkObj [("tag", .str "ident"), ("name", .str name)]
  | .tuple label fields =>
      let fieldJson := jsonArray fields fun (field, value) => .arr #[.str field, .str value]
      Lean.Json.mkObj <| [("tag", .str "tuple")] ++
        optionalField "label" label Lean.Json.str ++ [("fields", fieldJson)]

private def assignmentJson (assignment : SchemaV1.InstanceAssignmentV1) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str assignment.name),
     ("value", Lean.Json.mkObj [("items", jsonArray assignment.value.items setItemJson)])]

private def instanceJson (moduleInstance : SchemaV1.SchemaV1Instance) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str moduleInstance.name), ("schema", .str moduleInstance.schema),
     ("assignments", jsonArray moduleInstance.assignments assignmentJson)]

private def schemaJson (schema : SchemaV1.SchemaV1Schema) : Lean.Json :=
  Lean.Json.mkObj
    [("name", .str schema.name), ("objects", jsonNames schema.objects),
     ("subtypes", jsonArray schema.subtypes subtypeJson),
     ("relations", jsonArray schema.relations relationJson),
     ("generators", jsonArray schema.generators generatorJson)]

/-- Deterministic, non-authoritative AST comparison view. -/
def contractAstJsonV1 (moduleAst : AxiV1Module) : Lean.Json :=
  Lean.Json.mkObj
    [("module_name", .str moduleAst.moduleName), ("imports", jsonNames moduleAst.imports),
     ("schemas", jsonArray moduleAst.schemas schemaJson),
     ("theories", jsonArray moduleAst.theories theoryJson),
     ("instances", jsonArray moduleAst.instances instanceJson)]

end Axiograph.Axi.AxiV1
