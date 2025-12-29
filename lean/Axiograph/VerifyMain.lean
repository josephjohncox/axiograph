import Std
import Axiograph.Certificate.Check
import Axiograph.Util.Fnv1a
import Axiograph.Axi.AxiV1
import Axiograph.Axi.PathDBExportV1

/-!
`axiograph_verify` is intended to be a **small trusted checker executable**.

Avoid importing the umbrella `Axiograph` module here: it pulls in proof-only
modules (HoTT semantics and soundness theorems) and can inflate the transitive
dependency set enough to make macOS linking fragile (very long link commands).

Instead, we import only the concrete parsing/checking modules needed at runtime.
-/

open Axiograph

structure SnapshotContext where
  /-- Edge facts extracted from a `PathDBExportV1` snapshot (`relation_info`). -/
  relationInfo : Std.HashMap Nat Axiograph.Axi.PathDBExportV1.RelationInfoRow
  /-- Type facts extracted from `entity_type` (entity id → type id). -/
  entityType : Std.HashMap Nat Nat
  /-- Attribute facts extracted from `entity_attribute` ((entity id, key id) → value id). -/
  entityAttribute : Std.HashMap (Nat × Nat) Nat
  /-- Interned strings extracted from `interned_string` (string id → decoded UTF-8). -/
  internedString : Std.HashMap Nat String
  deriving Repr

structure AnchorContext where
  digestV1 : String
  /-- Parsed `.axi` module (always available for anchors). -/
  module : Axiograph.Axi.AxiV1.AxiV1Module
  /-- Optional snapshot tables when the anchor is a `PathDBExportV1` export. -/
  snapshot? : Option SnapshotContext
  deriving Repr

def loadAxiV1Anchor (path : System.FilePath) : IO (Except String AnchorContext) := do
  let text ← IO.FS.readFile path
  let digest := Axiograph.Util.Fnv1a.digestTextV1 text
  match Axiograph.Axi.AxiV1.parseAxiV1 text with
  | .error err =>
      pure <| .error s!"axi parse error at line {err.line}: {err.message}"
  | .ok m => do
      let snapshot? : Option SnapshotContext :=
        match Axiograph.Axi.PathDBExportV1.extractRelationInfo m with
        | .error _ => none
        | .ok relInfo =>
            match Axiograph.Axi.PathDBExportV1.extractEntityType m with
            | .error _ => none
            | .ok entityType =>
                match Axiograph.Axi.PathDBExportV1.extractEntityAttribute m with
                | .error _ => none
                | .ok entityAttribute =>
                    match Axiograph.Axi.PathDBExportV1.extractInternedString m with
                    | .error _ => none
                    | .ok internedString =>
                        some { relationInfo := relInfo, entityType, entityAttribute, internedString }
      pure <| .ok { digestV1 := digest, module := m, snapshot? := snapshot? }

def verifyCertificateJson
    (anchors : Std.HashMap String AnchorContext)
    (_path : System.FilePath)
    (json : Lean.Json) : Except String CertificateResult := do
  let env ← parseCertificateEnvelope json
  match env.anchor?, env.certificate with
  | none, cert =>
      verifyCertificate cert
  | some anchor, .reachabilityV2 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let some snap := ctx.snapshot?
        | throw "anchored reachability requires a `PathDBExportV1` anchor (missing snapshot tables)"
      let res ← Reachability.verifyReachabilityProofV2Anchored snap.relationInfo proof
      pure (.reachabilityV2 res)
  | some anchor, .axiWellTypedV1 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let res ← AxiWellTyped.verifyAxiWellTypedProofV1Anchored ctx.module proof
      pure (.axiWellTypedV1 res)
  | some anchor, .axiConstraintsOkV1 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let res ← AxiConstraintsOk.verifyAxiConstraintsOkProofV1Anchored ctx.module proof
      pure (.axiConstraintsOkV1 res)
  | some anchor, .queryResultV1 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let some snap := ctx.snapshot?
        | throw "query_result_v1 requires a `PathDBExportV1` anchor (missing snapshot tables)"
      let res ← Query.verifyQueryResultProofV1Anchored snap.relationInfo snap.entityType snap.entityAttribute snap.internedString proof
      pure (.queryResultV1 res)
  | some anchor, .queryResultV2 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let some snap := ctx.snapshot?
        | throw "query_result_v2 requires a `PathDBExportV1` anchor (missing snapshot tables)"
      let res ← Query.verifyQueryResultProofV2Anchored snap.relationInfo snap.entityType snap.entityAttribute snap.internedString proof
      pure (.queryResultV2 res)
  | some anchor, .queryResultV3 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let res ← Query.verifyQueryResultProofV3Anchored ctx.digestV1 ctx.module proof
      pure (.queryResultV3 res)
  | some anchor, .rewriteDerivationV3 proof =>
      let some ctx := anchors.get? anchor.axiDigestV1
        | throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      let res ← RewriteDerivation.verifyRewriteDerivationProofV3Anchored ctx.digestV1 ctx.module proof
      pure (.rewriteDerivationV3 res)
  | some anchor, cert =>
      -- For now we only have `.axi`-anchored checking for reachability.
      -- Still require that the referenced anchor digest was provided, so an
      -- anchor cannot silently become “unchecked metadata”.
      let _ ←
        match anchors.get? anchor.axiDigestV1 with
        | some _ => pure ()
        | none => throw s!"missing `.axi` anchor context for digest `{anchor.axiDigestV1}`"
      verifyCertificate cert

def printResult (res : CertificateResult) : IO Unit := do
  match res with
  | .reachabilityV1 r =>
      IO.println s!"ok: start={r.start} end={r.end_} len={r.pathLen} conf={r.confidence}"
  | .reachabilityV2 r =>
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
  | .queryResultV1 r =>
      IO.println s!"ok: query_result rows={r.rowCount} truncated={r.truncated}"
  | .queryResultV2 r =>
      IO.println s!"ok: query_result_v2 rows={r.rowCount} truncated={r.truncated}"
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
      IO.eprintln "usage: axiograph_verify [anchor.axi ...] <certificate.json> [more.json ...]"
      pure 2
  | paths =>
      let mut anchors : Std.HashMap String AnchorContext := {}
      let mut certPaths : Array System.FilePath := #[]

      -- First pass: load `.axi` anchors and collect certificate paths.
      for pathStr in paths do
        let path : System.FilePath := pathStr
        if path.extension == some "axi" then
          match (← loadAxiV1Anchor path) with
          | .error err =>
              IO.eprintln s!"anchor load failed ({path}): {err}"
              return 1
          | .ok ctx =>
              anchors := anchors.insert ctx.digestV1 ctx
              let isSnapshot := ctx.snapshot?.isSome
              IO.println s!"ok: loaded axi anchor digest={ctx.digestV1} snapshot={isSnapshot} file={path}"
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
