# Backend Projection Example

`ProjectionDemo.axi` is the authoring input. The accepted `.axi` import closure
and its immutable `KernelSnapshotIr` are the meaning plane. Every file emitted
from it is a read-only derived projection.

Generate a typed manifest and native-readable artifact:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection emit examples/backend_projection/ProjectionDemo.axi \
  --backend typedb \
  --search-root examples/backend_projection \
  --out /tmp/axiograph-typedb-projection.json \
  --artifact-out /tmp/axiograph-typedb-projection.tql
```

The accepted backend names are `pathdb`, `typedb`, `terminusdb`, `rdf-owl`, and
`property-graph`. Each manifest contains:

- the repository, accepted-snapshot, and compiled-kernel-IR anchors;
- one closed capability declaration for relation objects, n-ary relations,
  typed roles, subtype arrows, dependent indexes, refinements, context/world
  axes, evidence/provenance, constraints, path equations, rewrites, higher
  paths, finite instances, and native readback;
- typed projection records with source `KernelRefV2`, payload fingerprints, and
  backend encodings;
- a backend-readable artifact;
- exact finite coverage and semantic-loss reports;
- explicit non-claims and Axiograph-only mutation authority.

PathDB output is a materialization **plan**, not a bare `.axpd` database.
AxiStore remains the only publisher of authenticated SQLite `.axpd`
materializations. TypeDB emits TypeQL schema text. TerminusDB emits JSON-LD.
RDF/OWL emits TriG with reified relation objects. The portable property-graph
bundle keeps relation objects as nodes rather than pretending every relation is
a lossless binary edge.

Backend adapters produce an
`axiograph_projection_readback_inventory_v1` containing the manifest
`projection_id`, backend name, required adapter provenance
(`adapter_id`, `adapter_version`, `observation_method`, `backend_locator`), and
observed `(record_id,payload_fingerprint,native_key)` triples. Check it with:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  tools projection check-readback \
  --manifest /tmp/axiograph-typedb-projection.json \
  --inventory /tmp/typedb-readback.json \
  --out /tmp/typedb-readback-report.json
```

Even `exact_finite_record_match` means transport equality only. Readback is
wrapped as `evidence_only`, cannot change accepted state, and must pass typed
proposal, review, competency-question/trust gates, reconciliation, and
promotion before any new domain claim enters accepted `.axi`.

The executable contracts and adversarial cases live in:

- `rust/crates/axiograph-projections/tests/projection_contract.rs`
- `rust/crates/axiograph-cli/tests/projection_cli_e2e.rs`
