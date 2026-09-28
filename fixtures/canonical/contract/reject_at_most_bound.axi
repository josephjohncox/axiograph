module RejectAtMostBound

schema S:
  object Entity
  relation R(left: Entity, right: Entity)

theory T on S:
  constraint at_most nope R.left -> R.right
