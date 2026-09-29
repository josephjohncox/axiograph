# Exact bytes in this file are a parser-contract fixture.
module ContractAllConstructors
import Zed
import Alpha

schema Surface:
  object A
  object B
  object Context
  object World
  object Time
  object Parameter
  object Evidence
  subtype B < A
  subtype A < B as a_to_b
  relation Base(left: A @data, right: B @data)
  relation EveryRole(
    first: A @data,
    relation_value: relation(Base) @data,
    indexed_value: indexed(A; first) @context,
    refined_value: refined(A; eq(A0); in(A0|A1); cardinality(1|2); key(first); enum(A0|A1); predicate(Custom|arg)) @world,
    time: Time @temporal,
    parameter: Parameter @parameter,
    evidence: Evidence @evidence
  )
  aspect observe: A -> B
  function return_to_a: B -> A @reversible

theory SurfaceTheory on Surface:
  constraint functional Base.left -> Base.right
  constraint at_most 2 Base.left -> Base.right param (left)
  constraint typing Base: typed_rule
  constraint symmetric Base where Base.left in {A0, A1} on (left, right) param (left)
  constraint symmetric Base on (left, right) param (left)
  constraint transitive Base on (left, right) param (left)
  constraint key Base(left, right)
  constraint ReviewOnly:
    preserve this body exactly after trimming
    second review-only line
  constraint unsupported_shape remains visible
  equation identity_text:
    id(A) = id(A)
  rewrite forward_rule:
    orientation: forward
    vars: x: A, p: Path(A,A)
    lhs: p
    rhs: refl(x)
  rewrite backward_rule:
    orientation: backward
    vars: x: A, y: B
    lhs: step(x, Base, y)
    rhs: inv(step(x, Base, y))
  rewrite bidirectional_rule:
    orientation: bidirectional
    vars: x: A, y: B, z: A
    lhs: trans(step(x, Base, y), step(y, Base, z))
    rhs: step(x, Base, z)

instance SurfaceData of Surface:
  A = {A0, A1}
  B = {B0}
  Context = {C0}
  World = {W0}
  Time = {T0}
  Parameter = {P0}
  Evidence = {E0}
  Base = {base0: (left=A0, right=B0), (left=A1, right=B0)}
  EveryRole = {(first=A0, relation_value=base0, indexed_value=A0, refined_value=A0, time=T0, parameter=P0, evidence=E0)}
  observe = {(source=A0, target=B0)}
  return_to_a = {(source=B0, target=A0)}
