module RejectRewriteUnknownRelation

schema S:
  object A

theory T on S:
  rewrite bad_relation:
    vars: x: A, y: A
    lhs: step(x, Missing, y)
    rhs: refl(x)
