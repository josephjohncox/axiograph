module RewritePathId

schema S:
  object Node

theory T on S:
  rewrite BadPath:
    vars: x: Node
    lhs: id(x)
    rhs: refl(x)
