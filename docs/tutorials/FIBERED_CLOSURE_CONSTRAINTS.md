# Fibered Closure Constraints (`param (...)`)

**Diataxis:** Tutorial  
**Audience:** `.axi` authors + contributors

This tutorial shows a common open‑world pattern:

- relations that are scoped by **context/time** (and often carry extra “witness” fields),
- closure‑style constraints like **transitive** / **symmetric**,
- and how `param (...)` makes a conservative, certificate‑checked subset possible.

The key idea is: in Axiograph, these constraints are **closure annotations**, not “must materialize all implied tuples”.

## 1) The problem: transitivity with `(ctx,time,from,to,...)`

Suppose we model reachability/accessibility scoped by context and time:

- `ctx` identifies a world/named graph (source, authority, conversation, policy, etc).
- `time` identifies a temporal scope (timestamp/interval).
- `from,to` are the endpoints.
- `witness` is an out‑of‑band annotation (evidence pointer, comment, provenance id, etc).

In a canonical `.axi` schema:

```
relation Accessible(ctx: Context, time: Time, from: Node, to: Node, witness: Evidence)
```

We want to annotate:

```
constraint transitive Accessible on (from, to)
constraint key Accessible(ctx, time, from, to)
```

But without additional structure, a “transitive closure compatibility” certificate cannot decide:
**what `ctx` and `time` should an inferred `(from,to)` live in?**

## 2) See the failure (expected)

The repo includes a minimal module that is well‑typed but should **fail** the
`axi_constraints_ok_v1` gate for exactly this reason:

- `examples/demo_data/FiberedTransitivityNoParam.axi`

Run:

```bash
./bin/axiograph check validate examples/demo_data/FiberedTransitivityNoParam.axi
./bin/axiograph cert constraints examples/demo_data/FiberedTransitivityNoParam.axi
```

You should see an error like:

```
Error: transitive `Fibered.Accessible`: key constraint mentions non-carrier/non-param field `ctx`
```

This is a **fail‑closed** outcome: we prefer “cannot certify this meaning” over silently
dropping semantics.

## 3) Fix it: interpret closure *within each fixed fiber*

If the intended meaning is:

> `Accessible(ctx,time,a,b)` and `Accessible(ctx,time,b,c)` implies `Accessible(ctx,time,a,c)`  
> **for the same `ctx,time`**

…then the closure has a canonical, certificate‑checkable meaning:

```
constraint transitive Accessible on (from, to) param (ctx, time)
```

The repo includes the fixed version:

- `examples/demo_data/FiberedTransitivityParam.axi`

Run:

```bash
OUT=build/fibered_transitivity_param_demo
mkdir -p "$OUT"

./bin/axiograph check validate examples/demo_data/FiberedTransitivityParam.axi
./bin/axiograph cert constraints examples/demo_data/FiberedTransitivityParam.axi --out "$OUT/axi_constraints_ok_v1.json"

make verify-lean-cert AXI=examples/demo_data/FiberedTransitivityParam.axi CERT="$OUT/axi_constraints_ok_v1.json"
```

## 4) Full end-to-end demo (script)

For a single command that:

- validates both modules,
- demonstrates the expected failure without `param (...)`,
- emits + Lean‑verifies the passing certificate,
- renders a typed-overlay HTML viz for a richer example,

run:

```bash
./scripts/fibered_closure_constraints_demo.sh
```

## 5) What this certificate does (and does not) mean

`axi_constraints_ok_v1` is a conservative gate intended for ontology engineering:

- It does **not** require the inverse/transitive tuples to be explicitly present.
- It checks that your **keys/functionals remain consistent** under the intended closure,
  so treating the relation as symmetric/transitive won’t immediately contradict your own
  extensional uniqueness constraints.

If you want to *use* transitivity/symmetry for answers, that belongs in
**per-derivation certificates** (reachability witnesses, rewrite derivations, query result certs),
not in a single global “module OK” scan.

See:
- `docs/explanation/CONSTRAINT_SEMANTICS.md`
- `docs/reference/CERTIFICATES.md`

