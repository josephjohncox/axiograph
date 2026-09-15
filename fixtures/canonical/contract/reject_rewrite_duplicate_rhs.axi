module RejectRewriteDuplicateRhs

schema S:
  object Entity

theory T on S:
  rewrite bad:
    vars: x: Entity
    lhs: refl(x)
    rhs: refl(x)
    rhs: refl(x)
