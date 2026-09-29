module ScopeValid
schema S
  object A
  relation R(left:A, right:A)
theory T on S
  rewrite valid:
    vars: x: A
    lhs: refl(x)
    rhs: refl(x)
