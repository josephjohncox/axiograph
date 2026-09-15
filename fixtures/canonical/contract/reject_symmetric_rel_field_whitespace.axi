module SymmetricRelFieldWhitespace

schema S:
  object A
  object Kind
  relation R(left: A, right: A, kind: Kind)

theory T on S:
  constraint symmetric R where R .kind in {friend}
