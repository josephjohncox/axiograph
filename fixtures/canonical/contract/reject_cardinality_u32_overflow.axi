module CardinalityU32Overflow

schema S:
  object Node
  relation Edge(value: refined(Node; cardinality(0|4294967296)))
