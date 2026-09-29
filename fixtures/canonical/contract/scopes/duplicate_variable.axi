module ScopeDuplicate
schema S
  object A
theory T on S
  rewrite invalid:
    vars: x: A, x: A
    lhs: refl(x)
    rhs: refl(x)
