module RejectSymmetricShape

schema S:
  object Entity
  relation R(left: Entity, right: Entity)

theory T on S:
  constraint symmetric R where
