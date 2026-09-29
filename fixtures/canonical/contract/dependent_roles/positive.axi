module DepPositive
schema S
  object A
  relation R(base:A, value:indexed(A;base))
