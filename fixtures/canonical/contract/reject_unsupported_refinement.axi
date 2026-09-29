module RejectUnsupportedRefinement

schema S:
  object Entity
  relation Broken(value: refined(Entity; predicate(custom_check|argument)))
