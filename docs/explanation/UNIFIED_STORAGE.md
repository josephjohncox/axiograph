# Unified Storage Layer

**Diataxis:** Explanation  
**Audience:** contributors

This page is retained as a short historical note. The old “unified storage”
prototype treated `.axi` and PathDB as co-equal write targets. That is no longer
the current architecture.

## Current Model

The current V1 rule is:

```text
canonical .axi
  -> KernelModuleIr
  -> SchemaCategoryIr + TheoryIr + InstanceFunctorIr
  -> typed reports / derived projections / optional Lean certificates
```

Canonical accepted `.axi` plus the compiled semantic IR is the meaning plane.
PathDB, `.axpd`, graph backends, RDF/SHACL exports, embeddings, and visualizers
are derived execution, evidence, validation, or projection layers.

Mutation of accepted semantics must flow through Axiograph review/promotion,
typed runtime checks, CQ/trust gates, and semantic VCS. Backend-native writes or
PathDB byte changes are drift until re-imported through that flow.

## Read Instead

- `docs/howto/CANONICAL_SEMANTIC_SPINE.md` for the current user workflow.
- `docs/howto/SNAPSHOT_STORE.md` for accepted-plane snapshots and PathDB WALs.
- `docs/reference/KERNEL_IR.md` for the compiled semantic IR.
- `docs/reference/SEMANTIC_VCS.md` for semantic history and reconciliation.
- `docs/reference/TRUSTED_KERNEL.md` for the verification boundary.

## Historical Scope

Any older text or examples referring to atomic dual writes, PathDB as semantic
authority, or snapshot exports as certificate anchors should be read as retired
prototype material. `PathDBExportV1` remains scoped to debug/parser/live-byte
parity, not ontology/query/certificate authority.
