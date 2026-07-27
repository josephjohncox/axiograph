# Trusted Kernel

**Diataxis:** Reference  
**Audience:** contributors

This document defines the target boundary of Axiograph's trusted semantics.

The core rule is simple:

- **Rust** computes and may be clever.
- **Lean** checks the semantic justification for the narrow set of things we trust.
- Everything else is either:
  - an optimization,
  - an adapter,
  - a heuristic,
  - or explanation-level theory.

## Kernel Claim

The intended product-facing claim is:

> Axiograph has a Lean-checked core for typed rewrite/path witnesses and
> conservative ontology gates, with a roadmap toward richer categorical
> ontology semantics.

Claims to avoid:

- “Axiograph already has a full HoTT ontology kernel.”
- “Rust is the trusted semantics layer.”
- “PathDB storage format defines ontology meaning.”
- “Every certified query answer is complete.” The only completeness theorem is
  `query_result_v4` over its declared bounded finite denotation.

## When A Stronger Claim Becomes Honest

The phrase “dependently typed ontology engine” should be reserved for a more
specific state than “there is some theorem-bearing Lean code in the repo”.

In this repo, the stronger claim becomes justified only when:

- the verifier checks the certifiable semantic acceptance boundary for canonical
  `.axi`, checked rewrite rules, and narrow query/migration/certificate
  obligations;
- compiled schema/category IR objects are the stable semantic references that
  certificates and prepared queries cite;
- Rust operational handles are indexed by accepted anchors, lifecycle state, and
  schema/theory/context ids rather than raw strings alone;
- and authoring/query/migration/certification surfaces all reduce to the same
  typed semantic objects.

This still does **not** mean:

- all execution moves into Lean,
- every query is complete,
- or the storage/query engine itself becomes part of the trusted kernel.

It means the semantics-bearing seams are typed, anchored, and checked at a small
explicit boundary.

## Operational Security Is Not Semantic Trust

Filesystem no-follow checks, byte/count/depth limits, process groups, DNS
pinning, server semaphores, authenticated SQLite receipts, and strict release
archives protect runtime boundaries. They do not make Rust a trusted semantics
layer and do not promote runtime output into accepted meaning. The complete
resource and mutation contract is in
`docs/reference/SECURITY_BOUNDARIES.md`.

A result still needs the appropriate accepted `.axi` anchor and checked
certificate when it makes a trusted semantic claim. Conversely, a Lean theorem
does not remove the need to bound the parser, process, network, and storage path
that carries its inputs and receipt.

## Kernel Boundary

The trusted kernel is the import closure of the executable verifier target,
not the whole Lean tree.

Today that means the code path centered on:

- `lean/Axiograph/VerifyMain.lean`
- `lean/Axiograph/Certificate/Format.lean`
- `lean/Axiograph/Certificate/Check.lean`
- `lean/Axiograph/Certificate/Invariants.lean`
- `lean/Axiograph/Certificate/PathRewriteSoundness.lean`
- `lean/Axiograph/Theory/Finite.lean`
- `lean/Axiograph/HoTT/FreeGroupoid.lean`
- `lean/Axiograph/Prob/Verified.lean`
- `lean/Axiograph/Axi/AxiV1.lean`
- `lean/Axiograph/Axi/TypeCheck.lean`
- `lean/Axiograph/Axi/ConstraintsCheck.lean`
- `lean/Axiograph/Util/Sha256.lean`

Storage snapshot parity modules are deliberately outside this boundary. They
are not semantic input, query authority, certificate authority, or
accepted-plane promotion surfaces.

Adjacent theorem-bearing modules are important, but they are not automatically
part of the shipped runtime kernel unless imported by the verifier target.

## Canonical Compiler Boundary

`axiograph_kernel::CanonicalCompiler` is the single Rust compiler for accepted
exact-byte `.axi` packages. Its `CompiledKernelSnapshot` binds
`KernelSnapshotIr`, `SchemaPresentationIr`, and finite `InstanceModelIr` to an
explicit repository id and accepted snapshot id. This compiler is
non-authoritative in the proof-theoretic sense: Rust checks formation and the
supported finite-model fragment, while the Lean verifier remains the trusted
checker for the narrower certified fragment.

PathDB runtime indexes are derived only after the canonical compiler succeeds.
They cannot mint accepted handles or redefine schema meaning. A successful Rust
finite-model check is not a Lean certificate, a completeness claim, or a global
ontology-closure claim. The compiler exports one anchored
`category_kernel_v3` certificate containing its finite presentation,
contextual equation-congruence witnesses, both formal inverse-law
normalization traces for every generator, and generator-reachability
explanations. Lean reconstructs the presentation from the exact `.axi` schema,
requires exact equality with the compiler projection, and replays the witnesses;
it does not trust the serialized Rust IR. Formal inverses are proof syntax, not
claims that a non-reversible ontology relation can be traversed backward.

## Merge Trust Boundary

`axiograph_store::SemReconciliationV2` is the finite accepted-merge contract.
Rust recompiles reviewed parent and result candidates from exact accepted `.axi`
bytes, requires a passing `Merge` finite-theory replay receipt, checks one
payload fingerprint for every `KernelRefV2`, requires typed
keep/drop/introduce/transport accounting, binds exact canonical/CQ/trust/theory
reports, and permits protected-main materialization only through an exact
`[target_tip, source_tip]` `SemCommitV2` authenticated against current refs.

This strengthens auditability and prevents address-only union checks. It does
not move merge semantics into the `VerifyMain` closure. Rust remains untrusted;
a transport witness digest is only a commitment until a checker in the trusted
closure accepts the supported witness family. The implemented theorem is the
strongest finite decidable fragment currently justified: exact payload-union
accounting under compiled typed refs. It does not claim arbitrary categorical
pushouts/colimits, general dependent transport, univalence, higher inductive
types, arbitrary higher paths, open-world entailment, or ontology closure.

## Cryptographic Identity Status

`Axiograph.Util.Sha256` is now in the `VerifyMain` import closure. The checker
independently computes `RevisionDigestV2` from the exact accepted UTF-8 bytes it
loads. Rust and Lean use the same domain-separated, length-framed preimage and
the full SHA-256 output:

```text
axi:revision:v2:sha256:<64-lowercase-hex>
```

The accepted bytes remain the authority. The digest identifies those bytes; it
does not replace them. Rust remains an untrusted producer.

Envelope V3 uses this identity for `query_result_v4` and
`category_kernel_v3`. The checker recomputes `revision_digest_v2` from the exact
accepted bytes. Query verification also compares it with the caller-bound
receipt. Category-kernel file-mode verification uses the same V2 anchor to
reconstruct one named schema and check the emitted presentation, congruence, and
complete bounded generator saturation. V3 admits only these two strict payload
families; a prefix change or ignored field cannot reinterpret another
certificate kind. The query boundary has no FNV anchor or legacy query reader.

The additive Semantic VCS lineage modules reuse the same SHA-256 implementation
and canonical framing, but they do **not** enlarge this kernel. In particular,
`lean/Axiograph/SemanticVCS/Lineage.lean` and its dispatch through
`SemanticVCS/CheckMain.lean` are not imported by `VerifyMain`. They verify V2
commit integrity and ancestry only when invoked separately with a
caller-supplied expected repository id or head. Promoting them into the trusted
closure requires a separate import and trust review.

## Verifier Process Contract

Query-bound verification invokes `axiograph_verify --stdio-v2` through the
fail-closed process bridge. The protocol name is
`axiograph-verifier-stdio-v2`; the approved build id is
`axiograph-verify-main-v3`.

A result is accepted only when all of these checks pass:

- the executable SHA-256 matches the operator-approved value; the bridge stages
  those already-hashed bytes in a private directory and executes the staged
  copy rather than reopening the configured pathname;
- the receipt echoes that SHA-256 and the approved exact-finite build id;
- the typed UUID nonce matches the request;
- Lean recomputes the exact-byte `RevisionDigestV2` anchor;
- the exact certificate text has the expected cryptographic certificate digest;
- the non-null typed prepared-query and answer digests match the caller,
  certificate, Lean recomputation, and receipt;
- the kind is `query_result_v4`, the claim kind is
  `finite_exact_complete`, the decision is `accepted`, and process status
  agrees; and
- execution stays within configured time and I/O limits.

Missing, null, non-string, malformed, unknown, or old-protocol fields reject. A
successful process exit without a matching full `VerifierReceiptV2` has no
verification meaning. The stdio V1 reader was deleted.

The server `/status` response keeps binary discovery separate from verification
readiness. It reports the V2 protocol and expected build id, whether an actual
checker file is available and executable, and whether an approved SHA-256,
approved build id, and positive timeout were supplied. A neighboring
`axiograph_verify.sha256` file is package integrity metadata, not operator
approval; the server never reads it as verification authority.

The receipt proves witness soundness and exact answer completeness for the
bounded finite query denotation reconstructed from the accepted module. It does
not prove frontend source-to-lowering equivalence, open-world ontology closure,
evidence exhaustiveness, approximate-search completeness, or unrestricted
category/dependent/HoTT semantics.

### Regulated-shipment trust split

`make verify-regulated-shipment` is the primary cross-layer fixture. The
`VerifyMain` closure checks its anchored `axi_well_typed_v1`,
`axi_constraints_ok_v1`, and `query_result_v4` claims. For the query, acceptance
means exactly that `CoA_RX_42` is the complete answer to the declared two-hop
finite path denotation; the missing-row adversary must reject.

The matching `Axiograph.Theory.Finite` shipment presentation checks relation
objects, projection arrows in declared order, a parallel path equation, finite
reviewer refinement, checked hole lifecycle, saturation, and explanation replay.
`make verify-lean-e2e-category-kernel-v3` makes Rust emit the anchored
regulated-shipment category certificate; `VerifyMain` reconstructs its 23
objects, 43 arrows, identities, and one parallel equation, replays contextual
congruence and all 70 reachable endpoint explanations, and rejects presentation,
congruence, and saturation tampering. The workflow also runs `axiograph check
finite-query` for baseline and candidate. Its report carries an accepted V2
verifier receipt bound to the exact revision, prepared query, answer, and
certificate. That parsed report is the trust-gate object cited by the reviewed
candidate and merged protected-main commit. Placeholder or cross-answer receipt
identities reject before AxiStore is initialized. This trusted claim does not
include Rust's complete `InstanceModelIr`, relation-span equation lowering,
merge plan, or backend projection. Those remain operational evidence.

Release packaging does not enlarge the kernel. `make release-gate` is the
single binary/container publication decision, but archive hashes, executable
modes, image digests, PathDB smokes, and registry manifests are operational
integrity checks. They do not replace the exact accepted `.axi` bytes or import
new Lean modules into the `VerifyMain` closure. A release lane is supported only
when that exact lane records a successful packaged anchored smoke.

## Trust Classes

### Runtime kernel

These modules participate directly in shipped certificate/module verification.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.VerifyMain` | CLI/runtime verifier entrypoint | runtime kernel |
| `Axiograph.Certificate.Format` | certificate syntax and versioned shapes | runtime kernel |
| `Axiograph.Certificate.Check` | certificate replay / checking | runtime kernel |
| `Axiograph.Certificate.Invariants` | acceptance-to-semantics theorems for path normalization, rewrite replay, path equivalence, and resolution | runtime-kernel theorem closure |
| `Axiograph.Certificate.PathRewriteSoundness` | endpoint retyping and free-groupoid denotation preservation for every builtin path rewrite | runtime-kernel theorem closure through `Certificate.Invariants` |
| `Axiograph.Theory.Finite` | anchored finite category reconstruction, equation congruence, exact formal inverse-law normalization traces, and explanation-certified generator saturation | runtime kernel for `category_kernel_v3`; broader definitions are theorem support |
| `Axiograph.HoTT.FreeGroupoid` | constructive free-groupoid denotation used by the finite module | runtime-kernel import closure; no full HoTT claim |
| `Axiograph.Prob.Verified` | bounded fixed-point probability arithmetic | runtime kernel |
| `Axiograph.Axi.AxiV1` | canonical `.axi` parser for trusted gates | runtime kernel |
| `Axiograph.Util.Sha256` | exact-byte SHA-256 and V2 identity recomputation | runtime kernel |
| `Axiograph.Axi.TypeCheck` | conservative `.axi` typechecking gate | runtime kernel |
| `Axiograph.Axi.ConstraintsCheck` | conservative certifiable constraint gate | runtime kernel |

### Theorem support

These modules provide strong mathematical support and should stay rigorous, but
they are distinct from the runtime kernel boundary unless imported by
`VerifyMain`.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.HoTT.PathCongruence` | additional congruence support for path equalities | theorem support |
| `Axiograph.SemanticVCS` | finite semantic-slice join/meet, merge/rebase materialization, and preservation model for the external conformance slice | theorem support / external conformance |
| `Axiograph.SemanticVCS.Json` | JSON contract for Lean-checked merge/rebase plan exports consumed by `CheckMain` | theorem support / external conformance |
| `Axiograph.SemanticVCS.CheckMain` | executable checker for reduced finite merge/rebase payloads | conformance checker / not `VerifyMain` trusted boundary |
| `axiograph-store` lineage and restart validation | authenticated runtime integrity under exact state/subject pins | Rust operational check / no Lean authority |

### Explanation-level / roadmap theory

These are useful design guides, but should not be cited as the current shipped
trusted semantics.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.HoTT.Core` | constructive equality/equivalence/dependent-path vocabulary; univalence is an explicit non-claim and no first-party axiom is introduced | explanation / support |
| `Axiograph.HoTT.KnowledgeGraph` | higher-level path/knowledge-graph model sketch | explanation / support |
| `Axiograph.HoTT.PathAlgebraProofs` | additional proof experiments for path-algebra claims | explanation / support |
| `Axiograph.Topos.Overview` | topos/sheaf explanation layer | explanation / roadmap |

For the current status of what is encoded in Lean, what is in the shipped
verifier boundary, and what remains runtime-only, see
`docs/reference/LEAN_THEORY_EVALUATION.md`.

## Neighbor Surfaces

These surfaces are first-class runtime or review interfaces, but they are not
the trusted kernel unless they reduce to an accepted anchor and a checked
certificate/gate:

- Canonical schema/category/theory IR: `docs/reference/KERNEL_IR.md`.
- Runtime theory checker reports and finite admissibility scopes:
  `docs/reference/RUNTIME_THEORY_CHECKER.md`.
- Software-authoring/codegen and continuous coverage reports:
  `docs/reference/SOFTWARE_AUTHORING_TOOLS.md`.
- Semantic VCS refs, merge/rebase plans, reconciliation, and backend projection
  manifests: `docs/reference/SEMANTIC_VCS.md`.
- Embedding/RAG/vector sidecars and evidence promotion:
  `docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.
- Runtime I/O, resource, network, and mutation boundary:
  `docs/reference/SECURITY_BOUNDARIES.md`.
- Verification and test gates: `docs/howto/FORMAL_VERIFICATION.md` and
  `docs/howto/TESTING.md`.

The canonical cleanup spine is accepted `.axi` → compiled IR/runtime typed
reports → checked certificate/gate when a trusted claim is needed. Runtime
reports may be strong engineering evidence, but they are not Lean-certified
claims unless the certificate/gate is present and checked.

## What The Kernel Must Check

The kernel is responsible for a small number of semantic tasks:

1. Parsing anchored canonical `.axi` inputs.
2. Checking a conservative well-typedness gate for canonical `.axi` modules.
3. Checking a conservative certifiable constraint subset.
4. Checking typed path/rewrite certificates.
5. Checking bounded fixed-point confidence/probability arithmetic used by
   certificates. Per-composition rounding is deterministic but not associative,
   so path-associativity does not imply confidence equality.
6. Checking anchored membership of referenced facts/relations inside the chosen
   anchor format.
7. Reconstructing the finite relation-object category directly from exact
   anchored `.axi` and checking replayable generator-saturation explanations.
   The checker does not deserialize or trust the complete Rust kernel IR.
8. Checking narrow typed query and migration witnesses for explicitly supported
   fragments, with soundness scope stated conservatively.

## What The Kernel Must Not Own

The kernel must not become the place where all semantics-related code goes.

Keep these outside:

- query planning
- join ordering
- PathDB byte layout
- Rust-side lifecycle/anchor/kernel-IR modules such as
  `rust/crates/axiograph-pathdb/src/anchor.rs`,
  `rust/crates/axiograph-pathdb/src/lifecycle.rs`,
  `rust/crates/axiograph-pathdb/src/kernel_ir.rs`, and the fail-closed `.axi`
  import/typecheck wrappers
- SQLite transaction, journal, and crash-recovery mechanics
- heuristic reconciliation
- LLM/proposal-adapter scoring
- embedding search and embedding-derived relationship suggestions
- general RDF/OWL entailment
- explanation-level topos/sheaf/modal machinery
- storage/index performance logic

For the embedding/RAG sidecar boundary, use
`docs/reference/EMBEDDINGS_AND_EVIDENCE.md`.

## Runtime Engineering Outputs Outside The Kernel

The following outputs are important, but they belong to the runtime checker /
report layer rather than to the trusted kernel unless they embed a checked
certificate or gate result:

- business-rule applicability reports
- semantic coverage / drift reports
- typed authoring and evolution-preview bundles
- coding-agent semantic reports
- SHACL/RDF/olog alignment reports
- `axiograph-projections` manifests, native artifacts, semantic-loss reports,
  and backend readback reports
- authoring/query typed-hole and refinement-candidate reports

These outputs may cite kernel-checked anchors, certificates, and IR ids. That
does not make the whole report trusted. The correct product language is still
`certified`, `mixed`, `execution-only`, or `evidence-grounded` according to the
stated contract. Structured typed holes are especially important to classify
correctly: runtime hole handles are checker/exploration artifacts over compiled
IR and anchors, not trusted semantic proofs. The dependent `TypedPathHole` in
`Axiograph.Theory.Finite` proves only that its candidates share the expected
endpoints; choosing or promoting a candidate remains an external reviewed
operation. The same applies to the new shared
query+authoring refinement protocol: it is one runtime repair/apply surface
over compiled IR, not a new kernel claim.

`authoring_workspace_report_v1` now carries that runtime boundary consistently
across CLI, LSP, MCP, and HTTP. Its canonical-compilation status, typed holes,
repairs, finite CQ results, prepared-query explanation, payload-fingerprint
diff, olog path checks, runtime-theory report, and promotion review remain
outside the trusted checker. Its embedded finite-theory receipt exposes typed
scope, exact coverage, residuals, and non-claims for category formation,
refinements, saturation explanations, contexts, and identity transports.
`candidate_reviewable=true` is a finite operational status; review-only theory
obligations still block promotion. `protected_main_eligible` remains false until
a separate VerifyMain receipt and complete `AxiStore::PromotionPlan` exist. No
transport adapter can turn the report into a trusted receipt.

The finite payload diff establishes equality or change of encoded compiled IR
payloads under stable typed ids. It does not establish categorical equivalence,
naturality, univalence, higher-path equality, rewrite confluence, ontology
closure, or open-world completeness.

Projected backend read surfaces deserve the same conservatism as query
certificates. `ProjectionManifestV1` is anchored to one immutable
`CompiledKernelSnapshot` and covers one decidable finite record set.
`ReadbackReportV1::ExactFiniteRecordMatch` means only that every expected
record id had the expected payload fingerprint and no extra record was
observed. It does not discharge refinement predicates, constraints, equations,
rewrites, inverse laws, higher paths, or completeness obligations.

The readback payload is always wrapped in `ExternalEvidenceEnvelopeV1` with the
only authority variant `EvidenceOnly`; `accepted_state_change` is false. No
PathDB, TypeDB, TerminusDB, RDF/OWL, or property-graph write can change accepted
state. New backend observations must re-enter through typed proposal, review,
CQ/trust gates, reconciliation, and promotion. A backend branch, transaction,
SHACL result, OWL inference, TypeQL answer, SPARQL answer, Cypher traversal, or
PathDB result is not a VerifyMain receipt.

## Certificate Status Model

Each certificate kind should be classified using one of these statuses:

- `theorem-backed`
  - Lean checks a witness against a semantic model with a real preservation
    theorem behind the replay.
- `decision-procedure`
  - Lean decides a conservative gate directly.
- `replay-only`
  - Lean checks a structured witness chain but the broader semantics are still
    intentionally narrow.
- `recomputed-witness`
  - Lean recomputes the anchored finite witness under an explicit narrow claim.

`category_kernel_v3` is dispatched by `VerifyMain` under an exact-byte accepted
`.axi` anchor. That dispatch covers exact presentation reconstruction, ordered
projections and identities, typed composition, parallel equations, contextual
congruence, both endpoint-indexed inverse-law normalization traces for every
generator, and complete bounded generator reachability. The broader
interpretation, refinement, context, hole, and transport definitions in
`Axiograph.Theory.Finite` remain theorem support unless a certificate family
invokes them.

The semantic laws proved for dependent `GroupoidPath` values do not yet lift
`category_kernel_v3` acceptance into a denotational proposition. The wire
checker uses separate index-based path data. Until a wire-to-`GroupoidPath`
retyping theorem and cancellation-preservation theorem are imported through the
acceptance path, this family is replay-only.

Current target classification:

| Certificate kind | Status | Notes |
| --- | --- | --- |
| `axi_well_typed_v1` | decision-procedure | conservative `.axi` module gate |
| `axi_constraints_ok_v1` | decision-procedure | conservative certifiable subset only |
| `query_result_v4` | decision-procedure + witness replay / query-and-answer-bound | exact completeness for the declared bounded finite UCQ/RPQ denotation under accepted revision, prepared-query, and answer digests; no ontology-closure claim |
| `category_kernel_v3` | decision-procedure + replay-only / finite category presentation | reconstructs one bounded schema presentation from exact `.axi`, checks identities, typed composition, equations and contextual congruence, syntactically replays both formal inverse cancellations for every generator, and requires exact generator-reachability closure; the wire paths are not retyped as `GroupoidPath`, so acceptance currently has no denotation-preservation theorem and makes no data-level reversibility, fact-closure, or rewrite-closure claim |
| `reachability_v3` | replay-only | canonical `.axi`-anchored reachability with stable fact ids |
| `rewrite_derivation_v3` | replay-only moving toward theorem-backed | should consume checked rewrite rules |
| `normalize_path_v2` | theorem-backed | mandatory endpoint-preserving rewrite trace; for the endpoint-retyped input, accepted replay preserves free-groupoid denotation |
| `path_equiv_v2` | theorem-backed | mandatory traces to one normal form; for endpoint-retyped inputs, accepted replay implies equal free-groupoid denotation |
| `resolution_v2` | recomputed-witness | finite fixed-point reconciliation decision |
| `delta_f_v1` | recomputed-witness | narrow finite transport check, not full migration semantics |

## Accepted Rewrite Rules

User-authored rewrite rules are not supposed to be “proved from pure
foundations” inside the kernel. They are accepted ontology inputs that the
kernel must validate conservatively before allowing certificates to use them.

The target checked-rewrite layer should ensure:

- declared variables are well-scoped,
- endpoints are preserved,
- schema sorts line up,
- referenced arrows/roles are valid,
- and compiled checked rules are what certificates consume.

This is the correct trust boundary for ontology work:

- user-authored rewrite rules live in the accepted meaning plane,
- Lean checks they are well-formed enough to be trusted inputs,
- certificates may then cite them.

## Soundness Contract

When the verifier accepts a certificate or gate, the intended contract is:

- the claimed result is **sound with respect to the narrow checked semantics**,
- the result is **anchored** to a declared input,
- and any untrusted runtime optimization has been reduced to a checkable witness.

The verifier does **not** imply:

- query completeness outside a certificate family such as `query_result_v4`
  whose checked claim is explicitly finite and exact,
- closed-world truth,
- full ontology closure,
- or correctness of the entire storage/query engine.

## Planned Tightening

The next concrete tightening steps are:

1. Add a CI/import audit for the verifier target so the runtime kernel boundary
   stays explicit.
2. Add a checked rewrite-rule compilation layer in Lean and route
   `rewrite_derivation_v3` through it.
3. Add a certifiable schema/category IR well-formedness boundary so prepared
   queries and certificates stop depending on runtime-only heuristic naming.
4. Add narrow typed query/migration witness checking over that IR for clearly
   stated certifiable fragments.
5. Keep certificate/query flows on canonical accepted-plane anchors and compiled
   IR refs.
6. Move remaining query/cert semantics off binary-projection heuristics and
   onto the canonical `CompiledKernelSnapshot` plus validated derived citations.
