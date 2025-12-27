-- Machining Demo (canonical `axi_schema_v1` syntax)
--
-- This example is intentionally small: it exists to exercise the basic
-- schema/theory/instance surface without pulling in the larger machining
-- knowledge modules.

module MachiningDemo

schema MaterialSchema:
  object Material
  object Property
  object CuttingParams

  relation has_property(material: Material, property: Property)
  relation uses_params(material: Material, params: CuttingParams)

theory MaterialTheory on MaterialSchema:
  -- A material can have multiple properties, but this example treats the
  -- `uses_params` link as functional for simplicity.
  constraint functional uses_params.material -> uses_params.params

instance MachiningKG of MaterialSchema:
  Material = {Steel, Titanium}
  Property = {HighHardness, LowConductivity}
  CuttingParams = {SteelParams, TitaniumParams}

  has_property = {
    (material=Steel, property=HighHardness),
    (material=Titanium, property=HighHardness),
    (material=Titanium, property=LowConductivity)
  }

  uses_params = {
    (material=Steel, params=SteelParams),
    (material=Titanium, params=TitaniumParams)
  }

