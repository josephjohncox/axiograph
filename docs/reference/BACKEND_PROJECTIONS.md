# Typed Backend Projections

## Contract

Backend projections are derived execution and inspection surfaces. They are not
ontology authority.

The only projection input is an immutable `CompiledKernelSnapshot` produced
from the exact accepted `.axi` import closure. `axiograph-projections` does not
compile, infer, or maintain a second schema model. It emits
`ProjectionManifestV1`, anchored by:

- `RepositoryIdV2`;
- accepted `SnapshotIdV2`;
- compiled `KernelSnapshotIr` digest.

Every projected record carries a source `KernelRefV2`, a domain-separated
payload fingerprint, a backend encoding, and a deterministic record id. The
coverage report compares record source refs with the complete ref set in the
compiled snapshot. Coverage is finite address coverage, not semantic
completeness.

`ProjectionAuthorityV1` has only `DerivedOnly`.
`ProjectionMutationAuthorityV1` has only `AxiographSemanticVcsOnly`.

## Finite Semantic Fragment

The projector preserves an explicit, decidable record representation of:

- schema objects;
- relation objects with ordered role projections;
- subtype and explicit schema generators;
- role kind and complete compiled type-expression structure;
- finite carriers, function mappings, and relation facts;
- constraints, categorical path equations, and rewrite rules as obligations;
- accepted snapshot and semantic references.

For dependent/indexed and refinement types, the manifest preserves the nested
`ProjectedTypeExprV1` tree. It does not flatten an indexed or refined role to
its carrier type. No supported backend is claimed to prove that tree. The
semantic-loss report therefore marks dependent indexes and refinement
predicates as transported but not enforced.

For categorical and HoTT-like semantics, the manifest preserves typed path
equation obligations, rewrite obligations, generator direction, and
reversibility flags. A backend projection does not prove functorial semantic
preservation, inverse laws, rewrite confluence, arbitrary homotopies,
higher-inductive principles, or univalence. `HigherPaths` is explicitly
`Unsupported` for every backend. The trusted checker remains the import closure
of `lean/Axiograph/VerifyMain.lean` for its documented fragment.

## Backend Capability Profiles

| Backend | Native or primary shape | Required reductions |
| --- | --- | --- |
| PathDB | AxiStore-authenticated SQLite materialization hydrated into PathDB | manifest emits a materialization plan; obligations and refinements are sidecars |
| TypeDB | native entity/relation/role schema in TypeQL | the artifact is schema-only; subtype generators, instance loading/readback, refinements, equations, constraints, rewrites, and higher paths remain manifest or adapter work |
| TerminusDB | anchored JSON-LD record bundle | backend-specific schema/named-graph/branch loading is adapter work; relation objects are reified documents and backend history is not semantic VCS authority |
| RDF/OWL | TriG dataset, OWL classes, RDF properties, reified relation resources | n-ary tuples are reified; OWL/RDF entailment is not the kernel theory |
| Property graph | portable node/edge JSON bundle | relation objects stay nodes; no binary-edge or cross-engine feature-equivalence claim |

Each `BackendCapabilityDeclarationV1` is closed over relation objects, n-ary
relations, typed roles, subtype inclusions, dependent indexes, refinements,
context/world axes, evidence/provenance, constraints, path equations, rewrites, higher paths, finite instances, and
native readback. A disposition is one of `Native`, `Encoded`,
`Sidecar`, or `Unsupported`.

## Semantic-Loss Report

`SemanticLossReportV1` is generated from actual features in the compiled
snapshot, not from a generic warning list. Its classes are:

- `RepresentationChanged`: the finite structure is retained through a non-native
  row, node, reification, property, or named-graph encoding;
- `NotEnforcedByBackend`: the obligation is present but the backend does not
  discharge it;
- `NotRepresented`: the projection has no semantics for the feature;
- `EvidenceAuthorityBoundary`: the record is readable but remains outside
  accepted authority.

The report always sets `lossless_semantic_projection_claim=false`. This avoids
turning exact record transport into a false general preservation theorem.

## Readback

A backend adapter returns `ReadbackInventoryV1` with the manifest projection id,
backend kind, required adapter provenance (`adapter_id`, `adapter_version`,
`observation_method`, and `backend_locator`), and observed `(record_id,
payload_fingerprint,native_key)` rows. `check_readback_v1` recomputes the
manifest commitment and rejects content tampering, removed manifest/inventory
versions, empty provenance, wrong projection ids, wrong backends, and duplicate
record ids, then reports:

- matched records;
- missing records;
- payload or native-key drift;
- unexpected native records.

`ExactFiniteRecordMatch` means only that the declared record set and payload
fingerprints matched. It does not imply semantic equivalence, complete answers,
ontology closure, proof replay, or backend inference soundness.

Every report embeds `ExternalEvidenceEnvelopeV1`. Its authority is
`EvidenceOnly`; `accepted_state_change=false`; and
`requires_typed_proposal_review_and_promotion=true`. Backend-native writes,
extra records, inference results, and drift therefore cannot silently enter the
accepted plane.

## Regulated-Shipment Projection Fixture

The primary workflow emits TypeDB and PathDB projections from
`examples/regulated_shipment/RegulatedShipment.axi`. Its manifest must retain
all nine relation objects, context/temporal roles, the indexed
`DispatchReview.contained_batch` type, the reviewer refinement, the certificate
path equation, the rewrite obligation, and finite facts. The generated backend
artifacts remain read-only. Their manifest coverage does not inherit the
`query_result_v4` completeness theorem, and exact readback would still be
transport evidence rather than semantic authority.

## CLI

Emit one manifest and native-readable artifact:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection emit examples/backend_projection/ProjectionDemo.axi \
  --backend rdf-owl \
  --search-root examples/backend_projection \
  --out /tmp/rdf-projection.json \
  --artifact-out /tmp/rdf-projection.trig
```

Check adapter readback:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection check-readback \
  --manifest /tmp/rdf-projection.json \
  --inventory /tmp/rdf-readback.json \
  --out /tmp/rdf-readback-report.json
```

## Validation

```bash
cargo test --manifest-path rust/Cargo.toml -p axiograph-projections
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli --test projection_cli_e2e
```

The projection crate includes positive coverage for every backend and
adversarial tests for manifest tampering, removed versions, empty provenance,
wrong anchors, wrong backend kinds, duplicate readback records, missing records,
payload/native-key drift, unexpected records, unknown manifest fields, and
false higher-path capability claims.
