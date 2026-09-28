module UnknownConstraintSuffix

schema S:
  object Node
  relation Edge(left: Node, right: Node)

theory T on S:
  constraint review_only on (left, right)
