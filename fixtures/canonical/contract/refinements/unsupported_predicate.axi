module RefineInvalid
schema S
  object A
  relation R(value:refined(A;predicate(unsupported)))
