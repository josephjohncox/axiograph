module CategoryKernelRoleKey

schema S:
  object A
  relation R(first: A @data, second: refined(A; key(first)) @data)
