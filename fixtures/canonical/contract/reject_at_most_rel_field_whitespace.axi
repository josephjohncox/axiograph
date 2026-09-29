module AtMostRelFieldWhitespace

schema S:
  object A
  relation R(left: A, right: A)

theory T on S:
  constraint at_most 1 R.left -> R. right
