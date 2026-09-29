module RejectBadDependentRole

schema S:
  object Entity
  relation Broken(first: Entity, dependent: indexed(Entity; future), future: Entity)
