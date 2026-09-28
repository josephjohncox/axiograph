module RejectKeyShape

schema S:
  object Entity
  relation R(left: Entity, right: Entity)

theory T on S:
  constraint key R left
