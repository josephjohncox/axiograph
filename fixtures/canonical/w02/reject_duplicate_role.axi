module DuplicateRole
schema S:
  object A
  relation R(value: A, value: A)
