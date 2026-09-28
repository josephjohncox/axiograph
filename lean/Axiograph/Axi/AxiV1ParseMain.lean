import Axiograph.Axi.AxiV1

open Axiograph.Axi.AxiV1

private def differentialEnvelope
    (observedStage decision : String)
    (normalizedAst : Lean.Json)
    (diagnostic : Option String) : Lean.Json :=
  Lean.Json.mkObj
    [ ("schema", .str "axiograph.axi_v1_differential_envelope")
    , ("version", .num 1)
    , ("implementation", .str "lean")
    , ("requested_stage", .str "parse")
    , ("observed_stage", .str observedStage)
    , ("decision", .str decision)
    , ("normalized_ast", normalizedAst)
    , ("summary", .null)
    , ("rejection_class", .null)
    , ("diagnostic", diagnostic.map Lean.Json.str |>.getD .null)
    ]

private def run (path : String) (contractAst : Bool) : IO UInt32 := do
  let contents ← IO.FS.readFile path
  match parseAxiV1 contents with
  | .ok moduleAst =>
      if contractAst then
        IO.println (contractAstJsonV1 moduleAst).compress
      else
        IO.println
          s!"ok(axi_v1): module={moduleAst.moduleName} schemas={moduleAst.schemas.size} theories={moduleAst.theories.size} instances={moduleAst.instances.size}"
      pure 0
  | .error err =>
      IO.eprintln s!"parse error on line {err.line}: {err.message}"
      pure 1

private def runEnvelope (path : String) : IO UInt32 := do
  try
    let contents ← IO.FS.readFile path
    match parseAxiV1 contents with
    | .ok moduleAst =>
        IO.println (differentialEnvelope "parse" "accepted"
          (contractAstJsonV1 moduleAst) none).compress
        pure 0
    | .error err =>
        IO.println (differentialEnvelope "parse" "rejected" .null
          (some s!"parse error on line {err.line}: {err.message}")).compress
        pure 1
  catch error =>
    IO.println (differentialEnvelope "boundary" "rejected" .null
      (some s!"failed to read `{path}`: {error}")).compress
    pure 1

def main (args : List String) : IO UInt32 := do
  match args with
  | [path] => run path false
  | ["--contract-ast-v1", path] => run path true
  | ["--contract-envelope-v1", path] => runEnvelope path
  | _ =>
      IO.eprintln "usage: axiograph_parse_axi_v1 [--contract-ast-v1|--contract-envelope-v1] <file.axi>"
      pure 2
