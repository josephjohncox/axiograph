# Schema Discovery (Automated Ontology Engineering)

**Diataxis:** Tutorial  
**Audience:** users (and contributors)

This document describes Axiograph’s **schema discovery** loop: turning untrusted
evidence-plane artifacts (`proposals.json`) into a **candidate**, readable
canonical `.axi` module that you can iterate on interactively, validate with
typed reports, and promote through semantic VCS review.

The guiding architecture remains:

- Rust computes candidates (fast, heuristic, untrusted).
- Lean checks certificates for anything promoted into “certified” answers.
- `.axi` is the human-reviewable canonical source plane.
- PathDB is the derived execution/query substrate, not the semantic authority.
- Promotion emits typed previews/reports and advances semantic VCS history.

## Why schema discovery exists

AxQL and PathDB become much more useful when a derived PathDB snapshot contains
the `.axi` **meta-plane** (schema + theory metadata):

- AxQL planning can auto-add **implied type constraints** from relation field types.
- Keys/functionals can be used as **join planning hints** and (for fact atoms) candidate pruning.
- Fact atoms benefit from PathDB’s **FactIndex** for fast `axi_relation` filtering.
- Machine/report flows can prepare `query_ir_v1` as `PreparedQueryV1` and carry
  typed metadata, trust contracts, and refinement handles.

But many ingestion sources start in the evidence plane:

- SQL DDL
- proto descriptor sets
- JSON payloads
- repo/code analysis + extracted mentions

They produce `proposals.json` first, because that’s our generic, reviewable evidence format.

Schema discovery is the bridge that drafts a canonical `.axi` module so you can:

1) import it into PathDB,
2) query it with schema-directed AxQL or `PreparedQueryV1`-backed tooling,
3) inspect typed reports and apply refinement handles,
4) iterate/refine it (possibly with LLM assistance),
5) promote reviewed changes into your accepted `.axi` modules through semantic VCS.

## CLI: draft a module from proposals

Command:

```bash
cd rust
cargo run -p axiograph-cli -- discover draft-module <proposals.json> --out <module.axi>
```

Key flags:

- `--module <Name>`: module name
- `--schema <Name>`: schema name
- `--instance <Name>`: instance name
- `--infer-constraints`: infer **extensional** keys/functionals from current tuples

Example:

```bash
cd rust
cargo run -p axiograph-cli -- discover draft-module ../build/ingest_proposals.json \
  --out ../build/Discovered.proposals.axi \
  --module Discovered_Proposals \
  --schema Discovered \
  --instance DiscoveredInstance \
  --infer-constraints
```

## Extensionality (important caveat)

`--infer-constraints` is a deliberate **extensionality experiment**:

- It infers constraints from the *observed extension* (the data you currently have).
- These constraints are hypotheses. They can be invalidated by new data later.

Use cases:

- interactive exploration (faster fact lookup / better planning)
- “what seems functional/key-like in practice?”
- suggestions for what to make explicit in your accepted schema

Non-goals:

- declaring “truth about the world” from a small sample
- replacing intentional modeling decisions

## How to use the drafted module in the REPL

Once you have a drafted `.axi`:

```bash
cd rust
cargo run -p axiograph-cli -- repl
```

Then:

```text
axiograph> import_axi build/Discovered.proposals.axi
axiograph> schema Discovered
axiograph> constraints Discovered
axiograph> validate_axi
axiograph> viz build/Discovered_schema.dot format dot plane meta focus_name Discovered hops 3 max_nodes 240
```

You now have a meta-plane that the AxQL planner can use for:

- implied type constraints,
- key/functional hints (and best-effort key pruning),
- faster fact-atom selection via FactIndex.

## LLM-assisted loop (untrusted)

LLMs can help with **semantic discovery** and **structural discovery**, but they remain untrusted.
The intended pattern is:

1) ingest → `proposals.json`
2) `discover augment-proposals` (semantic labeling / suggestions)
3) `discover draft-module` (draft a candidate `.axi` module, optionally with extra structure suggestions)
4) review + reconcile + promote into accepted `.axi`

The promotion boundary is explicit so we do not conflate:

- “certificate-checked derivability from inputs” with
- “truth of the inputs”.

### Semantic discovery: route proposals into canonical domains (optional)

The discovery pipeline can ask an LLM to suggest `schema_hint` updates that route
proposals into one of the canonical example domains (still untrusted):

```bash
cd rust
cargo run -p axiograph-cli -- discover augment-proposals ../build/repo_proposals.json \
  --out ../build/repo_proposals.aug.json \
  --trace ../build/repo_proposals.aug.trace.json \
  --chunks ../build/repo_chunks.json \
  --llm-ollama \
  --llm-model nemotron-3-nano
```

Timeouts: local models sometimes take a long time on big prompts. You can tune the
request timeout either per-command or via env var:

- `--llm-timeout-secs 600` (0 disables; wait forever)
- `AXIOGRAPH_LLM_TIMEOUT_SECS=600` (applies to all LLM calls, including REPL `llm ...`)

By default this only fills missing per-proposal hints. If you want to allow the
LLM to overwrite existing hints:

```bash
  --overwrite-schema-hints
```

### LLM grounded expansion: add proposals (optional)

If you want the LLM to propose **new** untrusted entities/relations (still in the
evidence plane), add:

```bash
  --llm-add-proposals --chunks <chunks.json>
```

This is useful for “grounded augmentation”: turning free text into a richer
proposal graph, with new proposals carrying explicit `evidence.chunk_id` pointers.

### Structural discovery: suggest extra schema structure (optional)

When drafting a candidate module, you can ask an LLM to suggest:

- additional subtype edges between discovered object types, and
- candidate relation constraints (`symmetric`, `transitive`).

```bash
cd rust
cargo run -p axiograph-cli -- discover draft-module ../build/repo_proposals.aug.json \
  --out ../build/Discovered.proposals.axi \
  --module Discovered_Proposals \
  --schema Discovered \
  --instance DiscoveredInstance \
  --infer-constraints \
  --llm-ollama \
  --llm-model nemotron-3-nano
```

These suggestions are inserted into the draft module as clearly marked,
reviewable additions (a separate `theory ...Suggested` block and an “LLM-suggested subtypes”
section).

## Promotion gate (candidate → accepted)

Candidate modules are **not** accepted knowledge. Before you “promote” a draft
into your accepted `.axi` plane, run a small gate:

1) **Rust well-formedness gate**: parse + typecheck the module (fast).
2) **Lean certificate gate**: have Rust emit an `axi_well_typed_v1` certificate
   and have Lean re-parse + re-check (trusted checker).

Example:

```bash
cd rust

# 1) Rust gate: parse + typecheck
cargo run -p axiograph-cli -- check validate build/Discovered.proposals.axi

# 2) Emit a typecheck certificate (anchored to the module digest)
cargo run -p axiograph-cli -- cert typecheck build/Discovered.proposals.axi \
  --out build/Discovered.typecheck_cert.json

# 3) Lean gate: verify the certificate against the anchored input
cd ..
make verify-lean-cert AXI=build/Discovered.proposals.axi CERT=build/Discovered.typecheck_cert.json
```

If the gate succeeds, you can promote the module into your **accepted plane**.
This creates an append-only audit log, a content-derived snapshot id, a typed
promotion preview (`EvolutionPreviewV1`), and semantic VCS history for the
accepted delta.

```bash
cd rust

# Promote a reviewed module into the accepted plane (append-only).
# Prints the new snapshot id to stdout.
snapshot_id="$(cargo run -p axiograph-cli -- db accept promote ../build/Discovered.proposals.axi \
  --dir ../build/accepted_plane \
  --message \"reviewed: initial discovered schema\")"

echo "accepted snapshot: $snapshot_id"
```

Then build derived artifacts (PathDB snapshot + viz). These are execution and
inspection surfaces; the accepted `.axi` plus semantic VCS ref remains the
meaning plane.

```bash
# Rebuild a `.axpd` snapshot from the accepted-plane snapshot id.
cargo run -p axiograph-cli -- db accept build-pathdb \
  --dir ../build/accepted_plane \
  --snapshot "$snapshot_id" \
  --out ../build/Discovered.accepted.axpd

# Optional: commit doc/code chunks as an extension-layer overlay (append-only PathDB WAL).
# This enables `fts(...)` / evidence navigation in the REPL without changing the canonical `.axi`.
#
# Note: this overlay is *not* part of the certified core unless you explicitly promote it
# into canonical `.axi` and re-run the acceptance gate.
cargo run -p axiograph-cli -- db accept pathdb-commit \
  --dir ../build/accepted_plane \
  --accepted-snapshot "$snapshot_id" \
  --chunks ../build/ingest_chunks.json \
  --message "discovery overlay: import chunks"

cargo run -p axiograph-cli -- db accept pathdb-build \
  --dir ../build/accepted_plane \
  --snapshot latest \
  --out ../build/Discovered.accepted_with_chunks.axpd

# Meta-plane visualization (schema/theory)
cargo run -p axiograph-cli -- tools viz ../build/Discovered.accepted.axpd \
  --out ../build/Discovered.meta.html \
  --format html --plane meta --focus-name Discovered --hops 3

# Data-plane visualization (instances)
cargo run -p axiograph-cli -- tools viz ../build/Discovered.accepted.axpd \
  --out ../build/Discovered.data.html \
  --format html --plane data --hops 2
```

For query-facing tooling, the canonical contract is: compile to `query_ir_v1`,
prepare `PreparedQueryV1`, inspect the typed metadata/report envelope, and use
`.axi`-anchored `query_result_v3` witnesses when a certified result is required.
Raw AxQL in the REPL is the human-facing teaching/debug surface over the same
typed planning boundary.

## Debug parity only: `PathDBExportV1`

If you are testing live-byte storage or parser parity, export the derived
snapshot through `PathDBExportV1` explicitly:

```bash
cargo run -p axiograph-cli -- db pathdb export-axi ../build/Discovered.accepted.axpd \
  --out ../build/Discovered.snapshot_pathdb_export_v1.axi
```

Do not teach this as the promotion, query, certificate, or semantic interchange
path. It is a reversible snapshot/debug format only.

## Included demo assets

- Example proposals: `examples/schema_discovery/sql_schema_proposals.json`
- Drafted module: `examples/schema_discovery/SqlSchema.proposals.axi`
- REPL script: `examples/repl_scripts/sql_schema_discovery_axi_demo.repl`
- Example proposals (proto toy): `examples/schema_discovery/proto_api_proposals.json`
- Drafted module (proto toy): `examples/schema_discovery/ProtoApi.proposals.axi`
- REPL script (proto toy): `examples/repl_scripts/proto_schema_discovery_axi_demo.repl`
- Shell demo: `scripts/schema_discovery_sql_demo.sh`
- Shell demo (LLM semantic + structural discovery via Ollama): `scripts/ontology_engineering_ollama_discovery_demo.sh`
- Shell demo (Proto evolution over time, LLM augmentation via Ollama): `scripts/ontology_engineering_proto_evolution_ollama_demo.sh`
- Shell demo (Physics, LLM grounded expansion via Ollama): `scripts/physics_discovery_ollama_grounded_demo.sh`
