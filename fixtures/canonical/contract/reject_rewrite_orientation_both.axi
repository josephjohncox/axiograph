module RewriteOrientationBoth

schema S:
  object Node

theory T on S:
  rewrite BadOrientation:
    vars: x: Node
    orientation: both
    lhs: refl(x)
    rhs: refl(x)
