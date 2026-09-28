module EquationEndpoints
schema S:
  object A
  relation Edge(from: A, to: A)
theory T on S:
  equation bad:
    step(x, Edge, y) = refl(x)
