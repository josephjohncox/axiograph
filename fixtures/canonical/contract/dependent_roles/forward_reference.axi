module DepForward
schema S
  object A
  relation R(first:indexed(A;later), later:A)
