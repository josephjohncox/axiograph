module RejectConstraintUnknownField

schema S:
  object A
  relation R(x: A, y: A)

theory T on S:
  constraint functional R.x -> R.missing
