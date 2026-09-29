module EquationUnknownRelation
schema S:
  object A
  relation Edge(from: A, to: A)
theory T on S:
  equation bad:
    step(x, Missing, y) = refl(x)
