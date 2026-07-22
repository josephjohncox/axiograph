import Axiograph.Axi.AxiV1
import Axiograph.Axi.TypeCheck

open Axiograph.Axi.AxiV1
open Axiograph.Axi.TypeCheck


def main (args : List String) : IO UInt32 := do
  match args with
  | [path] =>
      let contents ← IO.FS.readFile path
      match parseAxiV1 contents with
      | .error err =>
          IO.eprintln s!"parse error: parse error on line {err.line}: {err.message}"
          pure 1
      | .ok moduleAst =>
          match typecheckModule moduleAst with
          | .error error =>
              IO.eprintln s!"type error: {error}"
              pure 1
          | .ok summary =>
              IO.println s!"ok(typecheck): module={summary.moduleName} schemas={summary.schemaCount} theories={summary.theoryCount} instances={summary.instanceCount} assignments={summary.assignmentCount} tuples={summary.tupleCount}"
              pure 0
  | _ =>
      IO.eprintln "usage: axiograph_typecheck_axi <file.axi>"
      pure 2
