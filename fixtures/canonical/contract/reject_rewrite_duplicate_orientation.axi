module RejectRewriteDuplicateOrientation

schema S:
  object Entity

theory T on S:
  rewrite bad:
    vars: x: Entity
    orientation: forward
    orientation: backward
    lhs: refl(x)
    rhs: refl(x)
