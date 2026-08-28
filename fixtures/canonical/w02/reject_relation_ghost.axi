module RelationGhost
schema S:
  object Node
  relation Flow(from: Node, to: Node)
  relation Depends(flow: relation(Flow), node: Node)
instance I of S:
  Node = {A, B}
  Flow = { flow1: (from=A, to=B) }
  Depends = { (flow=ghost, node=A) }
