module U32Maximum

schema S:
  object Node
  relation Edge(
    left: Node,
    right: refined(Node; cardinality(4294967295|4294967295))
  )

theory T on S:
  constraint at_most 4294967295 Edge.left -> Edge.right
