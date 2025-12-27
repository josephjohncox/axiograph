# Ontology Engineering Roadmap (process + quality + reuse)

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap captures **non-negotiable engineering work** needed to make Axiograph “production ontology engineering”-ready.

It is inspired by standard ontology engineering practice (e.g. Keet’s *An Introduction to Ontology Engineering*, 2nd ed.) and is designed to fit the repo’s core architecture:

- Rust is the untrusted engine (fast, scalable, heuristic).
- Lean is the trusted semantics + checker (certificate-first).
- `.axi` is the canonical, reviewable source plane.

The goal is not “build a perfect ontology once”; it is “build an ontology program with tests, quality gates, versioning, and explainable changes”.

For math-first foundations and certificates, see `docs/explanation/BOOK.md`.

---

## 0. Guiding principle

Treat an ontology like production code:

- requirements,
- tests (competency questions),
- lint/quality gates,
- versioning and releases,
- and reproducible builds.

---

## 1. Requirements and competency questions (CQs)

### 1.1 Make CQs first-class in `.axi`

- [ ] Add a `cq` block to the `.axi` grammar (Rust + Lean parsers).
- [ ] Define a CQ type that includes: id, natural language question, formal query, expected answer shape, and context/snapshot constraints.
- [ ] Add `axi test` (or `make verify-cq`) that executes CQs and checks:
  - results exist (when expected),
  - results are certificate-backed,
  - and the answers match expected constraints (exact set, subset, or “at least one”).

### 1.2 CQ template support (optional)

- [ ] Add a small CQ template library (inspired by controlled CQ template approaches) to help authors write formalisable CQs.
- [ ] Allow LLM-assisted CQ proposal, but require:
  - explicit human confirmation,
  - and the CQ must be executable/tested before it “counts”.

---

## 2. Ontology test-driven development (TDD) and regression suites

- [ ] Add a canonical folder (e.g. `examples/cq/`) with:
  - `.axi` modules,
  - CQ suites,
  - and golden expected outputs/certificates for CI.
- [ ] Add “entailment tests” (or Axiograph-equivalent) that assert certain consequences must/ must not hold.
- [ ] Ensure failures produce *actionable explanations* (derivation/certificate → human narrative).

---

## 3. Quality gates: linting, pitfalls, and taxonomy checks

### 3.1 `axi lint` (pitfall scanner)

- [ ] Implement an `axi lint` command with checks inspired by common ontology pitfalls:
  - cycles in `is-a` / subtype graph,
  - suspicious “misc/other” buckets,
  - inconsistent naming conventions,
  - “synonyms as classes” / duplicate concept creation patterns,
  - wrong inverse modeling patterns (where applicable),
  - missing disjointness / missing constraints where required by policy,
  - unsafe negation patterns (e.g., “NotX” class anti-pattern) in rule encoding.

### 3.2 OntoClean-style meta-properties (optional but high leverage)

- [ ] Add optional meta-annotations for rigidity/identity/unity/dependence on classes.
- [ ] Add a checker that flags violations of the meta-rules (taxonomy incoherence).
- [ ] Decide which subset should become **certificate-checkable** invariants (Lean side).

### 3.3 Quality metrics + dashboards

- [ ] Define a minimal quality scorecard per module:
  - CQ coverage,
  - number of lint warnings (by severity),
  - modularisation size metrics,
  - and change deltas per release.

---

## 4. Ontology Design Patterns (ODPs) as reusable `.axi` components

- [ ] Create a pattern library (start small): n-ary relation pattern (edge-object), role pattern, participation pattern, part-whole pattern variants, provenance/context pattern.
- [ ] Define “pattern instantiation” as a first-class operation:
  - emits `.axi` expansions,
  - and optionally emits certificates that the expansion preserves typing/invariants.
- [ ] Add a “housekeeping pattern” layer: naming conventions, labels, ids, metadata templates.

---

## 5. Modularisation and reuse (scaling + collaboration)

- [ ] Define module boundaries and module metadata for `.axi` (owners, purpose, version, dependencies).
- [ ] Implement module extraction for reuse:
  - branch/locality extraction (“pull a fragment starting from entity/class X”),
  - abstraction modules (drop details; keep taxonomy),
  - expressiveness modules (lower to a fragment by approximation rules).
- [ ] Add “privacy modules” (redaction/sanitization) as a first-class workflow:
  - define what is removed,
  - prove (certificate) that removal policy was applied.

---

## 6. Ontology matching / alignment (and mapping strength)

- [ ] Add a correspondence/alignment representation:
  - entity pairs + relation (equivalent/subsumed/similar) + confidence + provenance.
- [ ] Distinguish “alignment candidate” from “mapping accepted”:
  - accepted mapping must satisfy explicit constraints (no unsatisfiable entities, no policy violations).
- [ ] Add certificate kinds for:
  - simple alignment decisions (string/synset based with provenance),
  - and “mapping satisfiable” checks (Lean-defined, recompute-and-compare first).
- [ ] Support heterogeneous/pattern-based alignment (mapping between different modeling styles) as a long-term goal.

---

## 7. OBDA / linking ontologies to data (production data access)

- [ ] Specify a mapping layer from data sources (SQL/CSV/JSON) into `.axi` facts with stable identifiers.
- [ ] Add query rewriting strategies (e.g., “query by ontology terms → data-source query plan”).
- [ ] Emit certificates for rewriting correctness in the core cases (start with a narrow fragment).

---

## 8. Publishing, metadata, and lifecycle management

- [ ] Define a module header standard in `.axi` (name, version, authorship, license, provenance).
- [ ] Add changelog + deprecation/supersession workflow (“effective dates”).
- [ ] Tie certificates to snapshot ids and module hashes (already a direction elsewhere in the repo).

---

## 9. LLM assistance (untrusted by default)

- [ ] Allow LLMs to propose:
  - candidate classes/relations,
  - CQ drafts,
  - pattern instantiations,
  - alignment candidates,
  - and verbalizations/explanations.
- [ ] Require:
  - quarantine of proposals,
  - reconciliation + quality gates,
  - and certificates for any promoted “trusted/grounding” facts.

---

## 10. Semantic Web interop (RDF / OWL / SHACL / PROV) as boundary layers

The Semantic Web ecosystem is valuable for interoperability, but it must not
become Axiograph’s trusted kernel.

- [ ] Import/export adapters (RDF/SPARQL/OWL/SHACL) as **boundary layers** (not the internal trusted kernel).
- [ ] SHACL-like validation as a certificate-checked ingestion gate (“raw → validated”), preserving “unknown” explicitly.
- [ ] Context/world indexing (named graphs / PROV-inspired) as first-class semantics + certificates.

Design notes: `docs/explanation/SEMANTIC_WEB_INTEROP.md`.
