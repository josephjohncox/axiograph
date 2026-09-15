module RejectTransitiveShape

schema S:
  object Entity
  relation R(left: Entity, right: Entity)

theory T on S:
  constraint transitive R where extra
