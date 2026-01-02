-- Constraint certification demo (axi_constraints_ok_v1)
--
-- This module intentionally exercises the certified subset of theory constraints
-- beyond `key(...)` and `functional ...`:
--
-- - conditional symmetry:
--     `constraint symmetric Relationship where Relationship.relType in {Friend}`
-- - transitivity annotations:
--     `constraint transitive Accessible`
-- - executable typing rules (small builtin set):
--     `constraint typing HodgeStar: depends_on_metric_and_dualizes_degree`
--
-- Used by: `make verify-lean-e2e-axi-constraints-ok-v1`

module ConstraintsOkDemo

schema Demo:
  object Person
  object RelType

  -- A polymorphic-ish relationship relation (only some relTypes are symmetric).
  relation Relationship(from: Person, to: Person, relType: RelType)

  -- A tiny differential-forms fragment to demonstrate typing constraints.
  object Nat
  object Manifold
  object Metric
  object DifferentialForm

  relation ManifoldDimension(manifold: Manifold, dim: Nat)
  relation MetricOn(metric: Metric, manifold: Manifold)
  relation FormOn(form: DifferentialForm, manifold: Manifold)
  relation FormDegree(form: DifferentialForm, degree: Nat)

  relation HodgeStar(metric: Metric, input: DifferentialForm, output: DifferentialForm)

  -- A classic "manager" relation (functional from employee).
  relation ReportsTo(employee: Person, manager: Person)

  -- A tiny transitive relation to demonstrate "closure compatibility" checks.
  relation Accessible(from: Person, to: Person)

theory DemoRules on Demo:
  -- Relationship is symmetric only for certain kinds.
  constraint symmetric Relationship where
    Relationship.relType in {Friend}

  -- Uniqueness constraints (also used by the typing rule checks).
  constraint key ManifoldDimension(manifold, dim)
  constraint key MetricOn(metric, manifold)
  constraint key FormOn(form, manifold)
  constraint key FormDegree(form, degree)

  -- Functional dependency example (employee -> manager).
  constraint key ReportsTo(employee, manager)
  constraint functional ReportsTo.employee -> ReportsTo.manager

  -- Accessible is transitive (open world: we do not require explicit materialization).
  -- The certifiable check is: transitive closure remains compatible with keys/functionals.
  constraint key Accessible(from, to)
  constraint transitive Accessible

  -- Executable typing rule: Hodge star depends on a metric and dualizes degree.
  constraint typing HodgeStar: depends_on_metric_and_dualizes_degree

instance DemoInstance of Demo:
  Person = {Alice, Bob, Carol}
  RelType = {Friend, Mentor}

  Relationship = {
    (from=Alice, to=Bob, relType=Friend),
    (from=Bob, to=Alice, relType=Mentor)
  }

  Nat = {Nat0, Nat1, Nat2, Nat3, Nat4}
  Manifold = {M4}
  Metric = {g}
  DifferentialForm = {F}

  ManifoldDimension = {(manifold=M4, dim=Nat4)}
  MetricOn = {(metric=g, manifold=M4)}
  FormOn = {(form=F, manifold=M4)}
  FormDegree = {(form=F, degree=Nat2)}

  -- We intentionally omit FormOn/FormDegree for `F_dual` to show that the typing
  -- constraint can treat it as derivable, while still checking consistency if
  -- explicit facts exist.
  HodgeStar = {(metric=g, input=F, output=F_dual)}

  ReportsTo = {(employee=Alice, manager=Bob)}

  Accessible = {
    (from=Alice, to=Bob),
    (from=Bob, to=Carol)
  }
