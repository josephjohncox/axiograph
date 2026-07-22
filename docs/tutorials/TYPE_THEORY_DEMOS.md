# Type Theory Demos: Paths, Witnesses, Queries, and Certificates

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

The canonical schema semantics does not treat every relation as a binary edge.
It treats:

- object types and **relation objects** as category objects;
- each relation role as a total **projection arrow** from the relation object;
- explicit aspects/functions/subtype inclusions as other generators;
- endpoint-indexed paths as composites; and
- parallel-path equations/equivalence witnesses as explicit artifacts.

PathDB may derive binary traversal views when a carrier pair is explicit. That
projection is not the meaning plane. The HoTT/groupoid intuition applies to the
indexed path layer: alternative paths can carry explicit paths-between-paths.

In the migration plan, Rust is the **untrusted engine** and Lean is the **trusted checker**:

- Rust is allowed to be clever (indexes, heuristics, optimizations).
- Rust must emit a **certificate** (a witness).
- Lean checks the certificate against the formal semantics.

That’s what “proof-relevant” means here: the system can tell you *why* it believes something,
not just *that* it does.

### Run the finite semantics demo

From `axiograph_v6/`:

```bash
make verify-lean-theory
```

This runs the finite Lean category/dependent/groupoid fragment and focused Rust
regressions. The positive cases exercise:

- relation objects with typed role projections;
- indexed category paths and mathlib-backed groupoid laws;
- dependent role and context witnesses;
- finite refinements and typed holes; and
- bounded reachability saturation with replayable explanations.

The adversarial cases require rejection of bad role projections,
non-composable equations, unsupported/out-of-range refinements, exceeded
finite bounds, and tampered explanations.

The resulting saturation claim is intentionally narrow: finite generator
reachability. It does not execute arbitrary ontology rewrites, prove
termination/confluence, close an open world, or establish unrestricted HoTT or
topos semantics. The module is theorem support outside `VerifyMain` until a
versioned anchored certificate family is reviewed and dispatched there.

## 1) Proof-irrelevant exploration (REPL)

The quickest way to see the “paths + witness” structure is to import a
canonical `.axi` module that contains explicit relation/path witnesses.

Run a scenario script:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/family_hott_axi_demo.repl
```

Or import a canonical module that contains explicit schema morphisms and
equivalences:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/schema_evolution_axi_demo.repl
```

Try the proof-relevant-shaped queries inside the script (also runnable manually):

```text
# Basic typed traversals:
q select ?p where name("Alice") -Parent-> ?p limit 10
q select ?s where name("Alice") -Spouse-> ?s limit 10

# Inspect the alternative path witnesses directly:
q select ?to where name("Kevin") -PathEquivalence-> ?to limit 10
q select ?f ?lhs ?rhs where ?f is PathEquivalence, ?f -path1-> ?lhs, ?f -path2-> ?rhs limit 10
```

In this mode you get answers fast; you *don’t* get a machine-checkable witness.

## 2) Proof-relevant auditing (certificate-backed queries)

The same query can be run in **certified mode**:

1. Start from a canonical `.axi` module:
   - e.g. `examples/ontology/OntologyRewrites.axi`.
2. Emit a typed query witness anchored to that module:
3. Verify the certificate in Lean.

### 2.1 Choose a canonical `.axi` module

For this walkthrough we use:

- `examples/ontology/OntologyRewrites.axi`

### 2.2 Compile the canonical finite query

For everyday authoring, start from `.cq` or `ask` and let tooling lower the
question into `query_ir_v1`, then `CompiledFiniteQuery`. The direct
emission-only certificate command was removed; certified product routes must
bind the compiled query, exact accepted bytes, answer, and Lean receipt.

From `axiograph_v6/`:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover competency-questions examples/ontology/OntologyRewrites.axi \
  --from-cq examples/competency_questions/bob_parent.cq \
  --no-schema \
  --out build/ontology_rewrites_bob_parent_cq.json
```

A V4 result is **proof-relevant**: every returned full binding carries typed
atom witnesses anchored to canonical `.axi` names/facts, while Lean separately
checks exact equality with the bounded finite denotation.

### 2.3 Verify in Lean (trusted checker)

Run the repository's positive and adversarial V4 gates:

```bash
make verify-lean-e2e-query-result-module-v4
```

## 3) Proof relevance vs proof irrelevance (practical take)

In type theory, *proof irrelevance* roughly means “the program doesn’t care which proof you have”.

In Axiograph:

- **proof-irrelevant execution** is for iteration and performance (`repl`, `q`, `sql`, `ask`).
- **proof-relevant execution** is for auditability (bound `query_result_v4`, normalization certs, reconciliation certs, …).

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
- explicit path witnesses for the two “order derivations"

Run:

```bash
cd rust
cargo run -p axiograph-cli -- repl --script ../examples/repl_scripts/synthetic/proto_api_demo.repl
```

Then inspect:

```text
q select ?next where name("acme.svc0.v1.Service0.CreateWidget") -workflow_suggests_order-> ?next limit 10
q select ?next where name("acme.svc0.v1.Service0.CreateWidget") -observed_next-> ?next limit 10
q select ?p where ?p is PathWitness, ?p -from-> name("acme.svc0.v1.Service0.CreateWidget") limit 10
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
- **context-scoped tuples** via explicit `ctx : Context` roles (so “missing” is *unknown*, not *false*),
- **2-cells** via `RouteEquivalence(..., proof=...)` (path between paths),
- **proof terms** for obligations (`JustificationPath` objects),
- and a small “knowledge generation” slice by adding `DocChunk(text=...)` nodes in the REPL and exploring them with `fts(...)`.

Run the canonical REPL script:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  repl --script examples/repl_scripts/supply_chain_modalities_hott_demo.repl
```

### 4.2 Tacit knowledge via ingestion + explicit promotion (reviewable `.axi`)

This is the “GraphRAG → Axiograph” flow:

1) **Ingest** untrusted text into `proposals.json` (Evidence/Proposals schema):

```bash
cd rust
mkdir -p build/demo
cargo run -p axiograph-cli -- ingest doc ../examples/ingest_sources/machining_conversation.txt \
  --out build/demo/proposals.json \
  --machining \
  --chunks build/demo/chunks.json \
  --facts build/demo/facts.json
```

1) **Promote** proposals into *candidate* domain `.axi` modules (for review):

```bash
cd rust
cargo run -p axiograph-cli -- discover promote-proposals build/demo/proposals.json \
  -o build/demo/candidates \
  --domains machinist_learning
```

This writes (for example):

- `rust/build/demo/candidates/MachinistLearning.proposals.axi`
- `rust/build/demo/candidates/promotion_trace.json`

1) Validate the candidate module parses:

```bash
cd rust
cargo run -p axiograph-cli -- check validate build/demo/candidates/MachinistLearning.proposals.axi
```

The candidate `.axi` is **not canonical**: promotion into the accepted `.axi` plane is meant to be explicit (human review / policy gate).

For a deterministic no-LLM loop, use the ingest, draft, validate, and typed
AxiStore promotion steps above. There is no filesystem accepted-plane script.

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
- emits canonical `.axi`-anchored typed query witnesses for canonical-module demos.

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
