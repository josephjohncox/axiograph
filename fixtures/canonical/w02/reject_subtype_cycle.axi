module SubtypeCycle
schema S:
  object A
  object B
  subtype A < B
  subtype B < A
