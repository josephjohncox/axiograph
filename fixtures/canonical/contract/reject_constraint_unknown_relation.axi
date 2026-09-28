module RejectConstraintUnknownRelation

schema S:
  object A

theory T on S:
  constraint functional Missing.x -> Missing.y
