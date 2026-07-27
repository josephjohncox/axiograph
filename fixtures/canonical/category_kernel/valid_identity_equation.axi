module IdentityEquation

schema S:
  object A

theory T on S:
  equation identity:
    id(A) = id(A)
