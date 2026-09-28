module RelationCarrierParity
schema S
  object Node
  relation Edge(from: Node, to: Node)
  relation Pair(left: relation(Edge), right: relation(Edge))
theory T on S
  equation relation_carrier:
    step(x, Pair, y) = step(x, Pair, y)
