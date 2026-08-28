# Schema Discovery Examples

Schema-discovery examples show proposal material becoming canonical `.axi`
candidates. They are evidence/review-plane examples until validation and
promotion accept them.

- `ProtoApi.proposals.axi` is a proto/API proposal module.
- `SqlSchema.proposals.axi` is a SQL-schema proposal module.
- `inputs/*.json` are `ProposalsFileV1` proposal inputs for discovery
  workflows. They are checked-in tool inputs, not the preferred human
  authoring surface.

## Proto/API Review Flow

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
	  ingest proto ingest examples/proto/large_api \
	  --out build/examples/schema_discovery/proto_api_proposals.json

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
	  discover draft-module build/examples/schema_discovery/proto_api_proposals.json \
  --module ProtoApi_Proposals \
  --schema ProtoApi \
  --instance Observed \
  --out build/examples/schema_discovery/ProtoApi.proposals.axi

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate build/examples/schema_discovery/ProtoApi.proposals.axi
```

For checked-in tool inputs, use
`examples/repl_scripts/proto_schema_discovery_axi_demo.repl` as a review-plane
candidate inspection script. Promotion remains a separate semantic review step.
