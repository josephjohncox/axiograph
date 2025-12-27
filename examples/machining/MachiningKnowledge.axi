-- Machining Knowledge Ontology
-- Conceptual domain model for manufacturing knowledge capture
--
-- This schema captures tacit machining knowledge including:
-- - Materials science (workpiece properties, hardness, machinability)
-- - Chip formation (shear zones, built-up edge, chip types)
-- - Tool wear (flank wear, crater wear, tool life, Taylor equation)
-- - Chatter and vibration (stability lobes, regenerative chatter)
-- - Cutting regimes (roughing, finishing, HSM)
-- - Tolerancing and GD&T
-- - Inspection and CMM
-- - Process planning and scheduling

module MachiningKnowledge

schema Machining:
  -- ==========================================================================
  -- Materials Science
  -- ==========================================================================
  object Material
  object MaterialProperty
  object HardnessScale     -- Rockwell, Brinell, Vickers
  object MachinabilityRating
  object MicroStructure

  relation MaterialHardness(mat: Material, scale: HardnessScale, value: Scalar)
  relation MaterialMachinability(mat: Material, rating: MachinabilityRating)
  relation MaterialStructure(mat: Material, structure: MicroStructure)
  relation MaterialThermalConductivity(mat: Material, value: Scalar)
  relation MaterialYieldStrength(mat: Material, value: Scalar)

  -- ==========================================================================
  -- Tools and Tooling
  -- ==========================================================================
  object Tool
  object ToolGeometry
  object ToolMaterial
  object Coating
  object InsertGrade

  relation ToolHasGeometry(tool: Tool, geometry: ToolGeometry)
  relation ToolMadeOf(tool: Tool, material: ToolMaterial)
  relation ToolCoating(tool: Tool, coating: Coating)
  relation ToolRakeAngle(tool: Tool, value: Scalar)
  relation ToolClearanceAngle(tool: Tool, value: Scalar)
  relation ToolNoseRadius(tool: Tool, value: Scalar)

  -- ==========================================================================
  -- Chip Formation
  -- ==========================================================================
  object ChipType           -- continuous, discontinuous, segmented, BUE
  object ShearZone
  object BuiltUpEdge

  relation CuttingProducesChip(mat: Material, tool: Tool, chipType: ChipType) @context Context
  relation ChipFormationShearAngle(mat: Material, tool: Tool, angle: Scalar) @context Context
  relation BUEConditions(mat: Material, tool: Tool, speed: Scalar) @context Context

  -- ==========================================================================
  -- Tool Wear
  -- ==========================================================================
  object WearType           -- flank, crater, notch, thermal cracking
  object WearMechanism      -- abrasion, adhesion, diffusion, oxidation

  relation ObservedWear(tool: Tool, wearType: WearType, amount: Scalar) @temporal Time @context Context
  relation WearMechanismActive(mat: Material, tool: Tool, mechanism: WearMechanism) @context Context
  relation TaylorToolLife(mat: Material, tool: Tool, C: Scalar, n: Scalar)  -- V*T^n = C

  -- ==========================================================================
  -- Cutting Parameters
  -- ==========================================================================
  object CuttingRegime      -- roughing, semi-finish, finish, HSM
  object Scalar
  object Context
  object Time

  relation RecommendedSpeed(mat: Material, tool: Tool, regime: CuttingRegime, sfm: Scalar)
  relation RecommendedFeed(mat: Material, tool: Tool, regime: CuttingRegime, ipt: Scalar)
  relation RecommendedDOC(mat: Material, tool: Tool, regime: CuttingRegime, doc: Scalar)
  relation ActualParameters(op: Operation, speed: Scalar, feed: Scalar, doc: Scalar) @temporal Time

  -- ==========================================================================
  -- Chatter and Vibration
  -- ==========================================================================
  object StabilityLobe
  object ChatterEvent
  object FrequencyMode

  relation StabilityBoundary(tool: Tool, setup: Setup, rpm: Scalar, docLimit: Scalar)
  relation ObservedChatter(op: Operation, freq: Scalar, amplitude: Scalar) @temporal Time @context Context
  relation ChatterModeShape(chatter: ChatterEvent, mode: FrequencyMode)
  relation RegenerativeChatter(tool: Tool, workpiece: Workpiece, dominantFreq: Scalar)

  -- ==========================================================================
  -- Tolerancing and GD&T
  -- ==========================================================================
  object Feature
  object ToleranceType      -- position, flatness, perpendicularity, cylindricity, etc.
  object Datum
  object ToleranceZone

  relation FeatureTolerance(feat: Feature, tolType: ToleranceType, value: Scalar)
  relation FeatureDatum(feat: Feature, datum: Datum)
  relation ToleranceStackup(assembly: Assembly, resultingTol: Scalar)
  relation ProcessCapability(op: Operation, feat: Feature, cpk: Scalar)

  -- ==========================================================================
  -- Inspection and CMM
  -- ==========================================================================
  object Inspection
  object CMMMeasurement
  object InspectionMethod   -- CMM, optical, surface profilometry, etc.

  relation InspectionResult(insp: Inspection, feat: Feature, measured: Scalar, nominal: Scalar) @temporal Time
  relation InspectionMethod(insp: Inspection, method: InspectionMethod)
  relation PassFail(insp: Inspection, pass: Bool)
  relation SurfaceFinishMeasurement(insp: Inspection, feat: Feature, Ra: Scalar) @temporal Time

  -- ==========================================================================
  -- Process Planning
  -- ==========================================================================
  object Operation
  object Setup
  object Workpiece
  object Assembly
  object ProcessPlan

  relation OperationPrecedence(before: Operation, after: Operation)
  relation OperationSetup(op: Operation, setup: Setup)
  relation OperationTool(op: Operation, tool: Tool)
  relation OperationFeature(op: Operation, feat: Feature)
  relation PlanContainsOperation(plan: ProcessPlan, op: Operation, sequence: Scalar)

  -- ==========================================================================
  -- Scheduling and Capacity
  -- ==========================================================================
  object Machine
  object WorkCenter
  object TimeSlot

  relation MachineCapability(machine: Machine, op: Operation)
  relation OperationCycleTime(op: Operation, estimated: Scalar, actual: Scalar) @context Context
  relation MachineAvailability(machine: Machine, slot: TimeSlot, available: Bool)
  relation QueueLength(machine: Machine, jobs: Scalar) @temporal Time

  -- ==========================================================================
  -- Auxiliary concepts
  -- ==========================================================================
  object Bool

theory MachiningRules on Machining:
  -- Functional dependencies
  constraint functional MaterialHardness.mat -> MaterialHardness.value
  constraint functional TaylorToolLife.mat -> TaylorToolLife.C
  constraint functional FeatureTolerance.feat -> FeatureTolerance.value

  -- Keys
  constraint key InspectionResult(insp, feat)
  constraint key OperationPrecedence(before, after)
  constraint key PlanContainsOperation(plan, op)

-- Example instance with some tacit knowledge
instance MachinistKnowledge of Machining:
  -- Materials
  Material = {Al6061_T6, Ti6Al4V, Inconel718, AISI_4140, SS_316L}
  HardnessScale = {HRC, HRB, Brinell}
  MachinabilityRating = {Excellent, Good, Fair, Difficult, VeryDifficult}
  MicroStructure = {FCC, BCC, HCP}

  MaterialHardness = {
    (mat=Al6061_T6, scale=HRB, value=95),
    (mat=Ti6Al4V, scale=HRC, value=36),
    (mat=Inconel718, scale=HRC, value=40),
    (mat=AISI_4140, scale=HRC, value=28),
    (mat=SS_316L, scale=HRB, value=79)
  }

  MaterialMachinability = {
    (mat=Al6061_T6, rating=Excellent),
    (mat=Ti6Al4V, rating=Difficult),
    (mat=Inconel718, rating=VeryDifficult),
    (mat=AISI_4140, rating=Good),
    (mat=SS_316L, rating=Fair)
  }

  -- Tools
  Tool = {Endmill_HSS_0.5, Endmill_Carbide_0.5, Insert_CNMG, Drill_Carbide_0.25}
  ToolMaterial = {HSS, Carbide, Ceramic, CBN, PCD}
  Coating = {TiN, TiAlN, AlTiN, DLC, Uncoated}
  CuttingRegime = {Roughing, SemiFinish, Finish, HSM}

  -- Cutting recommendations (tacit knowledge encoded)
  Scalar = {SFM100, SFM300, SFM500, SFM800, SFM1200, IPT0_002, IPT0_005, IPT0_010, DOC0_050, DOC0_100, DOC0_250}

  RecommendedSpeed = {
    (mat=Al6061_T6, tool=Endmill_Carbide_0.5, regime=Roughing, sfm=SFM800),
    (mat=Al6061_T6, tool=Endmill_Carbide_0.5, regime=Finish, sfm=SFM1200),
    (mat=Ti6Al4V, tool=Endmill_Carbide_0.5, regime=Roughing, sfm=SFM100),
    (mat=Ti6Al4V, tool=Endmill_Carbide_0.5, regime=Finish, sfm=SFM300),
    (mat=Inconel718, tool=Insert_CNMG, regime=Roughing, sfm=SFM100)
  }

  -- Chip formation observations
  ChipType = {Continuous, Discontinuous, Segmented, WithBUE}
  CuttingProducesChip = {
    (mat=Al6061_T6, tool=Endmill_Carbide_0.5, chipType=Continuous, ctx=StandardConditions),
    (mat=Ti6Al4V, tool=Endmill_Carbide_0.5, chipType=Segmented, ctx=StandardConditions),
    (mat=AISI_4140, tool=Endmill_HSS_0.5, chipType=WithBUE, ctx=LowSpeedConditions)
  }

  -- Wear observations
  WearType = {FlankWear, CraterWear, NotchWear, ThermalCrack}
  WearMechanism = {Abrasion, Adhesion, Diffusion, Oxidation}
  Context = {StandardConditions, LowSpeedConditions, HighTempConditions, WetCutting, DryCutting}
  Time = {T0, T1, T2}

  WearMechanismActive = {
    (mat=Ti6Al4V, tool=Endmill_Carbide_0.5, mechanism=Diffusion, ctx=HighTempConditions),
    (mat=Al6061_T6, tool=Endmill_HSS_0.5, mechanism=Adhesion, ctx=StandardConditions),
    (mat=Inconel718, tool=Insert_CNMG, mechanism=Abrasion, ctx=StandardConditions)
  }

  -- Taylor tool life constants (tacit knowledge from experience)
  TaylorToolLife = {
    (mat=Al6061_T6, tool=Endmill_Carbide_0.5, C=1200, n=0_25),
    (mat=Ti6Al4V, tool=Endmill_Carbide_0.5, C=200, n=0_20),
    (mat=AISI_4140, tool=Endmill_Carbide_0.5, C=400, n=0_22)
  }

  -- GD&T
  Feature = {Bore_1, Face_A, Slot_1, Thread_M8}
  ToleranceType = {Position, Flatness, Perpendicularity, Cylindricity, Concentricity}
  Datum = {A, B, C}
  Bool = {True, False}

