import Axiograph.Util.Sha256

/-!
# AXIOGRAPH-ID registry

This is the Lean side of the single closed identity registry. The framing and
SHA-256 implementation live in `Axiograph.Util.Sha256`; production callers use
this domain type rather than supplying arbitrary domain strings.
-/

namespace Axiograph.Identity

inductive Domain where
  | repository | module | semanticKey | revision | schema | object | relation
  | role | theory | obligation | instance | constraint | equation | rewrite
  | fact | tree | snapshot | commit | reconciliation | objectBlob
  | materialization | query | answer | certificate | proposal | run | checker
  deriving Repr, DecidableEq

def Domain.asString : Domain → String
  | .repository => "repository"
  | .module => "module"
  | .semanticKey => "semantic-key"
  | .revision => "revision"
  | .schema => "schema"
  | .object => "object"
  | .relation => "relation"
  | .role => "role"
  | .theory => "theory"
  | .obligation => "obligation"
  | .instance => "instance"
  | .constraint => "constraint"
  | .equation => "equation"
  | .rewrite => "rewrite"
  | .fact => "fact"
  | .tree => "tree"
  | .snapshot => "snapshot"
  | .commit => "commit"
  | .reconciliation => "reconciliation"
  | .objectBlob => "object-blob"
  | .materialization => "materialization"
  | .query => "query"
  | .answer => "answer"
  | .certificate => "certificate"
  | .proposal => "proposal"
  | .run => "run"
  | .checker => "checker"

def Domain.all : Array Domain := #[
  .repository, .module, .semanticKey, .revision, .schema, .object, .relation,
  .role, .theory, .obligation, .instance, .constraint, .equation, .rewrite,
  .fact, .tree, .snapshot, .commit, .reconciliation, .objectBlob,
  .materialization, .query, .answer, .certificate, .proposal, .run, .checker
]

def derive (domain : Domain) (fields : Array ByteArray) : Except String String := do
  let preimage ← Axiograph.Util.Sha256.canonicalIdentityPreimageV2 domain.asString fields
  return s!"axi:{domain.asString}:v2:sha256:{Axiograph.Util.Sha256.toHex (Axiograph.Util.Sha256.hashBytes preimage)}"

def revisionDigestBytes (bytes : ByteArray) : Except String String :=
  derive .revision #[bytes]

def revisionDigestText (text : String) : String :=
  match revisionDigestBytes text.toUTF8 with
  | .ok digest => digest
  | .error _ => "invalid-revision-v2-identity"

def runtimeFactIdV2
    (moduleName schemaName instanceName relationName : String)
    (fieldsInDeclOrder : Array (String × String)) : String :=
  Id.run do
    let mut fields := #[
      moduleName.toUTF8,
      schemaName.toUTF8,
      instanceName.toUTF8,
      relationName.toUTF8
    ]
    for (field, value) in fieldsInDeclOrder do
      fields := fields.push field.toUTF8
      fields := fields.push value.toUTF8
    return match derive .fact fields with
      | .ok digest => digest
      | .error _ => "invalid-fact-v2-identity"

private def isLowerHexByte (byte : UInt8) : Bool :=
  (byte >= 0x30 && byte <= 0x39) || (byte >= 0x61 && byte <= 0x66)

def validWireFor (domain : Domain) (value : String) : Bool :=
  let wirePrefix := s!"axi:{domain.asString}:v2:sha256:"
  if !value.startsWith wirePrefix then false
  else
    let suffix := (value.drop wirePrefix.length).toString.toUTF8
    suffix.size == 64 && suffix.foldl (init := true) (fun ok byte => ok && isLowerHexByte byte)

private def parityExpected : Array String := #[
  "8a34106f978078d690364db0bad69d2e269fa7cbb35c325e99f6846ceb06d592",
  "2b409543af231f72fd8cc72ce4688ff02b1356343127c593ae6e9439c7b6aeee",
  "903c35101dfbfb73d69eb4545b9f33c5b858de32167739bbfde642a280a7cba8",
  "a7ce11aff75b4326bd06f0804c9a32ee057e2e17bb32f770e8f6ee7b2d7e622e",
  "c6f2bfba197a47f84ceea97390bf3d84f0211dbe0c172e7f95c82c55cd595cfd",
  "1c98766fbf321e12141f23196372273721bbb13008522c5dc90499e92501ee51",
  "abe43dddd80b68ef714b02344338133d736c60b2c7280e7b1b3918914ca1a826",
  "00722a929b1158131b850fadb64cff49b3bd31694cdbb188eb0d05812a2fe3bb",
  "4856a265bd932d2b53d1b800a8446a4ae5fdcbe6bfff245eb0e498763230e014",
  "6bff41d73b9cc9049ee089a35372daf25555067a8bcb46a44558b3ea58ebba5c",
  "880b54149cf0b65aafc272255819c1b19c7eeb2011c927707ff666bdd9339ddf",
  "1b1a8629443499055e827349508f540a936a28c6ce5b5f07bc0f0e2bb56b840c",
  "4aaf29b22021245b232c6b1440bae75d49e768db6386f5c9f6ac2876ceead7d3",
  "63c390da4e035602eb7a8912a71c2df0102bae1b2ce9e50b0ff2cce3d047c3f6",
  "78b3d493bd829a69e05be749c7ab7e9b036f2bd812082ac54f893f1c0cde9750",
  "15210fbebd387cb5c3e4d68e9a9fb03eab96c909eacc65bfb32f66df2e5571db",
  "7515f7ab415e1446322f478b5b7ab34328f69c00b9d76d7d81a0f0e74696ab26",
  "26143f9826097380221e2fc5d0a324d5db99a5792fe25df9aa61ae96e44cefa9",
  "0725fb86552e7173c31f49cf8b6df10b2373e7438226e8bb3703f294d5f6e6ca",
  "43ec352ed54b640cb2c253fc7667e6d776f2a77dfb4bea1271620d9209dd410c",
  "58c68b9b4e02a0ed6d32be863978729d9adfb568a58828ce66128b47ee4a5d91",
  "c9941b524ea638ab5926fc68666ffa567d0fe33333d8acb2942dd70bdbc0abc2",
  "81c80e5ed4f837d260a87d984ca7e0a47b3e714c5c13fd48175674c5cc468ec3",
  "264875b9d06b94a659bbf972a14a2743b9feeb40cfda5b3d9064287898f583c7",
  "488add43ef52e6b744aa8fc6d09b1f64dec0cdd84a7525cc0f340b2b878f6ec3",
  "b28ca681c858c2bcb1d28e68b8aaccde3b216df494d8f1c6b48d149bb8b5f6bc",
  "0e995b927377670dd7cde7dce9df7b8027d15ec5ed4affb6a56257e7d9a49367"
]

private def deriveOrEmpty (domain : Domain) (fields : Array ByteArray) : String :=
  match derive domain fields with
  | .ok wire => wire
  | .error _ => ""

private def parityVector (domain : Domain) : String :=
  deriveOrEmpty domain #[domain.asString.toUTF8, "λ|🧠".toUTF8, ByteArray.mk #[0, 255]]

example : Domain.all.size = parityExpected.size := by native_decide

example : (Domain.all.zip parityExpected).all (fun (domain, expected) =>
    parityVector domain = s!"axi:{domain.asString}:v2:sha256:{expected}") := by
  native_decide

example : validWireFor .revision (revisionDigestText "module X\n") := by native_decide
example : !validWireFor .fact (revisionDigestText "module X\n") := by native_decide
example : !validWireFor .revision
    "axi:revision:v2:sha256:ABCDEF0000000000000000000000000000000000000000000000000000" := by
  native_decide
example : !validWireFor .revision "axi:revision:v2:sha256:1234" := by native_decide

example :
    deriveOrEmpty .query #["a".toUTF8, "b|c".toUTF8] !=
    deriveOrEmpty .query #["a|b".toUTF8, "c".toUTF8] := by
  native_decide

example :
    deriveOrEmpty .query #["a".toUTF8, "b".toUTF8, "c".toUTF8] !=
    deriveOrEmpty .query #["a".toUTF8, "b|c".toUTF8] := by
  native_decide

end Axiograph.Identity
