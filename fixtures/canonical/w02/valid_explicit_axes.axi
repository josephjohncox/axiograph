module W02Axes

schema Axes:
  object Entity
  object Context
  object World
  object Time
  object Parameter
  object Evidence
  relation Observation(subject: Entity @data, ctx: Context @context, world: World @world, time: Time @temporal, parameter: Parameter @parameter, evidence: Evidence @evidence)

instance Current of Axes:
  Entity = {E}
  Context = {C}
  World = {W}
  Time = {T}
  Parameter = {P}
  Evidence = {Proof}
  Observation = { (subject=E, ctx=C, world=W, time=T, parameter=P, evidence=Proof) }
