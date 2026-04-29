# Knowledge Ingestion Pipeline

**Diataxis:** How-to  
**Audience:** contributors

This document describes how Axiograph ingests knowledge from various sources,
produces evidence-plane artifacts, drafts candidate canonical `.axi`, and then
uses typed reports, semantic VCS promotion, and derived PathDB snapshots for
querying.

## Overview

The knowledge ingestion pipeline follows Axiograph's core principle:
**Rust for parsing/extraction; Lean for semantics**.

```
┌───────────────────────────────────────────────────────────────────────────┐
│                         Knowledge Sources                                   │
├───────────┬───────────┬───────────┬───────────┬───────────┬──────────────┤
│   PDFs    │ Confluence│ Transcripts│   Books   │ Conversations │ Technical  │
│           │   Wiki    │ (Meeting) │           │   (Chat)   │   Manuals   │
└─────┬─────┴─────┬─────┴─────┬─────┴─────┬─────┴─────┬─────┴──────┬───────┘
      │           │           │           │           │            │
      v           v           v           v           v            v
┌───────────────────────────────────────────────────────────────────────────┐
│                    Rust Ingestion Layer (untrusted)                        │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │ PDF      │ │Confluence│ │Meeting   │ │ Markdown │ │ Fact Extraction  │ │
│  │ Parser   │ │ Parser   │ │ Parser   │ │ Parser   │ │ (Probabilistic)  │ │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘ └────────┬─────────┘ │
│       │            │            │            │                │           │
│       v            v            v            v                v           │
│  ┌───────────────────────────────────────────────────────────────────────┐│
│  │                         DocumentExtraction                           ││
│  │    chunks[], metadata, proposals[] with confidence                   ││
│  └───────────────────────────────────────────────────────────────────────┘│
└────────────────────────────────────┬──────────────────────────────────────┘
                                     │
                                     v
                          ┌──────────────────────┐
                          │ Evidence artifacts    │
                          │  - proposals.json     │
                          │  - chunks.json (opt)  │
                          │  - facts.json (opt)   │
                          └──────────┬───────────┘
                                     │
                                     v
┌───────────────────────────────────────────────────────────────────────────┐
│                     Promotion + acceptance                                 │
│   proposals.json → candidate domain `.axi` modules (explicit review)        │
│   accepted `.axi` → runtime PathDB `.axpd` (derived, rebuildable)           │
│   typed previews/reports → semantic VCS history                              │
│   prepared queries → query_result_v3 witnesses → Lean checks                 │
└───────────────────────────────────────────────────────────────────────────┘
```

## Supported Sources

### 1. Conversations (Slack, Teams, Transcripts)

```bash
axiograph ingest conversation input.txt --out proposals.json \
  --format slack \
  --chunks chunks.json \
  --facts facts.json
```

Formats supported:
- `slack`: "Speaker (timestamp): message"
- `meeting`: "SPEAKER NAME:" followed by paragraphs

Extracts:
- Non-question turns as potential knowledge
- Technical content detection (materials, tools, parameters)
- Speaker attribution for provenance

### 2. Confluence Wiki Pages

```bash
axiograph ingest confluence page.html --out proposals.json \
  --space MACHINING \
  --chunks chunks.json \
  --facts facts.json
```

Extracts:
- Sections (h2-h6) as separate chunks
- Tables (structured data)
- Code blocks (examples, procedures)
- Labels and links

### 3. Technical Documents

```bash
axiograph ingest doc manual.txt --out proposals.json \
  --machining \
  --domain machining \
  --chunks chunks.json \
  --facts facts.json
```

The `--machining` flag enables domain-specific:
- Material mention detection
- Tool and parameter extraction
- Quality/observation tagging

### 4. Structured sources (SQL / JSON / RDF)

For structured sources we typically ingest a whole directory and emit a single
`proposals.json` contract that downstream discovery/promotion can consume.

```bash
axiograph ingest dir ./data_sources --out-dir build/ingest \
  --proposals build/proposals.json
```

Current structured adapters in `axiograph ingest dir`:

- `*.sql` → SQL DDL → table/column/foreign-key proposals
- `*.json` → JSON sample → inferred schema proposals
- `*.nt` / `*.ntriples` / `*.ttl` / `*.turtle` / `*.nq` / `*.nquads` / `*.trig` / `*.rdf` / `*.owl` / `*.xml`
  → RDF graph (Sophia) → entity/relation proposals (named graphs become `Context` entities; each relation proposal carries a `context` attribute)

Semantic Web interop design notes: `docs/explanation/SEMANTIC_WEB_INTEROP.md`.

### 5. GitHub repos (code + proto APIs)

For “codebase discovery” we can ingest a repo into:

- `chunks.json` (for RAG / approximate discovery),
- lightweight repo edges (definitions/imports/TODOs),
- and (optionally) protobuf/gRPC API structure from a Buf descriptor set.

### 6. World model proposals (JEPA / objective-driven)

World models are **untrusted** proposal generators that emit evidence-plane
`proposals.json` overlays. They plug into the same ingest/promote pipeline.

Example (baseline plugin):

```bash
axiograph discover jepa-export examples/Family.axi --out build/family_jepa.json
axiograph ingest world-model \
  --input examples/Family.axi \
  --export build/family_jepa.json \
  --out build/family_proposals.json \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py
```

Note: use full canonical `.axi` modules (schema + theory + instance + contexts)
as the training/export source. PathDB snapshots are derived execution artifacts.

Offline/local repo:

```bash
axiograph ingest github import /path/to/repo --out-dir build/github_import/demo
```

Proto APIs without requiring `buf` (use an existing binary descriptor set):

```bash
axiograph ingest github import /path/to/repo --out-dir build/github_import/demo \
  --proto-descriptor examples/proto/large_api/descriptor.binpb
```

### 7. Web pages (scrape/crawl) -> evidence artifacts

Web ingestion is discovery tooling (untrusted):

- fetch pages (rate limited; size caps),
- HTML → Markdown conversion,
- chunks + extracted facts → `proposals.json`.

Small “scrape a few pages”:

```bash
axiograph ingest web ingest --out-dir build/web_demo \
  --url https://en.wikipedia.org/wiki/General_relativity \
  --url https://www.rfc-editor.org/rfc/rfc9110 \
  --max-pages 2 --delay-ms 400 --respect-robots
```

Large-ish crawl demo (Wikipedia, link-following):

- `./scripts/web_wikipedia_crawl_demo.sh`

## Probabilistic Fact Extraction

The ingestion layer uses pattern matching to extract facts with confidence:

### Fact Types

| Type | Description | Example |
|------|-------------|---------|
| Recommendation | "use X for Y" | "use carbide for titanium" |
| Observation | "we saw X" | "we saw chatter at 3000 RPM" |
| Causation | "X causes Y" | "increasing speed causes more heat" |
| Parameter | "set X to Y" | "set feed to 0.004 IPT" |
| Comparison | "X is better than Y" | "ceramic is better than HSS for hardened steel" |
| Heuristic | Rule of thumb | "generally, lower speeds for titanium" |
| Procedure | Steps | "first rough, then finish" |

### Confidence Scoring

Base confidence is pattern-dependent, then adjusted by:

| Factor | Adjustment |
|--------|------------|
| Technical source (Confluence, manual) | +10% |
| Expert attribution | +15% |
| Short evidence (<30 chars) | -10% |
| Numerical specificity (2+ numbers) | +10% |
| Hedging language ("maybe", "possibly") | -15% |

Multiple sources for the same fact combine via:
$$P_{combined} = 1 - (1 - P_1)(1 - P_2)$$

## Lean semantics and certificates

In the Rust+Lean architecture:

- ingestion produces **untrusted evidence** (`proposals.json` + provenance)
- promotion produces candidate **canonical** `.axi` modules (explicit + reviewable)
- preview/review flows produce typed reports such as `EvolutionPreviewV1`,
  trust/coverage reports, and refinement handles
- machine query flows prepare `query_ir_v1` as `PreparedQueryV1`; supported
  certified answers emit canonical `.axi`-anchored `query_result_v3` witnesses
  (Rust emits, Lean verifies)
- accepted deltas move through semantic VCS history; PathDB remains derived

## Example: Building a Machining Knowledge Base

```bash
# 1. Ingest expert conversations
axiograph ingest conversation shop_talk.txt --out conv_proposals.json \
  --format slack --facts conv_facts.json --chunks conv_chunks.json

# 2. Ingest Confluence docs
axiograph ingest confluence cutting_params.html --out wiki_proposals.json \
  --space MACHINING --facts wiki_facts.json --chunks wiki_chunks.json

# 3. Ingest technical manuals
axiograph ingest doc machinery_handbook.txt --out manual_proposals.json \
  --machining --facts manual_facts.json --chunks manual_chunks.json

# 4. Promote untrusted proposals into candidate MachinistLearning `.axi` modules (explicit)
axiograph discover promote-proposals manual_proposals.json \
  --out-dir build/candidates \
  --domains machinist_learning \
  --min-confidence 0.70

# 5. Validate the promoted candidates (Rust parser)
axiograph check validate build/candidates/MachinistLearning.proposals.axi

# 5b. (Optional) Draft a “discovered schema” module for interactive exploration
#
# This bootstraps a canonical `.axi` schema+instance module directly from `proposals.json`,
# so you can import it into PathDB and benefit from schema-directed AxQL planning.
axiograph discover draft-module manual_proposals.json \
  --out build/Discovered.proposals.axi \
  --module Discovered_Proposals \
  --schema Discovered \
  --instance DiscoveredInstance \
  --infer-constraints

# 6. Promote reviewed candidates through the accepted plane / semantic VCS
axiograph db accept promote build/candidates/MachinistLearning.proposals.axi \
  --dir build/accepted_plane \
  --message "reviewed: machinist learning ingestion"

# 7. Build a derived query snapshot from the accepted snapshot, not the
#    candidate artifact path.
axiograph db accept pathdb-build \
  --dir build/accepted_plane \
  --snapshot head \
  --out build/machinist_learning.axpd

# 8. Run the semantics verification suite (Rust + Lean certificates/parsers)
make verify-semantics
```

## Physics Knowledge Integration

The `PhysicsKnowledge.axi` example shows how to encode:

- **Dimensional analysis**: ensure physical equations are unit-consistent
- **Physical laws**: Newton's laws, thermodynamics, cutting models
- **Material properties**: yield strength, thermal conductivity with uncertainty
- **Heuristics**: "titanium needs low speed, high feed"

These are typed in the canonical `.axi` module, enabling:
1. Constraint checking (dimensional consistency)
2. Inference (if A and B, then C)
3. Probabilistic queries (confidence-weighted)
These are represented explicitly in canonical `.axi` (schema/theory/instance), with
certificates and invariants checked in Lean.

## RAG Integration

The chunks.json output is RAG-ready:

```json
{
  "chunks": [
    {
      "chunk_id": "conv_0",
      "text": "Try reducing to 100 SFM for titanium",
      "metadata": {
        "speaker": "Sarah",
        "topic": "machining",
        "mentions_parameters": "true"
      }
    }
  ]
}
```

Use with a vector store or Axiograph's snapshot-scoped embedding sidecars to
enable semantic search over the knowledge base. Treat retrieved chunks as
**evidence** that can produce new proposals; promotion into canonical `.axi`
remains explicit. See `docs/reference/EMBEDDINGS_AND_EVIDENCE.md` for the
sidecar model and how embedding-derived relationships are lifted into typed
proposals.

Embedding artifacts should be exported as three separate objects:

- `EmbeddingsFileV1` stores the vector payload and target keys.
- `EmbeddingSidecarManifestV1` stores the accepted ref/module digest, optional
  PathDB snapshot id, model version/digest, target ids, source text digests,
  normalization policy, and trust caveats.
- `EmbeddingEvidenceOverlayV1` stores vector-free similarity observations and
  advisory candidate relationships such as `similar_to`, `supports`, `mentions`,
  `implements`, `violates`, and `subtype_candidate`.

Do not import an embedding overlay as canonical `.axi`. If a relationship from
an overlay looks useful, convert it into a typed proposal, run validation and CQ
preview, review it, reconcile conflicts, and promote the accepted delta through
the semantic VCS.

Current Rust support lives in `rust/crates/axiograph-cli/src/embeddings.rs`:

- build a manifest with `build_embedding_sidecar_manifest_v1`;
- compute a stable vector-sidecar digest with `embedding_file_digest_v1`;
- create a tiny deterministic evidence overlay with
  `discover_embedding_evidence_overlay_v1`.

That discovery helper is pairwise and intended for small sidecars or tests. It
emits ranked cosine observations plus `advisory_only=true` candidate
relationships. The CLI surface `axiograph discover embedding-relationships`
uses the same manifest and overlay report family; do not add a separate
embedding evidence format for ingestion or promotion flows.

## Derived PathDB Snapshots

For large execution-oriented knowledge bases, materialize canonical `.axi` into
PathDB snapshots (`.axpd`). These snapshots are fast query/index artifacts and
can carry reviewable evidence overlays, but accepted semantics still live in
canonical `.axi` and semantic VCS history.

Use `PreparedQueryV1` metadata/trust reports for machine query flows and
`query_result_v3` witnesses for supported certified answers.

For storage byte round-trip checks only, use the explicit DB snapshot commands:

```bash
axiograph db pathdb export-axi knowledge.axpd --out snapshot_pathdb_export_v1.axi
axiograph db pathdb import-axi snapshot_pathdb_export_v1.axi --out knowledge.axpd
```

Do not feed derived snapshots into semantic/query/certificate commands. Those
flows require canonical accepted `.axi` modules and typed anchors.
