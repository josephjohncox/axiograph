module M
schema S
  object A
  relation Edge(value: A)
  relation Box(edge: relation( Edge ))
