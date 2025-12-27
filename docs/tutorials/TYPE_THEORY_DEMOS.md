# Type Theory Demos: Paths, Homotopies, Queries, and Certificates

**Diataxis:** Tutorial  
**Audience:** users (and contributors)

This doc is a hands-on set of “why this is interesting” demos for Axiograph’s
type-theoretic design:

- **Proof-irrelevant mode**: explore fast (no certificates).
- **Proof-relevant mode**: audit results (Rust emits a certificate; Lean checks it).
- **Approximate + tacit knowledge**: represent heuristics explicitly (low confidence + provenance),
  while keeping promotion into canonical `.axi` **reviewable**.

If you want background on certificate formats, see `docs/reference/CERTIFICATES.md`.
If you want the query language reference, see `docs/reference/QUERY_LANG.md`.

## 0) Mental model (why “type theory” shows up)

At runtime, PathDB is “just a graph”, but the design treats:

- **entities** as *points*,
- **relations** as *generating arrows*,
- **paths** as *composites*,
- and **homotopies** as *explicit witnesses that two derivations are equivalent*.

This is the core HoTT/groupoid intuition: “paths between paths” are first-class.

In the migration plan, Rust is the **untrusted engine** and Lean is the **trusted checker**:

- Rust is allowed to be clever (indexes, heuristics, optimizations).
- Rust must emit a **certificate** (a witness).
- Lean checks the certificate against the formal semantics.

That’s what “proof-relevant” means here: the system can tell you *why* it believes something,
not just *that* it does.

## 1) Proof-irrelevant exploration (REPL)

The quickest way to see the “paths + homotopies” structure is to use scenario generators.

Run a scenario script:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/enterprise_demo.repl
```

Or import a canonical module that contains explicit schema morphisms / equivalences
(and visualize the resulting `Morphism` / `Homotopy` witness nodes):

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/schema_evolution_axi_demo.repl
```

Try the proof-relevant-shaped queries inside the script (also runnable manually):

```text
# Two ways to derive the same endpoint:
q select ?svc where name("doc_0_0") -mentionsService-> ?svc limit 10
q select ?svc where name("doc_0_0") -mentionsEndpoint/belongsTo-> ?svc max_hops 4 limit 10

# A Homotopy object ties those derivations together:
q select ?h ?lhs ?rhs where
  ?h is Homotopy,
  ?h -from-> name("doc_0_0"),
  ?h -lhs-> ?lhs,
  ?h -rhs-> ?rhs
limit 10
```

In this mode you get answers fast; you *don’t* get a machine-checkable witness.

## 2) Proof-relevant auditing (certificate-backed queries)

The same query can be run in **certified mode**:

1. Use a scenario script to generate a dataset and export a reversible snapshot:
   - the scripts already do `export_axi build/<scenario>_export_v1.axi`.
2. Emit a query certificate anchored to that snapshot:
3. Verify the certificate in Lean.

### 2.1 Generate + export a snapshot (Rust)

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/proto_api_demo.repl
```

This writes a snapshot export like:

- `rust/build/proto_api_export_v1.axi`

### 2.2 Emit a `query_result_v1` certificate (Rust)

From `axiograph_v6/`:

```bash
cd rust
cargo run -p axiograph-cli -- cert query build/proto_api_export_v1.axi --lang axql \
  'select ?rpc where name("doc_proto_api_0") -mentions_http_endpoint/proto_http_endpoint_of_rpc-> ?rpc max_hops 3 limit 10' \
  > build/proto_api_query_cert.json
```

This certificate is **proof-relevant**:

- every returned row includes **path witnesses**,
- each witness is a chain of snapshot-scoped `relation_id` facts,
- confidences are fixed-point (`*_fp`) so Lean can check them without floats.

### 2.3 Verify in Lean (trusted checker)

```bash
make verify-lean-cert AXI=rust/build/proto_api_export_v1.axi CERT=rust/build/proto_api_query_cert.json
```

Or run the repo’s anchored query e2e target:

```bash
make verify-lean-e2e-query-result-v1
```

## 3) Proof relevance vs proof irrelevance (practical take)

In type theory, *proof irrelevance* roughly means “the program doesn’t care which proof you have”.

In Axiograph:

- **proof-irrelevant execution** is for iteration and performance (`repl`, `q`, `sql`, `ask`).
- **proof-relevant execution** is for auditability (`axiograph cert query`, normalization certs, reconciliation certs, …).

The system is designed so you can:

- explore quickly without certificates,
- then re-run the important query in certified mode,
- and keep the certificate as an artifact you can check later (or by a third party).

## 4) Tacit + approximate knowledge (explicit, not hidden)

Many “real” knowledge flows are not crisp facts:

- heuristics,
- inferred entity-resolution links,
- suggested workflow steps,
- “this probably implies that” edges.

We represent these as explicit edges with:

- **lower confidence**,
- and (via ingestion) **evidence pointers**.

### 4.1 Tacit/approx in PathDB scenarios (fast demo)

The `proto_api` scenario demonstrates this:

- `workflow_suggests_order` (heuristic)
- `observed_next` (another signal)
- `Homotopy` between the two “order derivations”

Run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/proto_api_demo.repl
```

Then inspect:

```text
q select ?next where name("acme.svc0.v1.Service0.CreateWidget") -workflow_suggests_order-> ?next limit 10
q select ?next where name("acme.svc0.v1.Service0.CreateWidget") -observed_next-> ?next limit 10
q select ?lhs ?rhs where name("homotopy_CreateWidget_to_GetWidget_0") -lhs-> ?lhs, name("homotopy_CreateWidget_to_GetWidget_0") -rhs-> ?rhs limit 10
```

Key point: a certificate proves **derivability from inputs**, not truth of inputs.
Tacit/approx edges should remain explicit, reviewable, and confidence-scoped.

### 4.1b Physics tacit knowledge + learning graph (canonical `.axi`)

The physics/machining knowledge base also includes a small **learning graph**
overlay (Concept prerequisites + guideline/explanation links + examples).

Run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/physics_learning_demo.repl
```

Then explore:

```text
learning_graph Physics
q select ?g where name("RegenerativeChatter") -explains-> ?g limit 10
q select ?c where name("Example_Ti_Roughing_TooFast") -demonstrates-> ?c limit 10
q select ?conf ?why where name("TitaniumLowSpeed") -HeuristicConfidence-> ?conf, name("TitaniumLowSpeed") -HeuristicRationale-> ?why limit 10
```

### 4.1b2 Mathematical physics ontology (diff geom / symplectic / relativity / QFT)

`examples/physics/PhysicsOntology.axi` is a larger canonical module that models:

- differential geometry (manifolds, metrics, connections, forms)
- symplectic/Hamiltonian mechanics
- special/general relativity hooks
- Clifford algebras (gamma matrices)
- QFT hooks (QED as a gauge theory)
- plus an explicit learning graph (Concept prerequisites, guidelines, examples)

Run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/physics_ontology_axi_demo.repl
```

### 4.1c Modalities (epistemic + deontic) + explicit evidence (canonical `.axi`)

This demo is a compact “modal knowledge” example:

- **epistemic**: worlds + accessibility + propositions-at-worlds
- **deontic**: ideal worlds + obligations-at-worlds
- **tacit evidence**: evidence pointers + confidence
- **2-morphisms**: alternative justifications related by `JustificationEquiv`

Run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/modalities_axi_demo.repl
```

Try:

```text
q select ?w where name("W0") -Accessible-> ?w limit 10
q select ?p where name("W1") -Holds-> ?p limit 10
q select ?p where name("Alice") -Knows-> ?p limit 10
q select ?obl where name("W0") -Obligatory-> ?obl limit 10
q select ?p2 where ?j = JustificationEquiv(path1=Path_Policy, path2=?p2, witness=?w) limit 10
```

### 4.1d Modalities + dependent-type-like witnesses (realistic ops demo)

This demo is a more “operational” version of the above: a tiny supply-chain plan
with:

- explicit **world/context indexing** (`Plan` vs `Observed` vs `Policy`),
- **context-scoped tuples** via `@context Context` (so “missing” is *unknown*, not *false*),
- **2-cells** via `RouteEquivalence(..., proof=...)` (path between paths),
- **proof terms** for obligations (`JustificationPath` objects),
- and a small “knowledge generation” slice by adding `DocChunk(text=...)` nodes in the REPL and exploring them with `fts(...)`.

Run:

```bash
./scripts/supply_chain_modalities_hott_demo.sh
```

Then open:

- `build/supply_chain_modalities_hott_demo/viz_rawmetal_a.html`
- `build/supply_chain_modalities_hott_demo/viz_erp_event_0.html`

If you prefer a pure REPL script (no wrapper), run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/supply_chain_modalities_hott_demo.repl
```

### 4.2 Tacit knowledge via ingestion + explicit promotion (reviewable `.axi`)

This is the “GraphRAG → Axiograph” flow:

1) **Ingest** untrusted text into `proposals.json` (Evidence/Proposals schema):

```bash
cd rust
mkdir -p build/demo
cargo run -p axiograph-cli -- ingest doc ../examples/docs/sample_conversation.txt \
  --out build/demo/proposals.json \
  --machining \
  --chunks build/demo/chunks.json \
  --facts build/demo/facts.json
```

2) **Promote** proposals into *candidate* domain `.axi` modules (for review):

```bash
cd rust
cargo run -p axiograph-cli -- discover promote-proposals build/demo/proposals.json \
  -o build/demo/candidates \
  --domains machinist_learning
```

This writes (for example):

- `rust/build/demo/candidates/MachinistLearning.proposals.axi`
- `rust/build/demo/candidates/promotion_trace.json`

3) Validate the candidate module parses:

```bash
cd rust
cargo run -p axiograph-cli -- check validate build/demo/candidates/MachinistLearning.proposals.axi
```

The candidate `.axi` is **not canonical**: promotion into the accepted `.axi` plane is meant to be explicit (human review / policy gate).

For an end-to-end, no-LLM version of this loop (including accepted-plane snapshot ids),
see: `scripts/physics_discovery_deterministic_demo.sh`.

## 5) “Path algebra” / groupoid demos (normalization, path equivalence)

These are certificate kinds that exercise the HoTT/groupoid side more directly:

```bash
make verify-lean-e2e-normalize-path-v2
make verify-lean-e2e-path-equiv-v2
```

They show:

- path expressions (`id`, `inv`, `trans`)
- normalization certificates
- equivalence checking in Lean

See `docs/reference/CERTIFICATES.md` for the exact certificate shapes.

## 6) What to run in CI / locally

Rust-only (no Lean required):

```bash
cd rust
cargo test -p axiograph-cli --offline
```

This includes an end-to-end suite that:

- validates all `examples/**/*.axi`,
- runs all `examples/repl_scripts/*.repl`,
- emits a `query_result_v1` certificate for each script’s exported snapshot.

Lean-inclusive (trusted checker):

```bash
make verify-semantics
```

## 7) Common pitfalls (what “certified” does and doesn’t mean)

- **Certificate-checked ≠ true**: certificates prove *derivability from inputs*, not correctness of inputs.
- **Avoid “checker re-runs the engine”**: recompute-and-compare is fine for bootstrapping, but we must shrink the trusted surface by checking *meaning*, not re-implementing the same algorithm.
- **Make “unknown vs false” explicit**: no-result is usually “unknown”, not “false” (avoid silent closed-world assumptions).
- **Don’t confuse groupoid inverses with real-world invertibility**: `inv` is a formal witness operation, not a factual inverse relation.
- **Don’t treat confidence as calibrated truth-probability**: it’s an evidence calculus with invariants; interpretation remains domain-specific.
