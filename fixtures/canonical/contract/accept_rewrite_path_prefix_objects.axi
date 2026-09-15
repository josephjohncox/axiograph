module M
schema S
  object Path
  object Pathology
theory T on S
  rewrite r
    vars: x: Path, y: Pathology
    lhs: refl(x)
    rhs: refl(x)
