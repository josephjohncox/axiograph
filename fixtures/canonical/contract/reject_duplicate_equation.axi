module DuplicateEquation
schema S:
  object A
theory T on S:
  equation duplicate:
    opaque(x) = opaque(x)
  equation duplicate:
    other(x) = other(x)
