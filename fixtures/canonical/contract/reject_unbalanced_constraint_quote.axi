module RejectUnbalancedConstraintQuote

schema S:
  object A

theory T on S:
  constraint review_only "
