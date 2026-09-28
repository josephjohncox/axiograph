module RejectRewriteMissingOrientationValue

schema S:
  object Entity

theory T on S:
  rewrite bad:
    vars: x: Entity
    orientation:
    lhs: refl(x)
    rhs: refl(x)
