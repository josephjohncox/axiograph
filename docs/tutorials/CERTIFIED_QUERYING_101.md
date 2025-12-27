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

## 2) Emit a query certificate (Rust)

Because the input is a canonical `.axi` module, `axiograph cert query` also writes
a derived `PathDBExportV1` snapshot anchor (used by Lean verification).

```bash
bin/axiograph cert query \
  --input examples/ontology/OntologyRewrites.axi \
  --lang axql \
  --query 'select ?p where ?f = Parent(parent=?p, child=Bob) limit 10' \
  --out build/bob_parent.query_cert.json \
  --anchor-out build/OntologyRewrites.anchor.axi
```

You now have:

- `build/bob_parent.query_cert.json` (certificate)
- `build/OntologyRewrites.anchor.axi` (snapshot anchor for the checker)

---

## 3) Verify the certificate (Lean)

```bash
make verify-lean-cert \
  CERT=build/bob_parent.query_cert.json \
  AXI=build/OntologyRewrites.anchor.axi
```

If the checker succeeds, it prints a success line and exits with code 0.

---

## 4) What you just proved (and what you did not)

You proved:

- the returned binding(s) are **derivable from the anchored snapshot input** under the AxQL semantics.

You did **not** prove:

- the input facts are “true in the real world”.

This separation is intentional: certificates prove **derivability from accepted inputs**, not correctness of the inputs.

