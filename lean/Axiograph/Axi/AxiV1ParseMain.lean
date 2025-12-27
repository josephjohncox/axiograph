import Axiograph.Axi.AxiV1

open Axiograph.Axi.AxiV1

def main (args : List String) : IO UInt32 := do
  match args with
  | [path] =>
      let contents ← IO.FS.readFile path
      match parseAxiV1 contents with
      | .ok moduleAst =>
          IO.println
            s!"ok(axi_v1): module={moduleAst.moduleName} schemas={moduleAst.schemas.size} theories={moduleAst.theories.size} instances={moduleAst.instances.size}"
          pure 0
      | .error err =>
          IO.eprintln s!"parse error on line {err.line}: {err.message}"
          pure 1
  | _ =>
      IO.eprintln "usage: axiograph_parse_axi_v1 <file.axi>"
      pure 2
