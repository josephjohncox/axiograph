module EquationComposition
schema S:
  object A
  relation Edge(from: A, to: A)
theory T on S:
  equation bad:
    trans(step(x, Edge, y), step(z, Edge, w)) = step(x, Edge, w)
