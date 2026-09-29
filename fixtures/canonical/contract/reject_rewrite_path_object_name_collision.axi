module RewritePathObjectNameCollision

schema S:
  object A

theory T on S:
  rewrite path_object_collision:
    vars: x: A, y: A, p: Path(x,y), p: A
    lhs: p
    rhs: p
