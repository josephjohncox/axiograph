# Protobuf / gRPC Ingestion (Buf)

**Diataxis:** How-to  
**Audience:** users (and contributors)

This repo supports ingesting large Protobuf/gRPC APIs into the generic Axiograph
Evidence/Proposals schema (`proposals.json`).

The goal is to capture:

- **Schema structure**: packages, files, messages, fields, enums, services, RPCs
- **Documentation**: doc comments become RAG chunks
- **Annotations**: proto options (including custom extensions) become explicit entities/edges
  (HTTP endpoints, auth scopes, idempotency, stability, tags, field semantics)
- **Tacit interaction hints**: low-confidence workflow groupings inferred from RPC naming

## Why Buf descriptor-set *JSON*?

In the binary `google.protobuf.FileDescriptorSet` format, custom options (proto
extensions) are encoded as extension fields. In Rust, decoding those extensions
requires an extension-aware/reflective runtime.

Buf’s descriptor-set **JSON** output includes extension fields explicitly, using
keys like:

```json
{
  "[acme.annotations.v1.http]": { "get": "/v1/payments/{payment_id}" }
}
```

That makes annotation-driven ingestion practical without adding a heavy runtime
dependency.

## Run on the included “large API” example

The example module is in `examples/proto/large_api/` and includes:

- multiple services (`payments`, `users`, `catalog`)
- custom RPC + field annotations (`acme.annotations.v1.*`)
- doc comments that describe typical interaction flows

Run ingestion (release mode recommended):

```bash
cd rust
cargo run -p axiograph-cli --release -- ingest proto ingest ../examples/proto/large_api \
  --out ../build/ingest/proto_api/proposals.json \
  --chunks ../build/ingest/proto_api/chunks.json
```

This produces:

- `../build/ingest/proto_api/descriptor.json` (Buf descriptor set, JSON)
- `../build/ingest/proto_api/proposals.json` (entities + relations)
- `../build/ingest/proto_api/chunks.json` (doc comment chunks for RAG)

## What gets emitted

**High-confidence (structural) entities** (≈ 0.98):

- `ProtoPackage`, `ProtoFile`
- `ProtoMessage`, `ProtoField`
- `ProtoEnum`, `ProtoEnumValue`
- `ProtoService`, `ProtoRpc`

**Annotation-driven entities** (≈ 0.98):

- `HttpEndpoint` (derived from `(…http)` method options)
- `ProtoAuthScope`, `ProtoStability`, `ProtoTag` (derived from `(…semantics)` method options)
- `Bool`, `ProtoUnit`, `ProtoExampleValue` (derived from field-level options like `(…field)`)

**Annotation-driven relations** (≈ 0.98):

- `proto_rpc_idempotent` / `proto_rpc_auth_scope` / `proto_rpc_stability` / `proto_rpc_has_tag`
- `proto_field_required` / `proto_field_pii` / `proto_field_units` / `proto_field_example`

**Low-confidence (tacit) entities** (≈ 0.60):

- `ApiWorkflow` (groups RPCs that look like they operate on the same resource)

Heuristic relations are emitted with lower confidence (≈ 0.55–0.65) and a
human-readable rationale, so reconciliation can keep “unknown vs derived” explicit.

## Run on your own API

If your repo already has a Buf module (a directory with `buf.yaml`), you can run:

```bash
cd rust
cargo run -p axiograph-cli --release -- ingest proto ingest /path/to/your/buf/module \
  --out ../build/ingest/your_api/proposals.json \
  --chunks ../build/ingest/your_api/chunks.json
```

If you already have a descriptor-set JSON file, you can skip `buf build`:

```bash
cd rust
cargo run -p axiograph-cli --release -- ingest proto ingest /unused/root \
  --descriptor /path/to/descriptor.json \
  --out ../build/ingest/your_api/proposals.json
```

## End-to-end ontology engineering (Proto, over time)

For a full “ingest → LLM augmentation → draft `.axi` → promotion gate → PathDB + viz”
demo across multiple proto services and several evolution ticks, run:

```bash
./scripts/ontology_engineering_proto_evolution_ollama_demo.sh
```

This demo also imports doc comment chunks into the produced `.axpd` snapshots
as `DocChunk` nodes (`axiograph db pathdb import-chunks ...`), enabling `fts(...)`
queries and LLM grounding over real doc text, plus semantic metadata (FQNs,
kinds, message/field names, etc) via `DocChunk.search_text`.
