module IndexedFiberProjectionMismatch

schema S:
  object Node
  object ContextOne
  object ContextTwo
  relation Target(node: Node @data, ctx: ContextOne @context)
  relation Use(ctx: ContextTwo @context, target: indexed(relation(Target); ctx) @data)
