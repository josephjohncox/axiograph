# Certified Querying 101 (Rust emits, Lean verifies)

**Diataxis:** Tutorial  
**Audience:** users (and contributors)

This tutorial walks through a minimal end-to-end “untrusted engine, trusted checker” flow:

1. write/use a canonical `.axi` module (meaning plane),
2. run a query and emit a **certificate** (Rust),
3. verify the certificate against the formal semantics (Lean).

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

## 1) Write the query (AxQL)

Question: “Who is Bob’s parent?”

In `OntologyRewrites.axi`, the instance includes:

- `Parent(parent=Alice, child=Bob)`

So we bind the tuple node and select `?p`:

```text
select ?p where ?f = Parent(parent=?p, child=Bob) limit 10
```

---

## 2) Emit a typed query witness (Rust)

For canonical `.axi` input, `axiograph cert query` now emits the canonical
`.axi`-anchored typed query-witness path directly.

```bash
bin/axiograph cert query \
  --lang axql \
  examples/ontology/OntologyRewrites.axi \
  'select ?p where ?f = Parent(parent=?p, child=Bob) limit 10' \
  --out build/bob_parent.query_cert.json
```

You now have:

- `build/bob_parent.query_cert.json` (certificate)

---

## 3) Verify the certificate (Lean)

```bash
make verify-lean-cert \
  CERT=build/bob_parent.query_cert.json \
  AXI=examples/ontology/OntologyRewrites.axi
```

If the checker succeeds, it prints a success line and exits with code 0.

---

## 4) What you just proved (and what you did not)

You proved:

- the returned binding(s) are **derivable from the anchored canonical `.axi` input** under the AxQL semantics.

You did **not** prove:

- the input facts are “true in the real world”.

This separation is intentional: certificates prove **derivability from accepted inputs**, not correctness of the inputs.
