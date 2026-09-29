module HeaderParity
schema S
  object A
theory T on S
  equation duplicate_colon::
    opaque(A) = opaque(A)
