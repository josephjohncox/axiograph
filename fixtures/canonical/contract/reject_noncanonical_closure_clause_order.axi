module RejectNoncanonicalClosureClauseOrder

schema S:
  object A
  relation R(left: A, right: A, ctx: A @context)

theory T on S:
  constraint symmetric R param (ctx) on (left, right)
