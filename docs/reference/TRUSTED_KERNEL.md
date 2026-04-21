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
- “Certified query answers are complete.”

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

## Kernel Boundary

The trusted kernel is the import closure of the executable verifier target,
not the whole Lean tree.

Today that means the code path centered on:

- `lean/Axiograph/VerifyMain.lean`
- `lean/Axiograph/Certificate/Format.lean`
- `lean/Axiograph/Certificate/Check.lean`
- `lean/Axiograph/Prob/Verified.lean`
- `lean/Axiograph/Axi/AxiV1.lean`
- `lean/Axiograph/Axi/TypeCheck.lean`
- `lean/Axiograph/Axi/ConstraintsCheck.lean`

`lean/Axiograph/Axi/PathDBExportV1.lean` remains useful for reversible snapshot
roundtrip/parity work, but it is not in the current `VerifyMain` import
closure and should not be described as part of the active verifier kernel.

Adjacent theorem-bearing modules are important, but they are not automatically
part of the shipped runtime kernel unless imported by the verifier target.

## Trust Classes

### Runtime kernel

These modules participate directly in shipped certificate/module verification.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.VerifyMain` | CLI/runtime verifier entrypoint | runtime kernel |
| `Axiograph.Certificate.Format` | certificate syntax and versioned shapes | runtime kernel |
| `Axiograph.Certificate.Check` | certificate replay / checking | runtime kernel |
| `Axiograph.Prob.Verified` | fixed-point probability algebra | runtime kernel |
| `Axiograph.Axi.AxiV1` | canonical `.axi` parser for trusted gates | runtime kernel |
| `Axiograph.Axi.TypeCheck` | conservative `.axi` typechecking gate | runtime kernel |
| `Axiograph.Axi.ConstraintsCheck` | conservative certifiable constraint gate | runtime kernel |

### Transitional parity support

These modules matter for snapshot roundtrip/parity workflows, but they are not
currently imported by `VerifyMain` and therefore sit outside the shipped
verifier kernel boundary.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.Axi.PathDBExportV1` | reversible PathDB snapshot/export parser for parity tooling | transitional parity support |

### Theorem support

These modules provide strong mathematical support and should stay rigorous, but
they are distinct from the runtime kernel boundary unless imported by
`VerifyMain`.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.HoTT.FreeGroupoid` | typed path denotation into free groupoid | theorem support |
| `Axiograph.Certificate.PathRewriteSoundness` | rewrite preservation via typed retyping | theorem support |
| `Axiograph.HoTT.PathCongruence` | congruence support for path equalities | theorem support |

### Explanation-level / roadmap theory

These are useful design guides, but should not be cited as the current shipped
trusted semantics.

| Module | Role | Trust class |
| --- | --- | --- |
| `Axiograph.HoTT.Core` | foundational HoTT scaffold with axiomatized pieces | explanation / support |
| `Axiograph.HoTT.KnowledgeGraph` | higher-level path/knowledge graph scaffold | explanation / support |
| `Axiograph.HoTT.PathAlgebraProofs` | additional proof scaffolding | explanation / support |
| `Axiograph.Topos.Overview` | topos/sheaf explanation layer | explanation / roadmap |

## What The Kernel Must Check

The kernel is responsible for a small number of semantic tasks:

1. Parsing anchored canonical `.axi` inputs.
2. Checking a conservative well-typedness gate for canonical `.axi` modules.
3. Checking a conservative certifiable constraint subset.
4. Checking typed path/rewrite certificates.
5. Checking fixed-point confidence/probability arithmetic used by certificates.
6. Checking anchored membership of referenced facts/relations inside the chosen
   anchor format.
7. Checking the certifiable well-formedness of the compiled schema/category IR
   slice that prepared queries, migrations, and certificates cite.
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
- WAL replay mechanics
- heuristic reconciliation
- LLM/world-model scoring
- embedding search
- general RDF/OWL entailment
- explanation-level topos/sheaf/modal machinery
- storage/index performance logic

## Runtime Engineering Outputs Outside The Kernel

The following outputs are important, but they belong to the runtime checker /
report layer rather than to the trusted kernel unless they embed a checked
certificate or gate result:

- business-rule applicability reports
- semantic coverage / drift reports
- typed authoring and evolution-preview bundles
- coding-agent semantic reports
- SHACL/RDF/olog alignment reports
- backend pushdown / projected read-surface contracts
- authoring/query typed-hole and refinement-candidate reports

These outputs may cite kernel-checked anchors, certificates, and IR ids. That
does not make the whole report trusted. The correct product language is still
`certified`, `mixed`, `execution-only`, or `evidence-grounded` according to the
stated contract. Structured typed holes are especially important to classify
correctly: they are runtime checker/exploration artifacts over compiled IR and
anchors, not trusted semantic proofs. The same applies to the new shared
query+authoring refinement protocol: it is one runtime repair/apply surface
over compiled IR, not a new kernel claim.

Projected backend read surfaces deserve the same conservatism as query
certificates:

- they may preserve useful lower-tier interfaces,
- they may preserve typed transport structure faithfully,
- but they still do **not** imply complete answers or ontology closure unless a
  stronger claim is separately formalized.

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
- `recompute-scaffold`
  - Lean currently recomputes or rederives enough to guard the result, but the
    long-term semantic story is not yet fully internalized.

Current target classification:

| Certificate kind | Status | Notes |
| --- | --- | --- |
| `reachability_v1` | replay-only | transitional float-based form |
| `reachability_v2` | replay-only | fixed-point arithmetic is stronger; historical `PathDBExportV1`-anchored form |
| `reachability_v3` | replay-only | canonical `.axi`-anchored reachability with stable fact ids |
| `axi_well_typed_v1` | decision-procedure | conservative `.axi` module gate |
| `axi_constraints_ok_v1` | decision-procedure | conservative certifiable subset only |
| `normalize_path_v2` | replay-only | valid narrow kernel slice |
| `path_equiv_v2` | replay-only | narrow typed path equality slice |
| `rewrite_derivation_v3` | replay-only moving toward theorem-backed | should consume checked rewrite rules |
| `query_result_v3` | replay-only / partial | row soundness only, not completeness |
| `delta_f_v1` | recompute-scaffold | not yet a final semantic story |

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

- query completeness,
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
5. Prefer canonical accepted-plane anchors over `PathDBExportV1` for more
   certificate flows.
6. Move query/cert semantics off binary-projection heuristics and onto the
   future kernel IR.
