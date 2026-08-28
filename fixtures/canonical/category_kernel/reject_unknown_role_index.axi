module CategoryKernelUnknownIndex

schema S:
  object A
  relation R(first: A @data, second: indexed(A; missing) @data)
