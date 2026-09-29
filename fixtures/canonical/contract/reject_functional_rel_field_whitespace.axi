module FunctionalRelFieldWhitespace

schema S:
  object A
  relation R(left: A, right: A)

theory T on S:
  constraint functional R .left -> R.right
