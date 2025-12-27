import Axiograph.Axi.SchemaV1

open Axiograph.Axi.SchemaV1

def main (args : List String) : IO UInt32 := do
  match args with
  | [path] =>
      let contents ← IO.FS.readFile path
      match parseSchemaV1 contents with
      | .ok moduleAst =>
          IO.println
            s!"ok: module={moduleAst.moduleName} schemas={moduleAst.schemas.size} theories={moduleAst.theories.size} instances={moduleAst.instances.size}"
          pure 0
      | .error err =>
          IO.eprintln s!"parse error on line {err.line}: {err.message}"
          pure 1
  | _ =>
      IO.eprintln "usage: axiograph_parse_schema_v1 <file.axi>"
      pure 2

