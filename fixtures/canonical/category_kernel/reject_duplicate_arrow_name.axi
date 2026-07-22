module CategoryKernelDuplicateArrow

schema S:
  object A
  object B
  object C
  subtype A < B as shared_inclusion
  subtype C < B as shared_inclusion
