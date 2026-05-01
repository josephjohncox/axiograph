# Proto/API Examples

Proto examples show how implementation artifacts can be lowered into canonical
ontology material. They are useful for ontology-driven development and coding
agents that need to reason about API surfaces.

`ProtoApiSemantics.axi` is the canonical module. Files under `large_api/` are
input fixtures for ingestion/schema-discovery workflows.

## Teaching Flow

Generate typed proposal material from the proto fixture:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  ingest proto ingest examples/proto/large_api \
  --out build/examples/proto/proto_api_proposals.json
```

Draft a review-plane `.axi` candidate:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover draft-module build/examples/proto/proto_api_proposals.json \
  --module ProtoApi_Proposals \
  --schema ProtoApi \
  --instance Observed \
  --out build/examples/proto/ProtoApi.proposals.axi
```

Then validate the candidate before any promotion:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate build/examples/proto/ProtoApi.proposals.axi
```

The JSON proposal file is `ProposalsFileV1`: evidence/review-plane input, not
accepted ontology truth.
