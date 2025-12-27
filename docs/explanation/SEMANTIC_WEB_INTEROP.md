# Semantic Web Interop (RDF / OWL / SHACL / PROV)

**Diataxis:** Explanation  
**Audience:** contributors

This document captures how Axiograph interoperates with “Semantic Web” tooling
without letting that ecosystem redefine the **trusted kernel** of the system.

**Principle:** RDF/OWL/SHACL/PROV are **boundary layers**.

- The *canonical* and *reviewable* truth plane remains `.axi` (accepted plane).
- Rust may import/export and run heuristics (untrusted).
- Lean remains the trusted checker for certificates over **Axiograph semantics**.

This avoids two common failure modes:

1) “The checker just re-runs the engine.” (proving the implementation, not the meaning)
2) “Interop becomes the kernel.” (semantics drift or OWL entailment assumptions leaking in)

---

## 1) Import / export adapters (boundary layers)

### 1.1 RDF import (graph → proposals)

RDF import is an ingestion adapter that produces:

- `proposals.json` (Evidence/Proposals schema) with provenance,
- optional draft `.axi` candidates (reviewable), and
- evidence chunks (if the source has docs/comments).

Recommended mapping (prototype; evolves):

- RDF resources (IRIs / blank nodes) → `ProposalV1::Entity`
- `rdf:type` → entity `entity_type` (or an attribute, if ambiguous)
- predicate/object triples:
  - IRI object → `ProposalV1::Relation`
  - literal object → attribute on the subject entity (`attributes[predicate]=literal`)

Supported serializations (via Sophia):

- N-Triples (`.nt`, `.ntriples`)
- Turtle (`.ttl`, `.turtle`)
- N-Quads (`.nq`, `.nquads`)
- TriG (`.trig`)
- RDF/XML (`.rdf`, `.owl`, `.xml`)

### 1.1.1 Public datasets + demos (for realistic testing)

We keep large public datasets out of git, but provide:

- a tiny SHACL fixture under `examples/rdfowl/w3c_shacl_minimal/` (committed),
- scripts to fetch and ingest public datasets into `build/` (optional, networked).

Recommended workflow:

1) Fetch datasets:

   - `./scripts/fetch_public_rdfowl_datasets.sh`

2) Run a deterministic local demo (fixture) and an optional W3C slice ingest:

   - `./scripts/rdfowl_public_datasets_demo.sh`

Notes:

- This is intentionally **open-world**: lack of a triple is “unknown”, not “false”.
- When a serialization supports **named graphs** (N-Quads/TriG), we treat graph names as
  first-class `Context` entities (a “world”), and each emitted relation proposal carries
  a `context` attribute pointing at the graph-context id (otherwise: a document context).
- `axiograph discover draft-module` preserves this scoping when generating a candidate `.axi`:
  relations gain `@context Context` and tuples include `ctx=...`, so PathDB imports derive
  `axi_fact_in_context` edges and the REPL can scope queries via `ctx use ...`.
- OWL entailment is not assumed automatically; if we want an entailment, we make it
  explicit as either:
  - a proposed rewrite rule (engine proposes, Lean checks), or
  - a proposed constraint/shape (engine validates, Lean checks).

Minimal offline named-graph demo:

- `./scripts/rdf_named_graph_context_demo.sh`

### 1.2 OWL import (ontology → constraints + patterns)

OWL import is split into two outputs:

1) **Structural proposals** (classes, properties, subclass graph).
2) **Constraint candidates** (functional/symmetric/transitive, domain/range).

We treat OWL axioms as:

- *constraints/shapes* where feasible (ingestion validation step), or
- *rewrite rules* where feasible (certificate-backed derivations),
- otherwise as **assumptions** that remain untrusted unless explicitly promoted.

### 1.3 Export to RDF (accepted `.axi` → RDF)

Export is for interoperability and downstream tooling; it is *not* canonical.

Key design point: Axiograph relations are often **n-ary**. RDF triples are binary.

To export faithfully, we use the “edge-object” pattern (aka reification):

- each relation tuple becomes an RDF resource (a “fact node”),
- fields become predicate edges from the fact node to their values,
- provenance/context metadata attaches naturally to the fact node.

This is also the recommended modeling pattern for interop with OWL/SHACL tooling.

---

## 2) SHACL-like validation (certificate-checked ingestion)

We want SHACL-like validation as a **gate**:

```
raw graph (untrusted) → validate (untrusted Rust) → validated graph + certificate → Lean verifies
```

### 2.1 “Unknown vs false” is explicit

Validation produces a 3-valued result per shape:

- `Valid`
- `Invalid`
- `Unknown` (insufficient information under open-world semantics)

The ingestion contract must preserve `Unknown` explicitly, rather than silently
dropping it or treating it as failure.

### 2.2 Certificate shape (planned)

We plan a certificate kind:

- `shape_validation_v1`

Checked by Lean via a small, semantics-driven validator:

- parse anchored `.axi` snapshot/module,
- interpret a restricted shape language,
- recompute validation results and compare with the certificate payload.

This keeps the trusted surface small while making the ingestion gate auditable.

---

## 3) Context/world indexing + provenance (named graphs / PROV-inspired)

Provenance and context are first-class in Axiograph:

- facts are scoped to a *context/world* (time, source, authority, conversation, policy),
- provenance accumulation is explicit (who/what asserted the fact, when, why),
- certificates are context-scoped (what world the derivation is valid in).

### 3.1 Modeling

We support two compatible representations:

1) **Graph-level:** treat each accepted snapshot as a context; facts are snapshot-scoped.
2) **In-graph:** represent contexts explicitly as objects, and attach facts to contexts.

Long term we want named-graph–like indexing (N-Quads style) and PROV-like linking
for source chains.

### 3.2 Certificates (planned)

Planned certificate extensions:

- include `context_id` / snapshot id on query certificates,
- “context transport” certificates (e.g. supersession, trusted import, perspective alignment),
- optional authenticated context membership proofs for third-party/offline verification.

---

## 4) MVP implementation plan

Near-term milestones (ordered):

1) `.nt` / `.nq` import → `proposals.json` (provenance attached).
2) Export accepted `.axi` snapshots to RDF using edge-object pattern.
3) Add a restricted SHACL-like validator + `shape_validation_v1` certificate.
4) Add first-class context/named-graph indexing in PathDB + context-scoped certificates.
