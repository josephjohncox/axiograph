module CategoryKernelDuplicateEquation

schema S:
  object A
  function f: A -> A

theory First on S:
  equation same_name:
    f = f

theory Second on S:
  equation same_name:
    f = f
