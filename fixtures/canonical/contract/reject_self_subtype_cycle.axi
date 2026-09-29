module SelfSubtypeCycle

schema S:
  object Node
  subtype Node < Node
