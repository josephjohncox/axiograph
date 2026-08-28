import Axiograph.Axi.AxiV1
import Axiograph.Theory.Finite

open Axiograph.Axi.AxiV1
open Axiograph.Theory.Finite

/-- Standalone exact-source formation check used by the Rust/Lean category
conformance corpus. It checks the same finite presentation that
`category_kernel_v3` reconstructs inside `VerifyMain`; it does not certify
instances or claims outside that presentation. -/
def main (args : List String) : IO UInt32 := do
  match args with
  | [path, schemaName] =>
      let contents ← IO.FS.readFile path
      let module ←
        match parseAxiV1 contents with
        | .ok module => pure module
        | .error err =>
            IO.eprintln s!"parse error at line {err.line}: {err.message}"
            return 1
      let schemas := module.schemas.filter (fun schema => schema.name == schemaName)
      if schemas.size != 1 then
        IO.eprintln s!"module must contain exactly one schema `{schemaName}`"
        return 1
      let some schema := schemas[0]?
        | IO.eprintln s!"internal schema selection failure for `{schemaName}`"
          return 1
      let presentation ←
        match compileAxiSchemaPresentation module schema with
        | .ok presentation => pure presentation
        | .error residuals =>
            IO.eprintln s!"category formation failed: {repr residuals}"
            return 1
      let manifest ←
        match categoryKernelPresentationV3 presentation with
        | .ok manifest => pure manifest
        | .error residuals =>
            IO.eprintln s!"category manifest failed: {repr residuals}"
            return 1
      IO.println s!"ok: category_kernel_formation schema={schemaName} objects={manifest.objectNames.size} arrows={manifest.arrows.size} equations={manifest.equations.size}"
      pure 0
  | _ =>
      IO.eprintln "usage: axiograph_category_kernel_formation <module.axi> <schema>"
      pure 2
