module W02Full

schema Work:
  object Entity
  object Engineer
  object Context
  subtype Engineer < Entity
  relation Flow(from: Entity @data, to: Entity @data, ctx: Context @context)
  relation Audit(ctx: Context @context, flow: indexed(relation(Flow); ctx) @data, reviewer: refined(Engineer; enum(Alice|Bob)) @data)
  aspect owner: Engineer -> Entity

theory WorkRules on Work:
  constraint key Flow(from, to, ctx)

instance CurrentWork of Work:
  Entity = {Alice, Bob}
  Engineer = {Alice}
  Context = {Current}
  Flow = { flow1: (from=Alice, to=Bob, ctx=Current) }
  Audit = { (ctx=Current, flow=flow1, reviewer=Alice) }
  owner = { (source=Alice, target=Alice) }
