# Protobuf / gRPC Ingestion (Buf)

**Diataxis:** How-to  
**Audience:** users (and contributors)

This repo supports a Protobuf/gRPC evidence adapter that emits typed
`ProposalsFileV1` and `EvidenceChunkBundleV1` artifacts. It is not a semantic,
query, certificate, or accepted-plane authority until reviewed into canonical
`.axi`.

The goal is to capture:

- **Schema structure**: packages, files, messages, fields, enums, services, RPCs
- **Documentation**: doc comments become RAG chunks
- **Annotations**: proto options (including custom extensions) become explicit entities/edges
  (HTTP endpoints, auth scopes, idempotency, stability, tags, field semantics)
- **Tacit interaction hints**: low-confidence workflow groupings inferred from RPC naming

## Why binary Buf descriptor sets?

The supported descriptor input is Buf’s binary
`google.protobuf.FileDescriptorSet` (`*.binpb`). Axiograph decodes it with
`prost-reflect::DescriptorPool::decode`, which preserves custom extension
options through a maintained reflection API.

## Run on the included “large API” example

The example module is in `examples/proto/large_api/` and includes:

- multiple services (`payments`, `users`, `catalog`)
- custom RPC + field annotations (`acme.annotations.v1.*`)
- doc comments that describe typical interaction flows

Run evidence extraction (release mode recommended):

```bash
cd rust
cargo run -p axiograph-cli --release -- ingest proto ingest ../examples/proto/large_api \
  --out ../build/ingest/proto_api/proposals.json \
  --chunks ../build/ingest/proto_api/chunks.json
```

This produces evidence-plane artifacts:

- `../build/ingest/proto_api/descriptor.binpb` (binary Buf descriptor set)
- `../build/ingest/proto_api/proposals.json` (entities + relations)
- `../build/ingest/proto_api/chunks.json` (`EvidenceChunkBundleV1` doc-comment evidence for RAG)

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

If you already have a binary descriptor-set file, you can skip `buf build`:

```bash
cd rust
cargo run -p axiograph-cli --release -- ingest proto ingest /unused/root \
  --descriptor /path/to/descriptor.binpb \
  --out ../build/ingest/your_api/proposals.json
```

## End-to-end ontology engineering (Proto, over time)

For a full “evidence extraction → LLM augmentation → draft `.axi` → review gate
→ derived PathDB + viz” demo across multiple proto services and several
evolution ticks, run:

```bash
./scripts/ops/ontology_engineering_proto_evolution_ollama_demo.sh
```

Doc comment chunks remain typed evidence. They may be loaded into process-local
query state or included as an explicitly ordered, content-digested
materialization overlay so grounding can cite `DocChunk` evidence. Promotion
still requires a reviewed canonical `.axi` candidate and the normal
CQ/trust/runtime-theory gates.
