module RejectTypingShape

schema S:
  object Entity
  relation R(left: Entity, right: Entity)

theory T on S:
  constraint typing R
