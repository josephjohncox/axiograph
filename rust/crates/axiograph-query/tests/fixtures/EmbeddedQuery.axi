module EmbeddedQuery

schema Demo:
  object Node
  object Supplier
  subtype Supplier < Node
  relation Flow(from: Supplier, to: Supplier)

theory DemoRules on Demo:
  constraint key Flow(from, to)

instance I of Demo:
  Supplier = {a, b}
  Flow = {(from=a, to=b)}
