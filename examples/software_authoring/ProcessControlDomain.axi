-- Pure domain ontology for process-control authorization in a chemical plant.
--
-- Simulator, PLC, HMI, optimizer, ERP, and codegen details live in
-- `process_control_tooling_overlay.json`, not in this `.axi` domain model.

module ProcessControlDomain

schema ProcessControl:
  object Context
  object Time
  object Batch
  object Reactor
  object MaterialLot
  object Certificate
  object SensorReading
  object SimulationRun
  object ControlAction
  object SafetyInterlock
  object PlantProcess
  object BusinessInvariant

  relation BatchUsesMaterial(batch: Batch, lot: MaterialLot, ctx: Context, time: Time)
  relation MaterialHasCertificate(lot: MaterialLot, certificate: Certificate, ctx: Context, time: Time)
  relation BatchHasCertifiedMaterial(batch: Batch, certificate: Certificate, ctx: Context, time: Time)
  relation ReactorHasReading(reactor: Reactor, reading: SensorReading, ctx: Context, time: Time)
  relation SimulationSupportsAction(run: SimulationRun, action: ControlAction, ctx: Context, time: Time)
  relation InterlockAllowsAction(interlock: SafetyInterlock, action: ControlAction, ctx: Context, time: Time)
  relation BatchClearedForCharge(batch: Batch, reactor: Reactor, ctx: Context, time: Time)
  relation ProcessRequiresInvariant(process: PlantProcess, invariant: BusinessInvariant)

theory ProcessControlRules on ProcessControl:
  constraint key BatchUsesMaterial(batch, lot, ctx, time)
  constraint key MaterialHasCertificate(lot, certificate, ctx, time)
  constraint key BatchHasCertifiedMaterial(batch, certificate, ctx, time)
  constraint key ReactorHasReading(reactor, reading, ctx, time)
  constraint key SimulationSupportsAction(run, action, ctx, time)
  constraint key InterlockAllowsAction(interlock, action, ctx, time)
  constraint key BatchClearedForCharge(batch, reactor, ctx, time)
  constraint key ProcessRequiresInvariant(process, invariant)

  -- Certification evidence for a lot can be transported to the batch that uses
  -- that lot. This is the domain fact consumed by ERP/MRP and HMI workflows.
  rewrite lot_certificate_to_batch_certificate:
    orientation: forward
    vars: batch: Batch, lot: MaterialLot, certificate: Certificate
    lhs: trans(step(batch, BatchUsesMaterial, lot), step(lot, MaterialHasCertificate, certificate))
    rhs: step(batch, BatchHasCertifiedMaterial, certificate)

instance ProcessControlSeed of ProcessControl:
  Context = {Accepted, Review, Evidence}
  Time = {T0, T1}

  Batch = {Batch_42}
  Reactor = {Reactor_A}
  MaterialLot = {Lot_17}
  Certificate = {Cert_17}
  SensorReading = {Reading_Reactor_A_Normal}
  SimulationRun = {Sim_Charge_Window_42}
  ControlAction = {OpenChargeValve}
  SafetyInterlock = {ChargeValveInterlock}
  PlantProcess = {AuthorizeCharge, ReleaseBatch}
  BusinessInvariant = {CertifiedMaterialRequired, SafeChargeWindowRequired}

  BatchUsesMaterial = {
    (batch=Batch_42, lot=Lot_17, ctx=Accepted, time=T0)
  }

  MaterialHasCertificate = {
    (lot=Lot_17, certificate=Cert_17, ctx=Accepted, time=T0)
  }

  BatchHasCertifiedMaterial = {
    (batch=Batch_42, certificate=Cert_17, ctx=Accepted, time=T1)
  }

  ReactorHasReading = {
    (reactor=Reactor_A, reading=Reading_Reactor_A_Normal, ctx=Accepted, time=T1)
  }

  SimulationSupportsAction = {
    (run=Sim_Charge_Window_42, action=OpenChargeValve, ctx=Accepted, time=T1)
  }

  InterlockAllowsAction = {
    (interlock=ChargeValveInterlock, action=OpenChargeValve, ctx=Accepted, time=T1)
  }

  BatchClearedForCharge = {
    (batch=Batch_42, reactor=Reactor_A, ctx=Accepted, time=T1)
  }

  ProcessRequiresInvariant = {
    (process=AuthorizeCharge, invariant=CertifiedMaterialRequired),
    (process=AuthorizeCharge, invariant=SafeChargeWindowRequired),
    (process=ReleaseBatch, invariant=CertifiedMaterialRequired)
  }
