import Std
import Axiograph.Certificate.Check
import Axiograph.Util.Fnv1a
import Axiograph.Axi.AxiV1

/-!
`axiograph_verify` is intended to be a **small trusted checker executable**.

Avoid importing the umbrella `Axiograph` module here: it pulls in proof-only
modules (HoTT semantics and soundness theorems) and can inflate the transitive
dependency set enough to make macOS linking fragile (very long link commands).

Instead, we import only the concrete parsing/checking modules needed at runtime.
-/

open Axiograph

structure AnchorContext where
  digestV1 : String
  /-- Parsed canonical `.axi` module used as verifier input context. -/
  module : Axiograph.Axi.AxiV1.AxiV1Module
  deriving Repr

def loadAxiV1Anchor (path : System.FilePath) : IO (Except String AnchorContext) := do
  let text ← IO.FS.readFile path
  let digest := Axiograph.Util.Fnv1a.digestTextV1 text
  match Axiograph.Axi.AxiV1.parseAxiV1 text with
  | .error err =>
      pure <| .error s!"axi parse error at line {err.line}: {err.message}"
  | .ok m =>
      pure <| .ok { digestV1 := digest, module := m }

def verifyCertificateJson
    (anchors : Std.HashMap String AnchorContext)
    (_path : System.FilePath)
    (json : Lean.Json) : Except String CertificateResult := do
  let env ← parseCertificateEnvelope json
  match env.anchor?, env.certificate with
  | none, cert =>
      verifyCertificate cert
  | some anchor, .reachabilityV3 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing canonical `.axi` module context for digest `{anchor.axiDigestV1}`"
      let index ← Query.buildAxiQueryIndexV3 ctx.module
      let res ← Query.verifyReachabilityProofV3Anchored index proof
      pure (.reachabilityV3 res)
  | some anchor, .axiWellTypedV1 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing canonical `.axi` module context for digest `{anchor.axiDigestV1}`"
      let res ← AxiWellTyped.verifyAxiWellTypedProofV1Anchored ctx.module proof
      pure (.axiWellTypedV1 res)
  | some anchor, .axiConstraintsOkV1 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing canonical `.axi` module context for digest `{anchor.axiDigestV1}`"
      let res ← AxiConstraintsOk.verifyAxiConstraintsOkProofV1Anchored ctx.module proof
      pure (.axiConstraintsOkV1 res)
  | some anchor, .queryResultV3 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing canonical `.axi` module context for digest `{anchor.axiDigestV1}`"
      let res ← Query.verifyQueryResultProofV3Anchored ctx.digestV1 ctx.module proof
      pure (.queryResultV3 res)
  | some anchor, .rewriteDerivationV3 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing canonical `.axi` module context for digest `{anchor.axiDigestV1}`"
      let res ← RewriteDerivation.verifyRewriteDerivationProofV3Anchored ctx.digestV1 ctx.module proof
      pure (.rewriteDerivationV3 res)
  | some anchor, cert =>
      -- For now we only have `.axi`-anchored checking for reachability.
      -- Still require that the referenced anchor digest was provided, so an
      -- anchor cannot silently become “unchecked metadata”.
      let _ ←
        match anchors.get? anchor.axiDigestV1 with
        | some _ => pure ()
        | none => throw s!"missing canonical `.axi` module context for digest `{anchor.axiDigestV1}`"
      verifyCertificate cert

def printResult (res : CertificateResult) : IO Unit := do
  match res with
  | .reachabilityV3 r =>
      let conf := Prob.toFloat r.confidence
      IO.println
        s!"ok: start={r.start} end={r.end_} len={r.pathLen} conf={conf} conf_fp={Prob.toNat r.confidence}"
  | .resolutionV2 r =>
      IO.println
        s!"ok: resolution={reprStr r.decision} first_fp={Prob.toNat r.firstConfidence} second_fp={Prob.toNat r.secondConfidence} threshold_fp={Prob.toNat r.threshold}"
  | .axiWellTypedV1 r =>
      IO.println s!"ok: axi_well_typed module={r.moduleName} schemas={r.schemaCount} instances={r.instanceCount}"
  | .axiConstraintsOkV1 r =>
      IO.println s!"ok: axi_constraints_ok module={r.moduleName} constraints={r.constraintCount} checks={r.checkCount}"
  | .queryResultV3 r =>
      IO.println s!"ok: query_result_v3 rows={r.rowCount} truncated={r.truncated}"
  | .normalizePathV2 r =>
      IO.println s!"ok: normalized path start={r.start} end={r.end_}"
  | .rewriteDerivationV2 r =>
      IO.println s!"ok: rewrite_derivation start={r.start} end={r.end_}"
  | .rewriteDerivationV3 r =>
      IO.println s!"ok: rewrite_derivation_v3 start={r.start} end={r.end_}"
  | .pathEquivV2 r =>
      IO.println s!"ok: path_equiv start={r.start} end={r.end_}"
  | .deltaFV1 r =>
      IO.println s!"ok: delta_f instance schema={r.pulledBack.schema} name={r.pulledBack.name}"

def main (args : List String) : IO UInt32 := do
  match args with
  | [] =>
      IO.eprintln "usage: axiograph_verify [module.axi ...] <certificate.json> [more.json ...]"
      pure 2
  | paths =>
      let mut anchors : Std.HashMap String AnchorContext := {}
      let mut certPaths : Array System.FilePath := #[]

      -- First pass: load canonical `.axi` modules and collect certificate paths.
      for pathStr in paths do
        let path : System.FilePath := pathStr
        if path.extension == some "axi" then
          match (← loadAxiV1Anchor path) with
          | .error err =>
              IO.eprintln s!"module load failed ({path}): {err}"
              return 1
          | .ok ctx =>
              anchors := anchors.insert ctx.digestV1 ctx
              IO.println s!"ok: loaded axi module digest={ctx.digestV1} file={path}"
        else
          certPaths := certPaths.push path

      -- Second pass: verify certificates (optionally anchored).
      let mut exitCode : UInt32 := 0
      for path in certPaths do
        let contents ← IO.FS.readFile path
        match Lean.Json.parse contents with
        | .error err =>
            IO.eprintln s!"JSON parse error ({path}): {err}"
            exitCode := 1
        | .ok json =>
            match verifyCertificateJson anchors path json with
            | .error err =>
                IO.eprintln s!"certificate verification failed ({path}): {err}"
                exitCode := 1
            | .ok res =>
                printResult res

      pure exitCode
