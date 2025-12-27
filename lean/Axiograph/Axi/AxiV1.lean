import Axiograph.Axi.SchemaV1

/-!
# Unified `.axi` entrypoint: `axi_v1`

`axi_v1` is the **single canonical** `.axi` surface language entrypoint used by
the Rust runtime and the Lean checker.

For the Rust+Lean-only release we intentionally keep exactly one concrete
surface syntax:

- `axi_schema_v1` (schema/theory/instance) → `Axiograph.Axi.SchemaV1`

Historical note: the repo previously carried a separate `axi_learning_v1`
dialect. We removed that split to keep certificates, import/export, and parsing
parity centered on one AST and one grammar.
-/

namespace Axiograph.Axi.AxiV1

open Axiograph.Axi

abbrev AxiV1Module : Type := SchemaV1.SchemaV1Module
abbrev ParseError : Type := SchemaV1.ParseError

def parseAxiV1 (text : String) : Except ParseError AxiV1Module :=
  SchemaV1.parseSchemaV1 text

end Axiograph.Axi.AxiV1

