import Std
import Axiograph.Certificate.Check
import Axiograph.Certificate.Invariants
import Axiograph.Identity
import Axiograph.Util.Sha256
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
  /-- Cryptographic revision identity independently recomputed from exact accepted bytes. -/
  revisionDigest : String
  /-- Parsed canonical `.axi` module used as verifier input context. -/
  module : Axiograph.Axi.AxiV1.AxiV1Module
  deriving Repr

def maxAxiModuleBytes : Nat := 4 * 1024 * 1024
def maxCertificateBytes : Nat := 8 * 1024 * 1024
def maxStdioRequestBytes : Nat := 16 * 1024 * 1024
def maxFileInputs : Nat := 32
def maxAnchorModules : Nat := 16
def maxJsonNesting : Nat := 128

partial def readUtf8Checked
    (readChunk : IO ByteArray) (limit : Nat) (label : String) : IO (Except String String) := do
  let rec loop (bytes : ByteArray) : IO (Except String ByteArray) := do
    let chunk ← readChunk
    if chunk.isEmpty then
      pure (.ok bytes)
    else
      let next := bytes ++ chunk
      if next.size > limit then
        pure (.error s!"{label} exceeds {limit} bytes")
      else
        loop next
  match (← loop ByteArray.empty) with
  | .error err => pure (.error err)
  | .ok bytes =>
      match String.fromUTF8? bytes with
      | some text => pure (.ok text)
      | none => pure (.error s!"{label} is not UTF-8")

def readHandleChecked
    (handle : IO.FS.Handle) (limit : Nat) (label : String) : IO (Except String String) :=
  readUtf8Checked (handle.read 65536) limit label

def readStreamChecked
    (stream : IO.FS.Stream) (limit : Nat) (label : String) : IO (Except String String) :=
  readUtf8Checked (stream.read 65536) limit label

def readFileChecked
    (path : System.FilePath) (limit : Nat) : IO (Except String String) := do
  try
    IO.FS.withFile path .read fun handle =>
      readHandleChecked handle limit s!"input `{path}`"
  catch err =>
    pure (.error s!"failed to read `{path}`: {err} ")

structure JsonDepthState where
  depth : Nat := 0
  inString : Bool := false
  escaped : Bool := false
  valid : Bool := true

def jsonNestingWithin (input : String) (limit : Nat) : Bool :=
  let state : JsonDepthState := input.toList.foldl (fun (state : JsonDepthState) char =>
    if !state.valid then state
    else if state.inString then
      if state.escaped then { state with escaped := false }
      else if char == '\\' then { state with escaped := true }
      else if char == '"' then { state with inString := false }
      else state
    else if char == '"' then { state with inString := true }
    else if char == '{' || char == '[' then
      let depth := state.depth + 1
      { state with depth, valid := depth <= limit }
    else if char == '}' || char == ']' then
      if state.depth == 0 then { state with valid := false }
      else { state with depth := state.depth - 1 }
    else state) {}
  state.valid && !state.inString && state.depth == 0

def loadAxiV1Anchor (path : System.FilePath) : IO (Except String AnchorContext) := do
  match (← readFileChecked path maxAxiModuleBytes) with
  | .error err => pure (.error err)
  | .ok text =>
      let revisionDigest := Axiograph.Identity.revisionDigestText text
      match Axiograph.Axi.AxiV1.parseAxiV1 text with
      | .error err =>
          pure <| .error s!"axi parse error at line {err.line}: {err.message}"
      | .ok m =>
          pure <| .ok { revisionDigest, module := m }

def verifyCertificateJson
    (anchors : Std.HashMap String AnchorContext)
    (_path : System.FilePath)
    (json : Lean.Json) : Except String CertificateResult := do
  let env ← parseCertificateEnvelope json
  match env.anchor?, env.certificate with
  | none, cert =>
      verifyCertificate cert
  | some (.v1 anchor), .reachabilityV3 proof =>
      let some ctx := anchors.get? anchor.revisionDigestV2
        | throw s!"missing canonical `.axi` module context for revision `{anchor.revisionDigestV2}`"
      let index ← Query.buildAxiFiniteQueryIndexV4 ctx.module
      let res ← Query.verifyReachabilityProofV3Anchored index proof
      pure (.reachabilityV3 res)
  | some (.v1 anchor), .axiWellTypedV1 proof =>
      let some ctx := anchors.get? anchor.revisionDigestV2
        | throw s!"missing canonical `.axi` module context for revision `{anchor.revisionDigestV2}`"
      let res ← AxiWellTyped.verifyAxiWellTypedProofV1Anchored ctx.module proof
      pure (.axiWellTypedV1 res)
  | some (.v1 anchor), .axiConstraintsOkV1 proof =>
      let some ctx := anchors.get? anchor.revisionDigestV2
        | throw s!"missing canonical `.axi` module context for revision `{anchor.revisionDigestV2}`"
      let res ← AxiConstraintsOk.verifyAxiConstraintsOkProofV1Anchored ctx.module proof
      pure (.axiConstraintsOkV1 res)
  | some (.v1 anchor), .rewriteDerivationV3 proof =>
      let some ctx := anchors.get? anchor.revisionDigestV2
        | throw s!"missing canonical `.axi` module context for revision `{anchor.revisionDigestV2}`"
      let res ← RewriteDerivation.verifyRewriteDerivationProofV3Anchored ctx.revisionDigest ctx.module proof
      pure (.rewriteDerivationV3 res)
  | some (.v2 _), .queryResultV4 _ =>
      throw "query_result_v4 requires axiograph-verifier-stdio-v2 caller expectations"
  | some (.v2 anchor), .categoryKernelV3 proof =>
      let some ctx := anchors.get? anchor.revisionDigestV2
        | throw s!"missing canonical `.axi` module context for revision `{anchor.revisionDigestV2}`"
      let res ← CategoryKernelCertificate.verifyV3 ctx.module proof
      pure (.categoryKernelV3 res)
  | some (.v2 _), _ =>
      throw "certificate envelope V3 accepts only query_result_v4 or category_kernel_v3"
  | some (.v1 anchor), cert =>
      let _ ←
        match anchors.get? anchor.revisionDigestV2 with
        | some _ => pure ()
        | none => throw s!"missing canonical `.axi` module context for revision `{anchor.revisionDigestV2}`"
      verifyCertificate cert

def printResult (res : CertificateResult) : IO Unit := do
  match res with
  | .reachabilityV3 r =>
      let conf := Prob.toFloat r.confidence
      IO.println
        s!"ok: start={r.start} end={r.end_} len={r.pathLen} conf={conf} conf_fp={Prob.toNat r.confidence}"
  | .categoryKernelV3 r =>
      IO.println s!"ok: category_kernel_v3 schema={r.schemaName} objects={r.objectCount} arrows={r.arrowCount} equations={r.equationCount} congruence={r.congruenceCertificateCount} groupoid_normalizations={r.groupoidNormalizationCount} reachability={r.reachabilityEntryCount} lifecycle={reprStr r.lifecycle}"
  | .resolutionV2 r =>
      IO.println
        s!"ok: resolution={reprStr r.decision} first_fp={Prob.toNat r.firstConfidence} second_fp={Prob.toNat r.secondConfidence} threshold_fp={Prob.toNat r.threshold}"
  | .axiWellTypedV1 r =>
      IO.println s!"ok: axi_well_typed module={r.moduleName} schemas={r.schemaCount} instances={r.instanceCount}"
  | .axiConstraintsOkV1 r =>
      IO.println s!"ok: axi_constraints_ok module={r.moduleName} constraints={r.constraintCount} checks={r.checkCount}"
  | .queryResultV4 r =>
      IO.println s!"ok: query_result_v4 rows={r.rowCount} exact_complete={r.exactComplete} prepared={r.preparedQueryDigest} answer={r.answerDigest}"
  | .normalizePathV2 r =>
      IO.println s!"ok: normalized path start={r.start} end={r.end_}"
  | .rewriteDerivationV3 r =>
      IO.println s!"ok: rewrite_derivation_v3 start={r.start} end={r.end_}"
  | .pathEquivV2 r =>
      IO.println s!"ok: path_equiv start={r.start} end={r.end_}"
  | .deltaFV1 r =>
      IO.println s!"ok: delta_f instance schema={r.pulledBack.schema} name={r.pulledBack.name}"

def verifierProtocolV2 : String := "axiograph-verifier-stdio-v2"
def verifierBuildIdV2 : String := "axiograph-verify-main-v3"

structure VerifierRequestV2 where
  nonce : String
  checkerSha256 : String
  moduleAxi : String
  certificateJson : String
  expectedPreparedQueryDigest : String
  expectedAnswerDigest : String

def validSha256Hex (value : String) : Bool :=
  value.length == 64 && Axiograph.Query.isLowerHex value

def parseVerifierRequestV2 (request : Lean.Json) : Except String VerifierRequestV2 := do
  Axiograph.requireExactFields request
    [ "version", "nonce", "checker_sha256", "module_axi", "certificate_json"
    , "expected_prepared_query_digest", "expected_answer_digest" ]
    [ "version", "nonce", "checker_sha256", "module_axi", "certificate_json"
    , "expected_prepared_query_digest", "expected_answer_digest" ]
  let version ← (← request.getObjVal? "version").getStr?
  if version != verifierProtocolV2 then
    throw s!"unsupported verifier protocol `{version}`"
  let nonce ← (← request.getObjVal? "nonce").getStr?
  if nonce.isEmpty || nonce.toUTF8.size > 128 then
    throw "verifier nonce must be non-empty and at most 128 UTF-8 bytes"
  let checkerSha256 ← (← request.getObjVal? "checker_sha256").getStr?
  if !validSha256Hex checkerSha256 then
    throw "checker_sha256 must be 64 lowercase hexadecimal characters"
  let moduleAxi ← (← request.getObjVal? "module_axi").getStr?
  if moduleAxi.toUTF8.size > maxAxiModuleBytes then
    throw s!"module_axi exceeds {maxAxiModuleBytes} bytes"
  let certificateJson ← (← request.getObjVal? "certificate_json").getStr?
  if certificateJson.toUTF8.size > maxCertificateBytes then
    throw s!"certificate_json exceeds {maxCertificateBytes} bytes"
  if !jsonNestingWithin certificateJson maxJsonNesting then
    throw s!"certificate_json nesting exceeds {maxJsonNesting} or is structurally unbalanced"
  let expectedPreparedQueryDigest ←
    (← request.getObjVal? "expected_prepared_query_digest").getStr?
  if !Axiograph.Query.isWellFormedV2Identity .query expectedPreparedQueryDigest then
    throw "expected_prepared_query_digest is malformed"
  let expectedAnswerDigest ← (← request.getObjVal? "expected_answer_digest").getStr?
  if !Axiograph.Query.isWellFormedV2Identity .answer expectedAnswerDigest then
    throw "expected_answer_digest is malformed"
  pure {
    nonce
    checkerSha256
    moduleAxi
    certificateJson
    expectedPreparedQueryDigest
    expectedAnswerDigest
  }

def certificateDigestTextV2 (certificateJson : String) : String :=
  match Axiograph.Identity.derive .certificate #[certificateJson.toUTF8] with
  | .ok digest => digest
  | .error _ => "invalid-certificate-v2-identity"

def verifierReceiptV2
    (nonce checkerSha anchorDigest certificateDigest preparedDigest answerDigest
      certificateKind claimKind decision message : String) : Lean.Json :=
  Lean.Json.mkObj
    [ ("version", .str verifierProtocolV2)
    , ("nonce", .str nonce)
    , ("checker_sha256", .str checkerSha)
    , ("checker_build_id", .str verifierBuildIdV2)
    , ("revision_digest_v2", .str anchorDigest)
    , ("certificate_digest_v2", .str certificateDigest)
    , ("prepared_query_digest_v1", .str preparedDigest)
    , ("answer_digest_v1", .str answerDigest)
    , ("certificate_kind", .str certificateKind)
    , ("claim_kind", .str claimKind)
    , ("decision", .str decision)
    , ("message", .str message)
    ]

def runStdioRequestV2 (request : Lean.Json) : IO UInt32 := do
  match parseVerifierRequestV2 request with
  | .error err =>
      IO.println (verifierReceiptV2 "" "" "" "" "" "" "unknown"
        "finite_exact_complete" "rejected" err).compress
      pure 1
  | .ok req =>
      let anchorDigest := Axiograph.Identity.revisionDigestText req.moduleAxi
      let certificateDigest := certificateDigestTextV2 req.certificateJson
      let certificateParsed := Lean.Json.parse req.certificateJson
      let verification : Except String Axiograph.Query.QueryResultV4 := do
        let module ←
          match Axiograph.Axi.AxiV1.parseAxiV1 req.moduleAxi with
          | .error err => throw s!"axi parse error at line {err.line}: {err.message}"
          | .ok module => pure module
        let certificateJson ← certificateParsed
        let envelope ← Axiograph.parseCertificateEnvelope certificateJson
        let some (.v2 anchor) := envelope.anchor?
          | throw "stdio V2 requires certificate envelope V3 with a V2 anchor"
        if anchor.revisionDigestV2 != anchorDigest then
          throw s!"certificate revision anchor mismatch: expected `{anchorDigest}`, got `{anchor.revisionDigestV2}`"
        let .queryResultV4 proof := envelope.certificate
          | throw "stdio V2 accepts only query_result_v4"
        Axiograph.Query.verifyQueryResultProofV4Anchored
          module
          req.expectedPreparedQueryDigest
          req.expectedAnswerDigest
          proof

      match verification with
      | .error err =>
          IO.println (verifierReceiptV2 req.nonce req.checkerSha256 anchorDigest
            certificateDigest req.expectedPreparedQueryDigest req.expectedAnswerDigest
            "query_result_v4" "finite_exact_complete" "rejected" err).compress
          pure 1
      | .ok result =>
          IO.println (verifierReceiptV2 req.nonce req.checkerSha256 anchorDigest
            certificateDigest result.preparedQueryDigest result.answerDigest
            "query_result_v4" result.claimKind "accepted"
            "exact finite query answer verified").compress
          pure 0

def runStdioV2 : IO UInt32 := do
  let stdin ← IO.getStdin
  match (← readStreamChecked stdin maxStdioRequestBytes "stdio verifier request") with
  | .error err =>
      IO.println (verifierReceiptV2 "" "" "" "" "" "" "unknown"
        "finite_exact_complete" "rejected" err).compress
      pure 1
  | .ok input =>
      if !jsonNestingWithin input maxJsonNesting then
        IO.println (verifierReceiptV2 "" "" "" "" "" "" "unknown"
          "finite_exact_complete" "rejected"
          s!"request JSON nesting exceeds {maxJsonNesting} or is structurally unbalanced").compress
        return 1
      match Lean.Json.parse input with
      | .error err =>
          IO.println (verifierReceiptV2 "" "" "" "" "" "" "unknown"
            "finite_exact_complete" "rejected" s!"request JSON parse error: {err}").compress
          pure 1
      | .ok request => runStdioRequestV2 request

def printRevisionDigestV2 (path : System.FilePath) : IO UInt32 := do
  match (← loadAxiV1Anchor path) with
  | .error err =>
      IO.eprintln err
      pure 1
  | .ok ctx =>
      IO.println ctx.revisionDigest
      pure 0

def main (args : List String) : IO UInt32 := do
  if args.length > maxFileInputs then
    IO.eprintln s!"too many verifier inputs: maximum is {maxFileInputs}"
    return 2
  match args with
  | ["--stdio-v2"] => runStdioV2
  | ["--revision-digest-v2", path] => printRevisionDigestV2 path
  | [] =>
      IO.eprintln "usage: axiograph_verify [module.axi ...] <certificate.json> [more.json ...]"
      pure 2
  | paths =>
      let mut anchors : Std.HashMap String AnchorContext := {}
      let mut anchorCount : Nat := 0
      let mut certPaths : Array System.FilePath := #[]

      -- First pass: load canonical `.axi` modules and collect certificate paths.
      for pathStr in paths do
        let path : System.FilePath := pathStr
        if path.extension == some "axi" then
          if anchorCount >= maxAnchorModules then
            IO.eprintln s!"too many `.axi` anchors: maximum is {maxAnchorModules}"
            return 2
          match (← loadAxiV1Anchor path) with
          | .error err =>
              IO.eprintln s!"module load failed ({path}): {err}"
              return 1
          | .ok ctx =>
              if anchors.contains ctx.revisionDigest then
                IO.eprintln s!"duplicate axi revision digest `{ctx.revisionDigest}` ({path})"
                return 1
              anchors := anchors.insert ctx.revisionDigest ctx
              anchorCount := anchorCount + 1
              IO.println s!"ok: loaded axi module revision={ctx.revisionDigest} file={path}"
        else
          certPaths := certPaths.push path

      if certPaths.isEmpty then
        IO.eprintln "no certificate inputs provided"
        return 2

      -- Second pass: verify certificates (optionally anchored).
      let mut exitCode : UInt32 := 0
      for path in certPaths do
        match (← readFileChecked path maxCertificateBytes) with
        | .error err =>
            IO.eprintln err
            exitCode := 1
        | .ok contents =>
            if !jsonNestingWithin contents maxJsonNesting then
              IO.eprintln s!"JSON nesting exceeds {maxJsonNesting} or is structurally unbalanced ({path})"
              exitCode := 1
              continue
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
