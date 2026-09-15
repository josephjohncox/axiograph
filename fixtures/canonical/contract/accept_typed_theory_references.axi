module AcceptTypedTheoryReferences

schema S:
  object A
  relation R(from: A, to: A)

theory T on S:
  constraint functional R.from -> R.to
  rewrite identity_step:
    vars: x: A, y: A
    lhs: step(x, R, y)
    rhs: step(x, R, y)
