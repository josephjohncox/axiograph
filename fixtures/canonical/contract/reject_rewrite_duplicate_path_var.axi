module DuplicateRewritePathVar

schema S:
  object A

theory T on S:
  rewrite duplicate_path:
    vars: x: A, y: A, p: Path(x,y), p: Path(x,y)
    lhs: p
    rhs: p
