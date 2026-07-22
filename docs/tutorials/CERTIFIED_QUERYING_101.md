# Certified Querying 101 (Rust emits, Lean verifies)

**Diataxis:** Tutorial  
**Audience:** users (and contributors)

This tutorial walks through a minimal end-to-end “untrusted engine, trusted checker” flow:

1. write/use a canonical `.axi` module (meaning plane),
2. express the intent as a question first,
3. compile the question into `CompiledFiniteQuery` (untrusted Rust),
4. request `query_result_v4` through a bound verifier route, and
5. accept the result only with the matching Lean receipt.

We use `examples/ontology/OntologyRewrites.axi`.

---

## 0) Build

```bash
make all
```

This builds:

- Rust CLI: `bin/axiograph`
- Lean checker: `lean/` (including `axiograph_verify`)

If you only want the checker executable:

```bash
make lean-exe
```

---

## 1) Start with the question

Question: “Who is Bob’s parent?”

In `OntologyRewrites.axi`, the instance includes:

- `Parent(parent=Alice, child=Bob)`

For authoring and CQ review, keep this intent question-first. A `.cq` form would
look like:

```text
version competency_question_bundle_v1

question bob_parent:
  ask: Who is Bob's parent?
  expect: exists OrgFamily.Parent(parent=?p, child=Bob)
  min_rows: 1
```

Load/lower authored CQs when you want the question-first report:

```bash
bin/axiograph discover competency-questions \
  examples/ontology/OntologyRewrites.axi \
  --from-cq examples/competency_questions/bob_parent.cq \
  --no-schema \
  --out build/bob_parent.cq.json
```

The executable compiler still consumes explicit structured query intent. The
equivalent human-facing lowered query text is:

```text
select ?p where ?f = OrgFamily.Parent(parent=?p, child=Bob) limit 10
```

---

## 2) Run through the compiled query family

CLI/REPL, HTTP, semantic MCP, and internal callers all lower into
`CompiledFiniteQuery`. The HTTP request is structured `query_ir_v1`; it returns
`family = compiled_finite_query` and makes no certificate claim because an
`.axpd` materialization cannot reconstruct exact accepted `.axi` bytes.

For a certified answer, use semantic MCP `axql_run` with
`certificate_policy = require_verified`. The server must be configured with:

- exact accepted `.axi` bytes for the runtime anchor;
- an approved `axiograph_verify` executable SHA-256;
- build id `axiograph-verify-main-v3`; and
- a positive verifier timeout.

The route emits `query_result_v4`, invokes Lean, validates every receipt field,
and returns authoritative `verified_rows`. There is no emission-only
query-certificate command.

---

## 3) Exercise the exact finite checker locally

```bash
make verify-lean-e2e-query-result-module-v4
make verify-lean-certificate-rejections
```

The positive path covers finite type, bounded path, and disjunctive queries. The
durable adversarial corpus rejects a missing row, a duplicate substituted row,
and any truncated answer.

---

## 4) What you just proved (and what you did not)

You proved, for the accepted receipt:

- every returned full binding has valid type/attribute/path witnesses against
  the exact accepted module;
- no satisfying binding in the declared bounded finite denotation is missing;
- the prepared query, ordered selected answer, exact certificate bytes, module
  revision, checker binary, and receipt are mutually bound.

You did **not** prove that the accepted facts are true in the world, that the
ontology is closed, that evidence retrieval is exhaustive, that approximate
operators are complete, or that general dependent/HoTT reasoning is decidable.
The theorem is exact only for the declared finite query fragment.
