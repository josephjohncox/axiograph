module PartialFunction
schema S:
  object Node
  function next: Node -> Node
instance I of S:
  Node = {A, B}
  next = { (source=A, target=B) }
