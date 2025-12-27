-- Physics Knowledge Ontology
-- Captures tacit physics understanding for engineering applications
--
-- This schema represents physical laws, dimensional analysis,
-- and engineering heuristics as typed knowledge.

module PhysicsKnowledge

schema Physics:
  -- ==========================================================================
  -- Dimensional Analysis
  -- ==========================================================================
  object Dimension           -- SI base dimensions
  object DimSignature        -- combination of dimensions with exponents
  object Quantity            -- value with dimension

  relation DimensionOf(qty: Quantity, dim: DimSignature)
  relation DimExponent(sig: DimSignature, baseDim: Dimension, exp: Scalar)

  -- ==========================================================================
  -- Physical Laws
  -- ==========================================================================
  object PhysicalLaw
  object LawCategory         -- mechanics, thermodynamics, electromagnetism, etc.

  relation LawInputs(law: PhysicalLaw, input: Quantity, dim: DimSignature)
  relation LawOutput(law: PhysicalLaw, output: Quantity, dim: DimSignature)
  relation LawFormula(law: PhysicalLaw, formula: Text)
  relation LawCategory(law: PhysicalLaw, category: LawCategory)
  relation LawConfidence(law: PhysicalLaw, conf: Confidence)

  -- ==========================================================================
  -- Material Properties
  -- ==========================================================================
  object Material
  object MaterialProperty
  object PropertyCategory    -- mechanical, thermal, electrical, etc.

  relation MaterialHasProperty(mat: Material, prop: MaterialProperty, value: Quantity)
  relation PropertyIsOfCategory(prop: MaterialProperty, cat: PropertyCategory)
  relation PropertyConditions(prop: MaterialProperty, condition: Text)
  relation PropertyUncertainty(mat: Material, prop: MaterialProperty, uncert: Confidence)

  -- ==========================================================================
  -- Engineering Heuristics
  -- ==========================================================================
  object Heuristic
  object HeuristicDomain     -- machining, structural, thermal, etc.

  relation HeuristicRule(h: Heuristic, rule: Text)
  relation HeuristicRationale(h: Heuristic, rationale: Text)
  relation HeuristicDomain(h: Heuristic, domain: HeuristicDomain)
  relation HeuristicApplicability(h: Heuristic, condition: Text)
  relation HeuristicConfidence(h: Heuristic, conf: Confidence)

  -- ==========================================================================
  -- Learning / concept graph (extension structures)
  -- ==========================================================================
  --
  -- This uses the same vocabulary as `examples/learning/MachinistLearning.axi`
  -- so PathDB can extract a typed `LearningGraph` and the REPL can demo
  -- "tacit ↔ learning" workflows on physics content too.
  object Concept
  object SafetyGuideline
  object Example

  relation requires(concept: Concept, prereq: Concept)
  relation explains(concept: Concept, guideline: SafetyGuideline)
  relation demonstrates(example: Example, concept: Concept)
  relation conceptDescription(concept: Concept, text: Text)
  relation exampleDescription(example: Example, text: Text)

  -- ==========================================================================
  -- Machining-Specific Physics
  -- ==========================================================================
  object CuttingCondition
  object ChipFormation
  object HeatPartition
  object StabilityRegion

  -- Cutting force models
  relation MerchantForceModel(mat: Material, shearAngle: Quantity, frictionAngle: Quantity)
  relation SpecificCuttingEnergy(mat: Material, kc: Quantity)  -- J/mm³

  -- Heat generation and partition
  relation HeatGeneration(cond: CuttingCondition, totalHeat: Quantity)
  relation HeatToChip(cond: CuttingCondition, fraction: Confidence)
  relation HeatToWorkpiece(cond: CuttingCondition, fraction: Confidence)
  relation HeatToTool(cond: CuttingCondition, fraction: Confidence)

  -- Chatter stability
  relation StabilityLobeData(tool: Tool, setup: Setup, rpm: Quantity, docLimit: Quantity)
  relation RegenerativeFrequency(setup: Setup, freq: Quantity)

  -- ==========================================================================
  -- Support types
  -- ==========================================================================
  object Scalar
  object Confidence
  object Text
  object Tool
  object Setup

theory PhysicsRules on Physics:
  -- Dimensional consistency
  constraint functional DimensionOf.qty -> DimensionOf.dim

  -- Properties are per material
  constraint key MaterialHasProperty(mat, prop)

  -- Laws have defined I/O
  constraint functional LawOutput.law -> LawOutput.output

instance TacitPhysicsKnowledge of Physics:
  -- Dimensions
  Dimension = {Length, Mass, Time, Temperature, Amount, Current, Luminosity}

  -- Law categories
  LawCategory = {Mechanics, Thermodynamics, FluidDynamics, MaterialsScience}

  -- Property categories
  PropertyCategory = {Mechanical, Thermal, Electrical, Chemical, Optical}

  -- Heuristic domains
  HeuristicDomain = {Machining, StructuralDesign, ThermalManagement, Welding}

  -- Physical laws
  PhysicalLaw = {NewtonsSecond, KineticEnergy, TaylorToolLife, MerchantShear, FourierHeat}
  Scalar = {Zero, One, Half, TwoThirds}
  Text = {
    -- Law formulas
    FmaFormula, KEFormula, TaylorFormula,

    -- Concept descriptions (stable identifiers; full prose can live in adjacent docs/chunks)
    Text_Desc_DimensionalAnalysis,
    Text_Desc_ThermalConductivity,
    Text_Desc_HeatPartitioning,
    Text_Desc_RegenerativeChatter,
    Text_Desc_SpecificCuttingEnergy,

    -- Example descriptions
    Text_Ex_Ti_Roughing_OK,
    Text_Ex_Ti_Roughing_TooFast,
    Text_Ex_Al_HighSpeed_OK
  }

  LawFormula = {
    (law=NewtonsSecond, formula=FmaFormula),
    (law=KineticEnergy, formula=KEFormula),
    (law=TaylorToolLife, formula=TaylorFormula)
  }

  LawCategory = {
    (law=NewtonsSecond, category=Mechanics),
    (law=KineticEnergy, category=Mechanics),
    (law=TaylorToolLife, category=MaterialsScience),
    (law=MerchantShear, category=MaterialsScience),
    (law=FourierHeat, category=Thermodynamics)
  }

  -- Engineering heuristics (tacit knowledge!)
  Heuristic = {
    CuttingForceProportional,
    HeatGoesToChip,
    ChatterStabilityLobes,
    TitaniumLowSpeed,
    AluminumHighSpeed,
    ToolWearDiffusion
  }

  Confidence = {High, Medium, Low, VeryHigh}

  HeuristicRule = {
    (h=CuttingForceProportional, rule=Force_increases_with_DOC_and_feed),
    (h=HeatGoesToChip, rule=Most_heat_enters_chip_at_high_speeds),
    (h=ChatterStabilityLobes, rule=Stable_zones_exist_between_RPM_and_DOC),
    (h=TitaniumLowSpeed, rule=Cut_titanium_at_low_speed_high_feed),
    (h=AluminumHighSpeed, rule=Aluminum_can_run_at_very_high_speeds),
    (h=ToolWearDiffusion, rule=Diffusion_wear_dominates_at_high_temperatures)
  }

  HeuristicDomain = {
    (h=CuttingForceProportional, domain=Machining),
    (h=HeatGoesToChip, domain=Machining),
    (h=ChatterStabilityLobes, domain=Machining),
    (h=TitaniumLowSpeed, domain=Machining),
    (h=AluminumHighSpeed, domain=Machining),
    (h=ToolWearDiffusion, domain=Machining)
  }

  HeuristicConfidence = {
    (h=CuttingForceProportional, conf=VeryHigh),
    (h=HeatGoesToChip, conf=High),
    (h=ChatterStabilityLobes, conf=VeryHigh),
    (h=TitaniumLowSpeed, conf=High),
    (h=AluminumHighSpeed, conf=VeryHigh),
    (h=ToolWearDiffusion, conf=Medium)
  }

  HeuristicRationale = {
    (h=CuttingForceProportional, rationale=Chip_cross_section_determines_material_removal),
    (h=HeatGoesToChip, rationale=Less_time_for_conduction_at_high_speed),
    (h=ChatterStabilityLobes, rationale=Regenerative_vibration_creates_unstable_regions),
    (h=TitaniumLowSpeed, rationale=Poor_thermal_conductivity_concentrates_heat)
  }

  -- ==========================================================================
  -- Learning graph: Concepts ↔ guidelines ↔ examples
  -- ==========================================================================
  --
  -- These are *not* "hard laws"; they are a structured way to connect:
  --   - conceptual prerequisites (`requires`)
  --   - guidelines/heuristics (`explains`)
  --   - observations and case studies (`demonstrates`)
  --
  -- Importantly, this stays explicit and queryable: the system doesn't
  -- silently turn tacit heuristics into ground truth.

  Concept = {
    DimensionalAnalysis,
    ThermalConductivityConcept,
    HeatPartitioningConcept,
    RegenerativeChatter,
    SpecificCuttingEnergyConcept
  }

  -- Reuse the existing heuristic identifiers as safety-guideline nodes.
  -- This intentionally makes them multi-typed (Heuristic + SafetyGuideline).
  SafetyGuideline = {
    CuttingForceProportional,
    HeatGoesToChip,
    ChatterStabilityLobes,
    TitaniumLowSpeed,
    AluminumHighSpeed,
    ToolWearDiffusion
  }

  Example = {
    Example_Ti_Roughing_OK,
    Example_Ti_Roughing_TooFast,
    Example_Al_HighSpeed_OK
  }

  requires = {
    (concept=HeatPartitioningConcept, prereq=ThermalConductivityConcept),
    (concept=SpecificCuttingEnergyConcept, prereq=DimensionalAnalysis),
    (concept=RegenerativeChatter, prereq=DimensionalAnalysis)
  }

  explains = {
    (concept=ThermalConductivityConcept, guideline=TitaniumLowSpeed),
    (concept=HeatPartitioningConcept, guideline=HeatGoesToChip),
    (concept=RegenerativeChatter, guideline=ChatterStabilityLobes),
    (concept=SpecificCuttingEnergyConcept, guideline=CuttingForceProportional)
  }

  conceptDescription = {
    (concept=DimensionalAnalysis, text=Text_Desc_DimensionalAnalysis),
    (concept=ThermalConductivityConcept, text=Text_Desc_ThermalConductivity),
    (concept=HeatPartitioningConcept, text=Text_Desc_HeatPartitioning),
    (concept=RegenerativeChatter, text=Text_Desc_RegenerativeChatter),
    (concept=SpecificCuttingEnergyConcept, text=Text_Desc_SpecificCuttingEnergy)
  }

  demonstrates = {
    (example=Example_Ti_Roughing_OK, concept=ThermalConductivityConcept),
    (example=Example_Ti_Roughing_TooFast, concept=HeatPartitioningConcept),
    (example=Example_Ti_Roughing_TooFast, concept=SpecificCuttingEnergyConcept),
    (example=Example_Al_HighSpeed_OK, concept=SpecificCuttingEnergyConcept)
  }

  exampleDescription = {
    (example=Example_Ti_Roughing_OK, text=Text_Ex_Ti_Roughing_OK),
    (example=Example_Ti_Roughing_TooFast, text=Text_Ex_Ti_Roughing_TooFast),
    (example=Example_Al_HighSpeed_OK, text=Text_Ex_Al_HighSpeed_OK)
  }

  -- Material properties
  Material = {Al6061_T6, Ti6Al4V, Inconel718, AISI_4140}

  MaterialProperty = {YieldStrength, ThermalConductivity, HardnessHRC, SpecificHeat}

  -- Specific cutting energy values (tacit knowledge from experience)
  Quantity = {Kc_Al_0_7, Kc_Ti_2_5, Kc_Inc_3_5, Kc_Steel_1_5}

  SpecificCuttingEnergy = {
    (mat=Al6061_T6, kc=Kc_Al_0_7),
    (mat=Ti6Al4V, kc=Kc_Ti_2_5),
    (mat=Inconel718, kc=Kc_Inc_3_5),
    (mat=AISI_4140, kc=Kc_Steel_1_5)
  }
