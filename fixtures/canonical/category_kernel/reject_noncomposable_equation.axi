module CategoryKernelBadComposition

schema S:
  object A
  object B
  object C
  function f: A -> B
  function g: C -> A

theory Laws on S:
  equation noncomposable:
    f;g = f;g
