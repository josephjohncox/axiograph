module RejectRewriteDuplicateLhs

schema S:
  object Entity

theory T on S:
  rewrite bad:
    vars: x: Entity
    lhs: refl(x)
    lhs: refl(x)
    rhs: refl(x)
