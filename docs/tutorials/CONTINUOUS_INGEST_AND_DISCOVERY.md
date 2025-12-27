# Continuous Ingest and Continuous Discovery (Prototype)

**Diataxis:** Tutorial  
**Audience:** contributors

This document describes the *operational* pipelines that connect real-world sources (code, Confluence, SQL, docs)
to Axiograph’s two core promises:

1. **Fast discovery** (approximate is allowed, but must be labeled and auditable).
2. **Certified answers** (high-value results come with Lean-checkable certificates).

The guiding rule is unchanged in production:

> Untrusted engines compute; a small trusted checker verifies.

In Axiograph v6:

- Rust runs ingestion, indexing, search, and optimization (untrusted).
- Lean defines semantics and checks certificates (trusted, mathlib-backed).
- `.axi` remains the **canonical** human-facing source of truth.

---

## 1. Two planes: evidence vs accepted knowledge

To keep the system usable *and* sound, we maintain two explicit planes:

### 1.1 Evidence plane (approximate)

Artifacts:

- `chunks.json`: document/code chunks with metadata (`path`, `language`, `span`, etc.)
- `proposals.json`: extracted **structured KG proposals** (entities/relations/claims) with confidence + evidence pointers
- `facts.json`: optional “raw extractor output” (pattern- or LLM-derived) retained for debugging and incremental development
- optional: vector index / embeddings (not a truth source)

This plane powers:

- retrieval (keyword / embeddings),
- summarization,
- hypothesis generation (“possible links”, “candidate schema edges”),
- triage (“what changed?”, “what’s new?”).

### 1.2 Accepted knowledge plane (canonical + certifiable)

Artifacts:

- `.axi` modules representing accepted schema + instances, committed in Git and/or an append-only log
- snapshots (commit index / content hash) used to scope certificates
- PathDB indexes derived from snapshots (rebuildable)

This plane powers:

- certificate-carrying queries,
- normalization/rewrite semantics,
- reconciliation decisions,
- migration operators (Δ_F / Σ_F) with checkable proofs.

---

## 2. Ingestion pipeline (continuous)

Ingestion is best modeled as a loop that continuously turns “stuff” into:

- evidence chunks, and
- structured proposals.

### 2.1 Sources

Common sources we support (or aim to support) as first-class ingesters:

- **Repo/codebase** (Rust, Lean, Markdown, etc.)
- **Confluence** (HTML export)
- **SQL** (DDL/schema)
- **RDF/OWL** (`.nt`/`.ttl`/`.nq`/`.trig`/`.rdf`/`.owl`/`.xml`; see `docs/explanation/SEMANTIC_WEB_INTEROP.md`)
- **CAD** (STEP/IGES)
- PDFs, transcripts, reading lists

### 2.2 Output contract: proposal facts with provenance

All extractors (regex heuristics, parsers, LLMs) should emit the same shape:

- structured entity/relation/claim proposals,
- a confidence score,
- evidence pointers into chunks and/or external citations,
- and a source identity (which extractor produced it).

This makes downstream reconciliation consistent and auditable.

---

## 3. Discovery pipeline (continuous)

Discovery consumes both planes:

### 3.1 Approximate discovery (fast, labeled)

Inputs:

- `chunks.json` + (optionally) embeddings
- `proposals.json` structured proposals

Outputs:

- ranked evidence bundles (“these chunks likely answer X”)
- proposed links/facts (“this symbol probably implements that concept”)
- summaries, alerts, and suggested queries

These outputs are *not* “truth”; they are candidates.

### 3.2 Certified discovery (sound, checkable)

Inputs:

- accepted `.axi` snapshots (canonical)
- derived PathDB indexes (rebuildable)

Outputs:

- query results plus certificates
- reconciliation/normalization/migration certificates where applicable

Verification:

- Lean replays or recomputes the semantics and rejects invalid certificates.

---

## 4. “Chain of Thought discovery” as discovery traces (policy)

We implement “Chain of Thought discovery” as **structured discovery traces**:

- evidence pointers (chunk ids, file paths, citations),
- a short *public* rationale (what evidence supports the proposal),
- optional certificates for any certified results,
- explicit labeling of approximate vs certified steps.

We do **not** store raw model hidden reasoning. It is not stable, not auditable, and not intended to be logged.

Example trace skeleton:

```json
{
  "trace_id": "…",
  "query": "Where is certificate checking implemented?",
  "generated_at": "…",
  "proposals": [
    {
      "kind": "relation",
      "rel_type": "Mentions",
      "from": "rust/crates/axiograph-pathdb/src/certificate.rs",
      "to": "lean/Axiograph/Certificate/Check.lean",
      "confidence": 0.76,
      "evidence": [
        { "chunk_id": "repo_…", "path": "docs/reference/CERTIFICATES.md", "span_id": "section_…" }
      ],
      "public_rationale": "Both files describe the Rust→Lean certificate boundary."
    }
  ],
  "certificate": null
}
```

---

## 5. Prototype CLI workflow (today)

The goal of the prototype tooling is to make this loop tangible without requiring a full distributed deployment.

### 5.1 Index a repo (evidence plane)

- Scan a directory, chunk files, extract lightweight facts, emit:
  - `chunks.json`,
  - `edges.json` (lightweight repo graph edges),
  - `proposals.json` (structured KG proposals with evidence pointers; generic Evidence/Proposals schema),
  - `traces.json` (optional).

Example:

```bash
cd rust
cargo run -p axiograph-cli -- ingest repo index ../axiograph_v6 \
  --out ../build/repo_proposals.json \
  --chunks ../build/repo_chunks.json \
  --edges ../build/repo_edges.json
```

To ingest a directory of heterogeneous sources into evidence artifacts (and optionally aggregate
chunks/facts/proposals), use:

```bash
cd rust
cargo run -p axiograph-cli -- ingest dir ../docs \
  --out-dir ../build/ingest_docs \
  --chunks ../build/ingest_chunks.json \
  --facts ../build/ingest_facts.json \
  --proposals ../build/ingest_proposals.json
```

### 5.2 Continuous ingest (polling)

- Periodically rescan for changed files and refresh:
  - chunks and structured proposals,
  - optional discovery trace outputs.

Example:

```bash
cd rust
cargo run -p axiograph-cli -- ingest repo watch ../axiograph_v6 \
  --out ../build/repo_proposals.json \
  --chunks ../build/repo_chunks.json \
  --edges ../build/repo_edges.json \
  --trace ../build/repo_discovery_trace.json \
  --interval-secs 60
```

### 5.3 Continuous discovery (polling)

- Periodically run discovery tasks on the evidence plane, for example:
  - “new symbols since last scan”
  - “new TODOs / design notes”
  - “suggest links from docs to code”

### 5.4 Hands-on demo: continuous ingest as mutations + viz

For a small, local “feel the loop” demo (no external services, no LLMs), use the
REPL script:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/continuous_ingest_demo.repl
```

It starts from a base `enterprise` scenario, then applies two ingest ticks:

- new `Doc` entities arrive
- low-confidence evidence edges are added (`mentionsService`, `suggestsSameColumn`, …)
- explicit witness objects are created (`PathWitness`, `Homotopy`)
- after each tick it writes neighborhood visualizations:
  - `build/continuous_ingest_round0.{dot,html}`
  - `build/continuous_ingest_round1.{dot,html}`
  - `build/continuous_ingest_round2.{dot,html}`

This is a prototype for the production loop: ingestion mutates the evidence plane,
while promotion into canonical `.axi` (and certificate checking) remains explicit.

### 5.5 Hands-on demos: continuous ingest from SQL / Proto (CLI-only)

If you want to keep everything in the “CLI command” surface (no interactive REPL),
use the demo scripts:

- SQL: `scripts/continuous_ingest_sql_cli_demo.sh`
- Proto: `scripts/continuous_ingest_proto_cli_demo.sh` (requires `buf`)

Both demos run two “ticks”:

1) ingest structured sources into `proposals.json` (evidence plane)
2) draft a readable candidate `.axi` module (schema discovery)
3) import to a PathDB snapshot (`.axpd`)
4) render HTML visualizations (meta/data neighborhood views)

### 5.6 Higher-level discovery loop: augment → promote

Once you have a `proposals.json`, you can run a deterministic augmentation pass
that derives additional structure (and optionally uses an LLM for semantic
labeling / routing):

```bash
cd rust
cargo run -p axiograph-cli -- discover augment-proposals ../build/repo_proposals.json \
  --out ../build/repo_proposals.aug.json \
  --trace ../build/repo_proposals.aug.trace.json
```

To use a local Ollama model (untrusted) to suggest `schema_hint` updates:

```bash
cd rust
cargo run -p axiograph-cli -- discover augment-proposals ../build/repo_proposals.json \
  --out ../build/repo_proposals.aug.json \
  --trace ../build/repo_proposals.aug.trace.json \
  --chunks ../build/repo_chunks.json \
  --llm-ollama \
  --llm-model nemotron-3-nano
```

If you also want the LLM to propose **new** untrusted entities/relations (grounded
in evidence chunks), add:

```bash
  --llm-add-proposals --chunks ../build/repo_chunks.json
```

Then promote the (possibly augmented) proposals into candidate `.axi` patches:

```bash
cd rust
cargo run -p axiograph-cli -- discover promote-proposals ../build/repo_proposals.aug.json \
  --out-dir ../build/candidates \
  --min-confidence 0.5
```

The candidate modules are **not** canonical; promotion into `examples/**.axi` is
explicit and reviewable.

Then promote selected outputs through reconciliation into accepted `.axi`.

### 5.7 Schema discovery loop: proposals → draft `.axi` module

Some sources are already structured (SQL DDL, proto descriptors, JSON) and are
best explored with a **schema-aware** query layer. For that, we can draft a
canonical `axi_v1` module from `proposals.json`:

```bash
cd rust
cargo run -p axiograph-cli -- discover draft-module ../build/ingest_proposals.json \
  --out ../build/discovered.proposals.axi \
  --module Discovered_Proposals \
  --schema Discovered \
  --instance DiscoveredInstance \
  --infer-constraints
```

This produces a **candidate** `.axi` module you can:

- import into PathDB (REPL/CLI) to get the `.axi` meta-plane, and then
- benefit from AxQL schema-directed planning (implied type constraints, keys as pruning hints).

The optional `--infer-constraints` flag is an *extensionality experiment*:
it infers key/functional constraints from the **observed extension** (current tuples).
Treat these as hypotheses until reviewed and promoted into the accepted `.axi` plane.

LLM-assisted variant: run `discover augment-proposals --llm-plugin ...` first, then
draft the module from the augmented output.

Example (one-shot discovery run):

```bash
cd rust
cargo run -p axiograph-cli -- discover suggest-links \
  ../build/repo_chunks.json \
  ../build/repo_edges.json \
  --out ../build/repo_discovery_trace.json
```

### 5.8 Promote proposals into candidate domain modules (explicit)

After ingestion has produced `proposals.json`, convert those untrusted proposals into **reviewable**
candidate `.axi` domain modules.

This step performs:

- basic **entity resolution** (merge duplicates; record conflicts),
- **schema mapping** into the canonical example domains:
  - `EconomicFlows` (economics),
  - `MachinistLearning` (machining/learning),
  - `SchemaEvolution` (ontology/migrations),
- and emits a `promotion_trace.json` so promotion stays explicit.

Example:

```bash
cd rust
cargo run -p axiograph-cli -- discover promote-proposals \
  ../build/ingest_proposals.json \
  --out-dir ../build/candidates \
  --min-confidence 0.70 \
  --domains machinist_learning
```

Outputs:

- `build/candidates/MachinistLearning.proposals.axi` (candidate blocks to merge)
- `build/candidates/promotion_trace.json` (what mapped vs what was skipped)

Promotion remains manual: review the candidate `.axi` output and merge accepted blocks into the
canonical modules under `examples/` (or your project’s canonical `.axi` tree).

---

## 6. Next steps (toward production)

1. Treat accepted `.axi` facts as an append-only log + snapshots (even on a single machine).
2. Bind certificates to snapshot ids (and later to cryptographic commitments for offline verification).
3. Expand certificate coverage beyond reachability into:
   - groupoid/rewrite derivations (normalization),
   - reconciliation derivations,
   - and Δ_F / Σ_F migrations.
4. Make LLM extraction pluggable but *untrusted by default*:
   - outputs are proposal facts with evidence pointers,
   - promoted only by explicit reconciliation policy.
