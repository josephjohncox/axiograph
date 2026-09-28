module HandFormation
schema S
  object A
  relation R(base:A, value:refined(A;key(base)))
