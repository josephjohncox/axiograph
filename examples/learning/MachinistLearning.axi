-- MachinistLearning.axi (axi_schema_v1)
--
-- This module is the canonical “learning + guardrails” example, expressed in
-- the same schema/theory/instance surface syntax as the rest of the corpus.
--
-- Notes:
-- - We intentionally keep *rich* content (rules, long explanations, modal
--   specs) inside the `theory` section as **unknown constraints** (preserved
--   verbatim, imported into PathDB meta-plane, and queryable).
-- - “Values” like numbers and long strings are modeled as named nodes
--   (`Scalar`, `Text`) so we stay within the identifier-only v1 instance
--   surface. The human-readable text is kept adjacent as comments.

module MachinistLearning

schema MachiningLearning:
  -- ==========================================================================
  -- Core entities
  -- ==========================================================================
  object Material
  object CuttingTool
  object MachiningOperation

  -- Learning entities
  object Concept
  object SafetyGuideline
  object Example
  object TacitKnowledge

  -- Value nodes (identifier-only v1 surface; see comments in the instance)
  object Text
  object Scalar
  object Confidence

  -- ==========================================================================
  -- Enumerations (as first-class object types)
  -- ==========================================================================
  object Coating
  object ToolMaterial
  object ToolGeometry
  object OperationType
  object DifficultyLevel
  object Severity
  object Outcome

  -- ==========================================================================
  -- Graph relations (edges)
  -- ==========================================================================
  relation hasMaterial(op: MachiningOperation, material: Material)
  relation usesTool(op: MachiningOperation, tool: CuttingTool)
  relation requires(concept: Concept, prereq: Concept)
  relation explains(concept: Concept, guideline: SafetyGuideline)
  relation demonstrates(example: Example, concept: Concept)
  relation suitableFor(tool: CuttingTool, material: Material)
  relation causes(op: MachiningOperation, outcome: Outcome)
  relation prevents(guideline: SafetyGuideline, outcome: Outcome)

  -- ==========================================================================
  -- Attribute-ish relations (typed “fields”)
  -- ==========================================================================
  relation hardness(material: Material, value: Scalar)
  relation thermalConductivity(material: Material, value: Scalar)
  relation machinabilityRating(material: Material, value: Scalar)

  relation coating(tool: CuttingTool, value: Coating)
  relation toolMaterial(tool: CuttingTool, value: ToolMaterial)
  relation toolGeometry(tool: CuttingTool, value: ToolGeometry)

  relation operationType(op: MachiningOperation, value: OperationType)
  relation cuttingSpeed(op: MachiningOperation, value: Scalar)
  relation feedRate(op: MachiningOperation, value: Scalar)
  relation depthOfCut(op: MachiningOperation, value: Scalar)

  relation conceptDifficulty(concept: Concept, value: DifficultyLevel)
  relation conceptDescription(concept: Concept, text: Text)

  relation guidelineSeverity(guideline: SafetyGuideline, value: Severity)
  relation guidelineExplanation(guideline: SafetyGuideline, text: Text)
  relation guidelineVisualExample(guideline: SafetyGuideline, text: Text)

  relation exampleDescription(example: Example, text: Text)
  relation exampleMaterial(example: Example, material: Material)
  relation exampleOperation(example: Example, op: MachiningOperation)
  relation exampleOutcome(example: Example, outcome: Outcome)

  relation tacitRule(tacit: TacitKnowledge, text: Text)
  relation tacitConfidence(tacit: TacitKnowledge, value: Confidence)
  relation tacitSource(tacit: TacitKnowledge, text: Text)

  relation confidenceValue(confidence: Confidence, value: Scalar)
  relation scalarValue(scalar: Scalar, text: Text)

theory MachiningLearningContent on MachiningLearning:
  -- ==========================================================================
  -- Guardrail rules (preserved as “unknown constraints”)
  -- ==========================================================================

  constraint MustHaveMaterial:
    forall op : MachiningOperation .
      exists m : Material . hasMaterial(op, m)
    message: "Every operation must specify workpiece material"
    severity: Critical

  constraint TitaniumSpeedLimit:
    forall op : MachiningOperation, m : Material .
      hasMaterial(op, m) && isTitanium(m) ->
        op.cuttingSpeed <= 60.0
    message: "Titanium cutting speed must not exceed 60 m/min"
    severity: Critical
    explains: TitaniumSpeed

  constraint DeepHoleRequiresCoolant:
    forall op : MachiningOperation .
      isDrilling(op) && op.depthOfCut > 3.0 * holeDiameter(op) ->
        hasThroughCoolant(op)
    message: "Deep holes (>3xD) require through-spindle coolant"
    severity: Warning
    explains: DeepHoleCoolant

  -- ==========================================================================
  -- Query patterns (stored as unknown constraints for now)
  -- ==========================================================================

  constraint query_titaniumPrerequisites:
    -- "What should I know before machining titanium?"
    titaniumPrerequisites =
      FollowPath(Titanium, [relatedTo, requires*])

  constraint query_titaniumExamples:
    -- "Show me examples of titanium machining"
    titaniumExamples =
      FindByRelation(demonstrates, TitaniumConcepts)
      `And` FindByType(Example)

  constraint query_titaniumRisks:
    -- "What could go wrong if I use high speed on titanium?"
    titaniumRisks =
      FollowPath(HighSpeedTitanium, [causes])
      `And` ProbabilisticQuery(_, minConfidence=0.8)

  constraint query_applicableGuidelines:
    -- "What safety guidelines apply to my operation?"
    applicableGuidelines(op) =
      FollowPath(op, [hasMaterial, relatedTo*, explains])
      `And` FindByType(SafetyGuideline)

  -- ==========================================================================
  -- Modal logic (deontic + epistemic) — preserved for future execution
  -- ==========================================================================

  constraint modal_deontic_MachiningObligation:
    modal deontic MachiningObligation {
      Obligatory(hasMaterial(op, Ti) -> op.cuttingSpeed <= 60)
      Permitted(hasMaterial(op, Al) && usesTool(op, carbide))
      Forbidden(isDrilling(op) && isDeep(op) && not(hasCoolant(op)))
    }

  constraint modal_epistemic_KnowledgeModel:
    modal epistemic KnowledgeModel {
      agents = [Novice, Journeyman, Expert]

      Knows(Novice, alwaysWearSafetyGlasses)
      Knows(Novice, neverOverrideInterlocks)

      Knows(Journeyman, titaniumRequiresSlowSpeed)
      Knows(Journeyman, aluminumNeedsCoolantForFinish)

      Knows(Expert, soundOfOptimalCut)
      Knows(Expert, feelOfToolWear)

      gap NoviceNeedsToLearn =
        Knows(Journeyman, x) && not(Knows(Novice, x))
    }

instance MachinistLearningExample of MachiningLearning:
  -- ==========================================================================
  -- Enumerations
  -- ==========================================================================

  Coating = { TiN, TiAlN, AlCrN, DLC, Uncoated }
  ToolMaterial = { HSS, Carbide, Ceramic, CBN, Diamond }
  ToolGeometry = { Positive, Neutral, Negative }
  OperationType = { Turning, Milling, Drilling, Boring, Tapping }
  DifficultyLevel = { Beginner, Intermediate, Advanced, Expert }
  Severity = { Info, Advisory, Warning, Critical, Blocking }
  Outcome = { Success, ToolWear, Chatter, BuiltUpEdge, Fracture }

  -- ==========================================================================
  -- Domain entities (materials, tools, operations)
  -- ==========================================================================

  Material = { Ti6Al4V, Aluminum_6061 }
  CuttingTool = { Carbide_Endmill }
  MachiningOperation = { Op_TitaniumSuccess, Op_TitaniumFailure }

  -- “Value” nodes (kept as identifiers; human text is in comments)
  Text = {
    -- Names / titles
    Text_ThermalConductivityInMachining,
    Text_WorkHardening,
    Text_ChatterAndVibration,
    Text_TitaniumCuttingSpeedLimits,
    Text_DeepHoleRequiresThroughCoolant,
    Text_ThinWallMachiningStrategy,
    Text_SuccessfulTitaniumRoughing,
    Text_ToolFailureDueToExcessiveSpeed,

    -- Tacit rule titles + sources + rule bodies (see comments below)
    Text_Tacit_TitaniumRequiresSlowSpeeds_title,
    Text_Tacit_TitaniumRequiresSlowSpeeds_rule,
    Text_Tacit_TitaniumRequiresSlowSpeeds_source,
    Text_Tacit_CarbideForHardMaterials_title,
    Text_Tacit_CarbideForHardMaterials_rule,
    Text_Tacit_CarbideForHardMaterials_source,
    Text_Tacit_CoolantPreventsBuiltUpEdge_title,
    Text_Tacit_CoolantPreventsBuiltUpEdge_rule,
    Text_Tacit_CoolantPreventsBuiltUpEdge_source,

    -- Concept descriptions (full prose kept here in comments; identifiers are stable)
    Text_Desc_ThermalConductivity,
    Text_Desc_WorkHardening,
    Text_Desc_ChatterVibration,

    -- Guideline explanations (full prose kept here in comments; identifiers are stable)
    Text_Explain_TitaniumSpeed,
    Text_Explain_DeepHoleCoolant,
    Text_Explain_ThinWallChatter
  }

  Scalar = {
    -- Material properties
    Scalar_Ti6Al4V_hardness_45_0,          -- 45.0 (Rockwell, approx)
    Scalar_Ti6Al4V_thermalK_7_1,           -- 7.1 W/m·K
    Scalar_Ti6Al4V_machinability_30,       -- 30/100
    Scalar_Al6061_hardness_60_0,           -- example placeholder
    Scalar_Al6061_thermalK_205_0,          -- 205 W/m·K
    Scalar_Al6061_machinability_80,        -- 80/100

    -- Operation parameters
    Scalar_Speed_45_0,                     -- m/min
    Scalar_Speed_120_0,                    -- m/min (too fast)
    Scalar_Feed_0_15,                      -- mm/rev
    Scalar_Feed_0_10,                      -- mm/rev
    Scalar_Depth_2_0,                      -- mm
    Scalar_Depth_3_0                       -- mm
  }

  Confidence = { Conf_0_95, Conf_0_88, Conf_0_92 }

  scalarValue = {
    (scalar=Scalar_Ti6Al4V_hardness_45_0, text=Text_scalar_45_0),
    (scalar=Scalar_Ti6Al4V_thermalK_7_1, text=Text_scalar_7_1),
    (scalar=Scalar_Ti6Al4V_machinability_30, text=Text_scalar_30),
    (scalar=Scalar_Al6061_hardness_60_0, text=Text_scalar_60_0),
    (scalar=Scalar_Al6061_thermalK_205_0, text=Text_scalar_205_0),
    (scalar=Scalar_Al6061_machinability_80, text=Text_scalar_80),
    (scalar=Scalar_Speed_45_0, text=Text_scalar_45_0),
    (scalar=Scalar_Speed_120_0, text=Text_scalar_120_0),
    (scalar=Scalar_Feed_0_15, text=Text_scalar_0_15),
    (scalar=Scalar_Feed_0_10, text=Text_scalar_0_10),
    (scalar=Scalar_Depth_2_0, text=Text_scalar_2_0),
    (scalar=Scalar_Depth_3_0, text=Text_scalar_3_0)
  }

  Text = {
    Text_scalar_45_0,
    Text_scalar_7_1,
    Text_scalar_30,
    Text_scalar_60_0,
    Text_scalar_205_0,
    Text_scalar_80,
    Text_scalar_120_0,
    Text_scalar_0_15,
    Text_scalar_0_10,
    Text_scalar_2_0,
    Text_scalar_3_0
  }

  confidenceValue = {
    (confidence=Conf_0_95, value=Scalar_conf_0_95),
    (confidence=Conf_0_88, value=Scalar_conf_0_88),
    (confidence=Conf_0_92, value=Scalar_conf_0_92)
  }

  Scalar = { Scalar_conf_0_95, Scalar_conf_0_88, Scalar_conf_0_92 }
  scalarValue = {
    (scalar=Scalar_conf_0_95, text=Text_scalar_0_95),
    (scalar=Scalar_conf_0_88, text=Text_scalar_0_88),
    (scalar=Scalar_conf_0_92, text=Text_scalar_0_92)
  }
  Text = { Text_scalar_0_95, Text_scalar_0_88, Text_scalar_0_92 }

  -- Material attribute relations
  hardness = {
    (material=Ti6Al4V, value=Scalar_Ti6Al4V_hardness_45_0),
    (material=Aluminum_6061, value=Scalar_Al6061_hardness_60_0)
  }
  thermalConductivity = {
    (material=Ti6Al4V, value=Scalar_Ti6Al4V_thermalK_7_1),
    (material=Aluminum_6061, value=Scalar_Al6061_thermalK_205_0)
  }
  machinabilityRating = {
    (material=Ti6Al4V, value=Scalar_Ti6Al4V_machinability_30),
    (material=Aluminum_6061, value=Scalar_Al6061_machinability_80)
  }

  -- Tool attribute relations (minimal)
  coating = { (tool=Carbide_Endmill, value=Uncoated) }
  toolMaterial = { (tool=Carbide_Endmill, value=Carbide) }
  toolGeometry = { (tool=Carbide_Endmill, value=Positive) }

  -- Operation attribute relations
  operationType = {
    (op=Op_TitaniumSuccess, value=Milling),
    (op=Op_TitaniumFailure, value=Milling)
  }
  cuttingSpeed = {
    (op=Op_TitaniumSuccess, value=Scalar_Speed_45_0),
    (op=Op_TitaniumFailure, value=Scalar_Speed_120_0)
  }
  feedRate = {
    (op=Op_TitaniumSuccess, value=Scalar_Feed_0_15),
    (op=Op_TitaniumFailure, value=Scalar_Feed_0_10)
  }
  depthOfCut = {
    (op=Op_TitaniumSuccess, value=Scalar_Depth_2_0),
    (op=Op_TitaniumFailure, value=Scalar_Depth_3_0)
  }

  -- ==========================================================================
  -- Learning content
  -- ==========================================================================

  Concept = { ThermalConductivity, WorkHardening, ChatterVibration }
  conceptDifficulty = {
    (concept=ThermalConductivity, value=Beginner),
    (concept=WorkHardening, value=Intermediate),
    (concept=ChatterVibration, value=Advanced)
  }
  requires = {
    (concept=WorkHardening, prereq=ThermalConductivity),
    (concept=ChatterVibration, prereq=WorkHardening)
  }
  conceptDescription = {
    (concept=ThermalConductivity, text=Text_Desc_ThermalConductivity),
    (concept=WorkHardening, text=Text_Desc_WorkHardening),
    (concept=ChatterVibration, text=Text_Desc_ChatterVibration)
  }

  -- Concept descriptions (human-readable prose)
  -- Text_Desc_ThermalConductivity:
  --   Thermal conductivity determines how heat flows from the cutting zone.
  --   Low conductivity materials (like titanium) concentrate heat at the tool tip,
  --   causing rapid wear. High conductivity materials (like aluminum) dissipate
  --   heat quickly but may require different cutting strategies.
  --
  -- Text_Desc_WorkHardening:
  --   Some materials harden when deformed. If you let a tool "rub" instead of
  --   "cut" (too light feed), the surface hardens before the next pass, making
  --   cutting even harder. This creates a vicious cycle of tool wear.
  --
  -- Text_Desc_ChatterVibration:
  --   Chatter is self-excited vibration between tool and workpiece. It leaves
  --   visible marks, damages surface finish, and can break tools.

  SafetyGuideline = { TitaniumSpeed, DeepHoleCoolant, ThinWallChatter }
  guidelineSeverity = {
    (guideline=TitaniumSpeed, value=Critical),
    (guideline=DeepHoleCoolant, value=Warning),
    (guideline=ThinWallChatter, value=Advisory)
  }
  guidelineExplanation = {
    (guideline=TitaniumSpeed, text=Text_Explain_TitaniumSpeed),
    (guideline=DeepHoleCoolant, text=Text_Explain_DeepHoleCoolant),
    (guideline=ThinWallChatter, text=Text_Explain_ThinWallChatter)
  }
  guidelineVisualExample = {
    (guideline=TitaniumSpeed, text=Text_titanium_heat_zones_png)
  }
  Text = { Text_titanium_heat_zones_png }

  -- Guideline explanations (human-readable prose)
  -- Text_Explain_TitaniumSpeed: (see original learning example for full prose)
  -- Text_Explain_DeepHoleCoolant: (see original learning example for full prose)
  -- Text_Explain_ThinWallChatter: (see original learning example for full prose)

  Example = { TitaniumSuccess, TitaniumFailure }
  exampleDescription = {
    (example=TitaniumSuccess, text=Text_SuccessfulTitaniumRoughing),
    (example=TitaniumFailure, text=Text_ToolFailureDueToExcessiveSpeed)
  }
  exampleMaterial = {
    (example=TitaniumSuccess, material=Ti6Al4V),
    (example=TitaniumFailure, material=Ti6Al4V)
  }
  exampleOperation = {
    (example=TitaniumSuccess, op=Op_TitaniumSuccess),
    (example=TitaniumFailure, op=Op_TitaniumFailure)
  }
  exampleOutcome = {
    (example=TitaniumSuccess, outcome=Success),
    (example=TitaniumFailure, outcome=ToolWear)
  }

  -- ==========================================================================
  -- Tacit knowledge (probabilistic rules as first-class nodes)
  -- ==========================================================================

  TacitKnowledge = {
    Tacit_TitaniumRequiresSlowSpeeds,
    Tacit_CarbideForHardMaterials,
    Tacit_CoolantPreventsBuiltUpEdge
  }
  tacitRule = {
    (tacit=Tacit_TitaniumRequiresSlowSpeeds, text=Text_Tacit_TitaniumRequiresSlowSpeeds_rule),
    (tacit=Tacit_CarbideForHardMaterials, text=Text_Tacit_CarbideForHardMaterials_rule),
    (tacit=Tacit_CoolantPreventsBuiltUpEdge, text=Text_Tacit_CoolantPreventsBuiltUpEdge_rule)
  }
  tacitConfidence = {
    (tacit=Tacit_TitaniumRequiresSlowSpeeds, value=Conf_0_95),
    (tacit=Tacit_CarbideForHardMaterials, value=Conf_0_88),
    (tacit=Tacit_CoolantPreventsBuiltUpEdge, value=Conf_0_92)
  }
  tacitSource = {
    (tacit=Tacit_TitaniumRequiresSlowSpeeds, text=Text_Tacit_TitaniumRequiresSlowSpeeds_source),
    (tacit=Tacit_CarbideForHardMaterials, text=Text_Tacit_CarbideForHardMaterials_source),
    (tacit=Tacit_CoolantPreventsBuiltUpEdge, text=Text_Tacit_CoolantPreventsBuiltUpEdge_source)
  }

  -- Tacit rule bodies (kept verbatim as comments; stable identifiers above)
  --
  -- Text_Tacit_TitaniumRequiresSlowSpeeds_rule:
  --   hasMaterial(op, m) && isTitanium(m) -> preferLowSpeed(op)
  -- Text_Tacit_TitaniumRequiresSlowSpeeds_source:
  --   Machinist's Handbook, 30+ years experience
  --
  -- Text_Tacit_CarbideForHardMaterials_rule:
  --   hardness(m) > 45 -> recommendCarbide(op)
  -- Text_Tacit_CarbideForHardMaterials_source:
  --   Tool manufacturer recommendations
  --
  -- Text_Tacit_CoolantPreventsBuiltUpEdge_rule:
  --   floodCoolant(op) && isAluminum(m) -> reduces(builtUpEdge, 0.8)
  -- Text_Tacit_CoolantPreventsBuiltUpEdge_source:
  --   Industry best practice
