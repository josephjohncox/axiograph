module AtMostU32Overflow

schema S:
  object Node
  relation Edge(left: Node, right: Node)

theory T on S:
  constraint at_most 4294967296 Edge.left -> Edge.right
