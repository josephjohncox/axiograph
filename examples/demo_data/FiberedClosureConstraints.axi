-- Fibered closure constraints demo
--
-- This module demonstrates the canonical:
--
--   `param (field0, field1, ...)`
--
-- clause for closure-style constraints:
--
-- - `constraint symmetric Rel ... param (...)`
-- - `constraint transitive Rel ... param (...)`
--
-- In Axiograph’s open-world stance, these constraints are **not** “must materialize all
-- implied tuples”. Instead, they are *closure-compatibility* annotations:
--
-- - the checker does **not** require inverse/transitive tuples to be explicitly present,
-- - it checks that keys/functionals remain consistent under the intended closure,
-- - and `param (...)` says “perform the closure on endpoints **within each fixed assignment**
--   of these parameter fields” (e.g. `ctx`, `time`, `kind`).
--
-- See: docs/explanation/CONSTRAINT_SEMANTICS.md

module FiberedClosureConstraints

schema Fibered:
  object Node
  object Context
  object Time
  object Evidence
  object RelKind

  -- Accessibility with explicit context/time scoping and an evidence witness.
  relation Accessible(from: Node, to: Node, ctx: Context, time: Time, witness: Evidence)

  -- A polymorphic relationship: only some kinds are symmetric, and symmetry is interpreted
  -- fibered by `(ctx, kind)`. The extra `witness` field is treated as an out-of-scope
  -- annotation for the purposes of the certified constraint checker.
  relation Relationship(a: Node, b: Node, kind: RelKind, ctx: Context, witness: Evidence)

  -- A simple symmetric relation scoped to a context.
  relation Knows(a: Node, b: Node, ctx: Context)

theory FiberedRules on Fibered:
  -- Transitivity is interpreted on `(from,to)` **within each fixed (ctx,time)** fiber:
  --
  --   Accessible(ctx,time,a,b) ∧ Accessible(ctx,time,b,c) ⇒ Accessible(ctx,time,a,c)
  --
  -- `param (ctx,time)` is what makes keys mentioning ctx/time certifiable.
  constraint transitive Accessible on (from, to) param (ctx, time)

  -- Treat (from,to,ctx,time) as the tuple identity for accessibility facts.
  -- Without `param (ctx,time)`, this would be rejected by `axi_constraints_ok_v1` as
  -- “key mentions non-carrier fields”.
  constraint key Accessible(from, to, ctx, time)

  -- Only some relationship kinds are symmetric, and symmetry is interpreted within each
  -- fixed `(ctx, kind)` fiber (so kinds do not mix).
  constraint symmetric Relationship where Relationship.kind in {Friend, Colleague}
    on (a, b) param (ctx, kind)

  constraint key Relationship(a, b, kind, ctx)

  -- Symmetry scoped by ctx.
  constraint symmetric Knows param (ctx)
  constraint key Knows(a, b, ctx)

instance SmallFiberedWorld of Fibered:
  Node = {Alice, Bob, Carol}
  Context = {Census2020, FamilyTree2023}
  Time = {T2020, T2023}
  Evidence = {Ev0, Ev1, Ev2}
  RelKind = {Friend, Rival, Colleague}

  -- Note: transitive closure tuples are not required to be explicitly present.
  Accessible = {
    (from=Alice, to=Bob, ctx=Census2020, time=T2020, witness=Ev0),
    (from=Bob, to=Carol, ctx=Census2020, time=T2020, witness=Ev1),

    -- Same endpoints in a different fiber (ctx/time), treated independently.
    (from=Alice, to=Carol, ctx=FamilyTree2023, time=T2023, witness=Ev2)
  }

  -- Only Friend/Colleague relationships are symmetric.
  Relationship = {
    (a=Alice, b=Bob, kind=Friend, ctx=Census2020, witness=Ev0),
    (a=Bob, b=Carol, kind=Rival, ctx=Census2020, witness=Ev1),
    (a=Alice, b=Carol, kind=Colleague, ctx=FamilyTree2023, witness=Ev2)
  }

  -- We assert only one direction; symmetry is a closure annotation.
  Knows = {
    (a=Alice, b=Bob, ctx=Census2020),
    (a=Carol, b=Alice, ctx=FamilyTree2023)
  }

