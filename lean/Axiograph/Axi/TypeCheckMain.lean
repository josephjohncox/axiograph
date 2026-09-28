import Axiograph.Axi.AxiV1
import Axiograph.Axi.TypeCheck

open Axiograph.Axi.AxiV1
open Axiograph.Axi.TypeCheck

private def differentialEnvelope
    (observedStage decision : String)
    (normalizedAst summary : Lean.Json)
    (diagnostic : Option String) : Lean.Json :=
  Lean.Json.mkObj
    [ ("schema", .str "axiograph.axi_v1_differential_envelope")
    , ("version", .num 1)
    , ("implementation", .str "lean")
    , ("requested_stage", .str "typecheck")
    , ("observed_stage", .str observedStage)
    , ("decision", .str decision)
    , ("normalized_ast", normalizedAst)
    , ("summary", summary)
    , ("rejection_class", .null)
    , ("diagnostic", diagnostic.map Lean.Json.str |>.getD .null)
    ]

private def typecheckSummaryJson (summary : TypeCheckSummaryV1) : Lean.Json :=
  Lean.Json.mkObj
    [ ("module", .str summary.moduleName)
    , ("schemas", .num summary.schemaCount)
    , ("theories", .num summary.theoryCount)
    , ("instances", .num summary.instanceCount)
    , ("assignments", .num summary.assignmentCount)
    , ("tuples", .num summary.tupleCount)
    ]

private def run (path : String) (envelope : Bool) : IO UInt32 := do
  try
    let contents ← IO.FS.readFile path
    match parseAxiV1 contents with
    | .error err =>
        let diagnostic := s!"parse error: parse error on line {err.line}: {err.message}"
        if envelope then
          IO.println (differentialEnvelope "parse" "rejected" .null .null
            (some diagnostic)).compress
        else
          IO.eprintln diagnostic
        pure 1
    | .ok moduleAst =>
        match typecheckModule moduleAst with
        | .error error =>
            let diagnostic := s!"type error: {error}"
            if envelope then
              IO.println (differentialEnvelope "typecheck" "rejected"
                (contractAstJsonV1 moduleAst) .null (some diagnostic)).compress
            else
              IO.eprintln diagnostic
            pure 1
        | .ok summary =>
            if envelope then
              IO.println (differentialEnvelope "typecheck" "accepted"
                (contractAstJsonV1 moduleAst) (typecheckSummaryJson summary) none).compress
            else
              IO.println s!"ok(typecheck): module={summary.moduleName} schemas={summary.schemaCount} theories={summary.theoryCount} instances={summary.instanceCount} assignments={summary.assignmentCount} tuples={summary.tupleCount}"
            pure 0
  catch error =>
    if envelope then
      IO.println (differentialEnvelope "boundary" "rejected" .null .null
        (some s!"failed to read `{path}`: {error}")).compress
      pure 1
    else
      throw error

def main (args : List String) : IO UInt32 := do
  match args with
  | [path] => run path false
  | ["--contract-envelope-v1", path] => run path true
  | _ =>
      IO.eprintln "usage: axiograph_typecheck_axi [--contract-envelope-v1] <file.axi>"
      pure 2
