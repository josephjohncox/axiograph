import Axiograph.Axi.SchemaV1

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

end Axiograph.Axi.AxiV1
