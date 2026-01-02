-- Fibered transitivity demo (no `param (...)`).
--
-- This module is *well-typed* but should FAIL `axi_constraints_ok_v1`:
--
-- - We declare a transitive annotation on endpoints `(from,to)`,
-- - but our key includes `(ctx,time)`,
-- - and without `param (ctx,time)` there is no certificate-checkable meaning for
--   “what ctx/time should inferred transitive tuples live in?”.
--
-- Fix: add `param (ctx, time)` to interpret transitivity *within each fixed ctx/time fiber*.
--
-- See also:
-- - examples/demo_data/FiberedTransitivityParam.axi
-- - docs/explanation/CONSTRAINT_SEMANTICS.md
-- - docs/tutorials/FIBERED_CLOSURE_CONSTRAINTS.md

module FiberedTransitivityNoParam

schema Fibered:
  object Node
  object Context
  object Time
  object Evidence

  -- Note the field order: `ctx,time` come first.
  relation Accessible(ctx: Context, time: Time, from: Node, to: Node, witness: Evidence)

theory FiberedRules on Fibered:
  -- Carrier fields are explicitly the endpoints:
  constraint transitive Accessible on (from, to)

  -- We want the fact identity to include ctx/time.
  --
  -- Without `param (ctx,time)`, this is not certifiable by `axi_constraints_ok_v1`:
  -- transitive closure would have to invent ctx/time values for inferred endpoint pairs.
  constraint key Accessible(ctx, time, from, to)

instance SmallFiberedWorld of Fibered:
  Node = {Alice, Bob, Carol}
  Context = {Census2020}
  Time = {T2020}
  Evidence = {Ev0, Ev1}

  Accessible = {
    (ctx=Census2020, time=T2020, from=Alice, to=Bob, witness=Ev0),
    (ctx=Census2020, time=T2020, from=Bob, to=Carol, witness=Ev1)
  }

