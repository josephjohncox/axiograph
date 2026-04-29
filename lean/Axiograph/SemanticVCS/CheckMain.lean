import Lean
import Axiograph.SemanticVCS.Json

/-!
Executable checker for Lean-readable semantic VCS theory payloads.

This is not the trusted certificate verifier yet.  It is a concrete
Rust-theory conformance checker: Rust emits a reduced merge/rebase plan shape,
and Lean checks the fail-closed materialization semantics against the finite
semantic-slice theory in `Axiograph.SemanticVCS`.
-/

open Lean

namespace Axiograph
namespace SemanticVCS

def checkSemanticVcsPayload (json : Lean.Json) : Except String String := do
  let version ← (← json.getObjVal? "version").getStr?
  match version with
  | "semantic_vcs_lean_merge_plan_v1" =>
      Json.checkMergePlanJson json
      pure "semantic_vcs_lean_merge_plan_v1"
  | "semantic_vcs_lean_rebase_plan_v1" =>
      Json.checkRebasePlanJson json
      pure "semantic_vcs_lean_rebase_plan_v1"
  | other =>
      throw s!"unsupported semantic VCS Lean payload version `{other}`"

def checkSemanticVcsPayloadFile (path : System.FilePath) : IO (Except String String) := do
  let text ← IO.FS.readFile path
  match Lean.Json.parse text with
  | .error err => pure (.error s!"JSON parse error: {err}")
  | .ok json => pure (checkSemanticVcsPayload json)

end SemanticVCS
end Axiograph

def main (args : List String) : IO UInt32 := do
  match args with
  | [] =>
      IO.eprintln "usage: semantic_vcs_check <merge-or-rebase-lean-json> [more.json ...]"
      pure 2
  | paths =>
      let mut exitCode : UInt32 := 0
      for pathStr in paths do
        let path : System.FilePath := pathStr
        match (← Axiograph.SemanticVCS.checkSemanticVcsPayloadFile path) with
        | .ok version =>
            IO.println s!"ok: {version} file={path}"
        | .error err =>
            IO.eprintln s!"semantic VCS theory check failed ({path}): {err}"
            exitCode := 1
      pure exitCode
