module RejectRewriteDuplicateVars

schema S:
  object Entity

theory T on S:
  rewrite bad:
    vars: x: Entity
    vars: y: Entity
    lhs: refl(x)
    rhs: refl(x)
