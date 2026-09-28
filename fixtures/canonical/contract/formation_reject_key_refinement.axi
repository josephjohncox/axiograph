module FormationRejectKeyRefinement

schema S:
  object Entity
  relation R(first: Entity, refined_value: refined(Entity; key(first)))
