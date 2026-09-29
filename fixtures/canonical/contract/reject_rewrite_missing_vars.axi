module RejectRewriteMissingVars

schema S:
  object Entity

theory T on S:
  rewrite bad:
    lhs: refl(x)
    rhs: refl(x)
