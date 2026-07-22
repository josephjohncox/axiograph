module ViolatedEquation
schema S:
  object Node
  function left: Node -> Node
  function right: Node -> Node
theory T on S:
  equation agree:
    left = right
instance I of S:
  Node = {A, B}
  left = { (source=A, target=A), (source=B, target=B) }
  right = { (source=A, target=B), (source=B, target=B) }
