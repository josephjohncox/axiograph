module RejectFunctionalShape

schema S:
  object Entity
  relation R(left: Entity, right: Entity)

theory T on S:
  constraint functional R.left => R.right
