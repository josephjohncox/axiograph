# Constraint Semantics (Open World) and What We Can Certify

**Diataxis:** Explanation  
**Audience:** contributors + `.axi` authors

This note explains what `.axi` **constraints** mean in Axiograph’s model, why only
some constraints are **certificate-checkable** today, and how to write constraints
so they stay consistent with the project’s core stance:

> Missing facts are usually **unknown**, not **false**.

Certificates prove **derivability from accepted inputs under the formal semantics**.
They do not prove that the inputs are “true in reality”.

## 1) Two different things called “constraints”

In practice, `.axi` theory constraints serve two distinct roles:

1. **Integrity/quality constraints** (checkable on the concrete instance data)  
   Examples: uniqueness keys and unary functional dependencies.

2. **Axioms / inference permissions** (about what can be derived)  
   Examples: “this relation is transitive”, “this is an equivalence relation”, or
   domain-specific laws that require witnesses/proofs.

Only (1) is straightforward to “check on the instance”. For (2), the right
mechanism is usually: certify each *use* of the axiom in an output certificate,
not attempt to prove a global property by re-running the engine.

## 2) What “certifying a constraint” should (and should not) mean

When we say a constraint kind is “certified”, we mean:

- Rust can emit a certificate claiming the anchored module satisfies the constraint
  under the intended semantics.
- Lean re-parses the anchored `.axi` and re-checks the same claim **fail-closed**.

We do **not** mean:

- completeness (“all consequences are explicitly materialized”), or
- truth of the inputs (grounding is a separate concern), or
- “the checker just re-runs the same optimizer/query”.

## 3) Constraint taxonomy (what’s consistent with open world)

### A) Well-formedness / typing (always required)

This is not a “theory constraint” in the logic sense; it’s a **syntax + typing**
gate:

- every instance assignment refers to declared objects/relations,
- every tuple has exactly the declared fields,
- every field value is in the declared object set (modulo subtyping closure).

This is certificate-backed via `axi_well_typed_v1`.

### B) Integrity constraints (extensional, checkable)

These are constraints that can be checked against the explicitly present tuples:

- `constraint key Rel(field, ...)`  
  No two tuples agree on the key-fields.

- `constraint functional Rel.src -> Rel.dst`  
  A special case of determinism (“src determines dst”).

- `constraint at_most N Rel.src -> Rel.dst`  
  Bounded fan‑out (“src determines at most N distinct dst values”).
  Optional fiber params: `... param (ctx, time)` to enforce per fixed assignment.

These are good candidates for certification because they are:

- local and deterministic,
- independent of “missing facts” being treated as false,
- directly useful for query planning and indexing.

### C) Closure-compatible constraints (derivation semantics, still checkable)

Some constraints are “inference-like” but have a **deterministic closure** that
does not require inventing new witness objects.

Example: symmetry annotations.

For `axi_constraints_ok_v1`, the certified semantics of:

- `constraint symmetric Rel`
- `constraint symmetric Rel where Rel.field in {A, B, ...}`

is **not** “the inverse tuple must exist”.

Instead, we treat symmetry as an *admissible closure* on the relation’s first two
fields (the “endpoints”), and we certify a compatibility property.

Carrier fields:
- By default, the carrier fields are the **first two** relation fields.
- For relations with extra fields (context/time/witnesses), authors can be explicit:
  `constraint symmetric Rel ... on (field0, field1)`.
Parameter fields (fibered closure):
- Optionally, authors can name which *non-carrier* fields should be treated as
  fixed parameters of the closure:
  `constraint symmetric Rel ... param (ctx, time)`.
- When `param (..)` is present, the certified symmetry semantics is:
  “swap endpoints **within each fixed assignment** of the parameter fields”.
  Other relation fields are treated as out-of-scope annotations/witnesses for
  this certificate kind.

- if you add the swapped-endpoint tuples (respecting any `where … in {…}` guard),
  the relation’s **key/functional** constraints remain satisfied.

This stays open-world-friendly and avoids demanding materialization.

### D) Typing/definitional rules (small executable subset)

Some “constraints” are better understood as **definitional typing rules** that
help the system elaborate and validate data/query shapes.

In `axi_constraints_ok_v1`, `constraint typing Rel: rule_name` is certified for a
small builtin set of rule names (see `docs/reference/CERTIFICATES.md`), where Lean
can validate consistency against supporting typing relations (e.g. `FormDegree`)
and treat missing output facts as “derivable”.

This is intentionally narrow: it’s a bridge between ontology engineering and a
future where more semantics move into first-class rewrite rules + certificates.

### E) Global axioms (usually not checkable as a single “module OK” gate)

Examples in the `.axi` corpus:

- `constraint transitive Rel`

In an open-world setting, transitivity is an **existential inference permission**:
if `Rel(a,b)` and `Rel(b,c)`, then `Rel(a,c)` is derivable even if it’s not
explicitly asserted. That makes it a poor fit for a “scan the tuples once and
decide OK” gate.

What we *can* do instead (and what aligns with “untrusted engine, trusted checker”):

- certify each use of transitivity in an output witness:
  - reachability certificates for “there exists an `Rel+` path”,
  - rewrite-derivation certificates for “this expression is equivalent under rules”.

As a pragmatic ontology-engineering gate, we can also certify a weaker property:

- the module’s **key/functional** constraints remain consistent under transitive
  closure on the relation’s *carrier fields* (by convention: the first two fields;
  or explicitly via `constraint transitive Rel on (field0, field1)`),
  so “treat this as transitive” won’t contradict your own uniqueness constraints,
  without requiring explicit materialization of the closure.
  When `param (..)` is present (e.g. `constraint transitive Accessible param (ctx, time)`),
  the closure is interpreted as operating *within each fixed parameter fiber*:
  `Accessible(ctx,time,a,b) ∧ Accessible(ctx,time,b,c) ⇒ Accessible(ctx,time,a,c)`.

### F) Negative constraints (require explicit closed-world intent)

Constraints like “acyclic”, “irreflexive”, “disjoint”, “no such tuple exists” are
not safe under open world unless you model negation explicitly (policy, signed
evidence, or an explicit closed-world region).

If we ever certify a negative constraint, it must be paired with a semantics that
makes “false” explicit (not inferred from absence).

## 4) Practical guidance for `.axi` authors

- Prefer **structured constraints** over free-form text:
  - If something isn’t expressible yet, use a named block:
    `constraint Name:` with an indented body.
- Avoid non-canonical functional dependency syntax:
  - use `functional Rel.a -> Rel.b` for unary dependencies,
  - use `key Rel(a,b,...)` for composite determinism.
- Keep in mind that the *certified* symmetry closure operates on the relation’s
  first two fields. If your “equivalence carrier” is some other pair, put that
  pair first in the relation signature (or plan to migrate to a richer constraint
  form that names the carrier fields explicitly).

## 5) How this plugs into promotion and tooling

- Accepted-plane promotion is strict about parseable semantics:
  - truly unknown constraints (`ConstraintV1.unknown`) are rejected for canonical
    modules, because they are a semantics drift hazard.
- `axi_constraints_ok_v1` also fails closed on unknown constraints (even if the
  certified subset would otherwise pass).
- `axi_constraints_ok_v1` is a conservative, high-ROI gate:
  - it certifies a small subset that is both open-world-compatible and valuable
    for query/optimization.

See also:

- `docs/reference/CERTIFICATES.md` (what each certificate kind checks)
- `docs/explanation/VERIFICATION_AND_GUARDRAILS.md` (“unknown ≠ false” and other failure modes)
- `lean/Axiograph/Axi/ConstraintsCheck.lean` (trusted checker implementation)

## 6) Worked example: `param (ctx,time)` makes transitivity certifiable

The most common place this comes up is relations that are scoped by context/time
and also carry extra “witness” fields:

```
Accessible(ctx: Context, time: Time, from: Node, to: Node, witness: Evidence)
```

If you try to write:

- `constraint transitive Accessible on (from,to)` and
- `constraint key Accessible(ctx,time,from,to)`

then `axi_constraints_ok_v1` must fail closed: a global closure-compatibility
checker cannot invent `ctx/time` values for inferred `(from,to)` endpoint pairs.

Adding:

```
param (ctx, time)
```

makes the intended meaning explicit: closure is interpreted *within each fixed
assignment* of `(ctx,time)` (“fibered closure”), so keys/functionals that mention
`ctx/time` become checkable.

End-to-end tutorial + demo scripts:

- `docs/tutorials/FIBERED_CLOSURE_CONSTRAINTS.md`
- `examples/demo_data/FiberedTransitivityNoParam.axi`
- `examples/demo_data/FiberedTransitivityParam.axi`
- `scripts/fibered_closure_constraints_demo.sh`

## 7) Proposal: negative constraints and richer guarded constraints

Many desirable ontology constraints are “negative” or require stronger guard
expressivity:

- **negative**: irreflexive, acyclic, disjointness, “no such tuple exists”, etc.
- **richer guards**: multi-field predicates, inequalities, numeric ranges, etc.

Under Axiograph’s open-world stance, these do not have a single uniform meaning:
absence of a fact is usually **unknown**, not **false**.

So we propose two separate mechanisms (and we should keep them distinct in both
UX and certification).

### A) Closed-world regions (explicit completeness claims)

If you want a negative constraint to be meaningful, you need an explicit
completeness boundary.

Proposal (canonical pattern):

- Treat a `Context` as the carrier of a *policy*.
- Add a first-class “completeness claim” object/fact for a relation within a
  context (and optionally within a key-fiber).

Sketch:

- `CompleteRel(ctx: Context, rel: AxiMetaRelationDecl, scope: Scope)` where `scope`
  might be “all tuples” or “all tuples matching guard G”.

Then negative constraints can be interpreted as:

- “in contexts where `CompleteRel` holds, the forbidden tuples are provably absent”.

Certification direction:

- introduce a separate certificate kind (e.g. `axi_closed_world_ok_v1`) that is
  explicitly scoped to a context and a relation, and checks the negative property
  by scanning the explicitly present tuples in that region.

This keeps open-world as the default while allowing explicit “closed” islands.

### B) Per-derivation certificates (for inference-like constraints)

Many constraints are really “inference permissions” (or rewrite rules), where the
right correctness story is:

- Rust produces a derivation/witness for a *specific* conclusion, and
- Lean checks that derivation against the formal semantics.

This is a better fit than a global “module OK” scan for:

- transitivity/symmetry when you want to *use* them (not just check key/FD compatibility),
- domain equational laws and reconciliation explanations,
- obligations/epistemics/modal reasoning steps.

Direction:

- continue expanding query/path certificates so witnesses can include equivalence
  chains + rewrite derivations (not only raw edge reachability).

### C) Richer guarded constraints: keep a certifiable subset

We can extend guard syntax, but we should be clear about certification:

- `axi_constraints_ok_v1` should only certify guard fragments that are
  decidable from the module’s explicit data (e.g. `field in {...}`, maybe `field = const`),
  and that don’t require inventing witness values.
- richer guards can be accepted as named blocks (for documentation / untrusted
  tooling) until a precise semantics + certifiable subset is designed.
