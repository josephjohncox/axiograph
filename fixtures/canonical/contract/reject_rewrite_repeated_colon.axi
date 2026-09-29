module HeaderParity
schema S
  object A
theory T on S
  rewrite duplicate_colon::
    vars: x: A
    lhs: refl(x)
    rhs: refl(x)
