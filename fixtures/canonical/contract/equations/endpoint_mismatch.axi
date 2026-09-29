module EqScope
schema S
  object A
theory T on S
  equation bad:
    refl(x) = refl(y)
