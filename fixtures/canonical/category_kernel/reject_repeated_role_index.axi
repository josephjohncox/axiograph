module RepeatedRoleIndex

schema S:
  object A
  relation R(a: A, b: indexed(A; a|a))
