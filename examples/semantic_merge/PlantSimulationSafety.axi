-- Simulation/safety review slice for chemical-plant operations.
--
-- This branch adds process-model and probabilistic-risk obligations. It is
-- intentionally independent from the procurement/certification branch at the
-- module level, but semantically connected through shared plant concepts.

module PlantSimulationSafety

schema PlantSimulation:
  object Context
  object Time

  object Reactor
  object ProductionRun
  object SimulationScenario
  object PhysicsModel
  object StateEstimate
  object RiskNode
  object SafetyEnvelope
  object OptimizerRun

  relation ScenarioCoversRun(scenario: SimulationScenario, run: ProductionRun, ctx: Context, time: Time)
  relation ScenarioUsesModel(scenario: SimulationScenario, model: PhysicsModel, ctx: Context, time: Time)
  relation ScenarioEstimatesState(scenario: SimulationScenario, state: StateEstimate, ctx: Context, time: Time)
  relation RiskNodeForState(risk: RiskNode, state: StateEstimate, ctx: Context, time: Time)
  relation SafetyEnvelopeForReactor(envelope: SafetyEnvelope, reactor: Reactor, ctx: Context, time: Time)
  relation OptimizerEvaluatesScenario(optimizer: OptimizerRun, scenario: SimulationScenario, envelope: SafetyEnvelope, ctx: Context, time: Time)

theory SimulationRules on PlantSimulation:
  constraint key ScenarioCoversRun(scenario, run, ctx, time)
  constraint key ScenarioUsesModel(scenario, model, ctx, time)
  constraint key ScenarioEstimatesState(scenario, state, ctx, time)
  constraint key RiskNodeForState(risk, state, ctx, time)
  constraint key SafetyEnvelopeForReactor(envelope, reactor, ctx, time)
  constraint key OptimizerEvaluatesScenario(optimizer, scenario, envelope, ctx, time)

instance SimulationSeed of PlantSimulation:
  Context = {Plan, Simulated, Review}
  Time = {T0, T1}

  Reactor = {R101}
  ProductionRun = {Run_042}
  SimulationScenario = {Startup_R101_Run042}
  PhysicsModel = {MassEnergyBalance_v2}
  StateEstimate = {R101_TemperaturePressureEstimate}
  RiskNode = {OverpressureRisk_R101}
  SafetyEnvelope = {R101_StartupEnvelope}
  OptimizerRun = {Optimizer_StartupSchedule_01}

  ScenarioCoversRun = {
    (scenario=Startup_R101_Run042, run=Run_042, ctx=Simulated, time=T1)
  }

  ScenarioUsesModel = {
    (scenario=Startup_R101_Run042, model=MassEnergyBalance_v2, ctx=Simulated, time=T1)
  }

  ScenarioEstimatesState = {
    (scenario=Startup_R101_Run042, state=R101_TemperaturePressureEstimate, ctx=Simulated, time=T1)
  }

  RiskNodeForState = {
    (risk=OverpressureRisk_R101, state=R101_TemperaturePressureEstimate, ctx=Review, time=T1)
  }

  SafetyEnvelopeForReactor = {
    (envelope=R101_StartupEnvelope, reactor=R101, ctx=Review, time=T1)
  }

  OptimizerEvaluatesScenario = {
    (optimizer=Optimizer_StartupSchedule_01, scenario=Startup_R101_Run042, envelope=R101_StartupEnvelope, ctx=Review, time=T1)
  }
