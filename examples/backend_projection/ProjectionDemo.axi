module ProjectionDemo

schema SupplyProjection:
  object Part
  object Plant
  object World
  object Evidence
  object CertifiedPart
  subtype CertifiedPart < Part
  relation Transfer(part: Part @data, source: Plant @data, target: Plant @data, world: World @world, evidence: Evidence @evidence)
  aspect home: Part -> Plant

theory SupplyProjectionRules on SupplyProjection:
  constraint key Transfer(part, source, target, world, evidence)
  equation transfer_path_stable:
    step(part, Transfer, plant) = step(part, Transfer, plant)

instance ProjectionSample of SupplyProjection:
  Part = {P1}
  Plant = {A, B}
  World = {Observed}
  Evidence = {Receipt1}
  CertifiedPart = {P1}
  Transfer = {
    transfer_1: (part=P1, source=A, target=B, world=Observed, evidence=Receipt1)
  }
  home = {
    (source=P1, target=A)
  }
