module RewriteRulesAnchor

schema S:
  object Obj
  relation Edge(from: Obj, to: Obj)

theory T on S:
  -- A tiny `.axi`-defined rewrite rule (v1) used by the `rewrite_derivation_v3`
  -- e2e test.
  --
  -- This mirrors the builtin groupoid rewrite `id_left`, but is declared as an
  -- explicit *first-class* rule so certificates can reference it by:
  --
  --   axi:<axi_digest_v1>:T:id_left_axi
  rewrite id_left_axi:
    vars: x: Obj, y: Obj, p: Path(x,y)
    lhs: trans(refl(x), p)
    rhs: p

