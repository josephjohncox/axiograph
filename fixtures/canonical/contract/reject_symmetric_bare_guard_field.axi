module RejectSymmetricBareGuardField

schema S:
  object A
  relation R(left: A, right: A)

theory T on S:
  constraint symmetric R where left in {A}
