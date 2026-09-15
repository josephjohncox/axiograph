module EquationDeclaredOrderCarrier
schema S:
  object A
  relation Pair(lhs: A, rhs: A)
theory T on S:
  equation stable:
    step(x, Pair, y) = step(x, Pair, y)
