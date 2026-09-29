module RefinePositive
schema S
  object A
  relation R(value:refined(A;cardinality(0|2)))
