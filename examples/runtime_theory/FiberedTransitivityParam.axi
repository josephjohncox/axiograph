-- Fibered transitivity demo (with `param (ctx, time)`).
--
-- This module should PASS `axi_constraints_ok_v1`:
--
-- - `param (ctx,time)` interprets transitivity *within each fixed ctx/time fiber*:
--
--     Accessible(ctx,time,a,b) ∧ Accessible(ctx,time,b,c) ⇒ Accessible(ctx,time,a,c)
--
-- - That makes keys/functionals that mention ctx/time certificate-checkable (no invented
--   ctx/time values).
--
-- See also:
-- - examples/runtime_theory/FiberedTransitivityNoParam.axi
-- - docs/tutorials/FIBERED_CLOSURE_CONSTRAINTS.md

module FiberedTransitivityParam

schema Fibered:
  object Node
  object Context
  object Time
  object Evidence

  relation Accessible(ctx: Context @context, time: Time @temporal, from: Node, to: Node, witness: Evidence @evidence)

theory FiberedRules on Fibered:
  constraint transitive Accessible on (from, to) param (ctx, time)
  constraint key Accessible(ctx, time, from, to)

instance SmallFiberedWorld of Fibered:
  Node = {Alice, Bob, Carol}
  Context = {Census2020}
  Time = {T2020}
  Evidence = {Ev0, Ev1, Ev2}

  Accessible = {
    (ctx=Census2020, time=T2020, from=Alice, to=Bob, witness=Ev0),
    (ctx=Census2020, time=T2020, from=Bob, to=Carol, witness=Ev1),
    (ctx=Census2020, time=T2020, from=Alice, to=Carol, witness=Ev2)
  }
