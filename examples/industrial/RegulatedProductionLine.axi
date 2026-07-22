-- Regulated production line in business context.
--
-- This is the first canonical industrial co-evolution example for Axiograph.
-- It keeps the scope deliberately small while tying together:
-- - process execution,
-- - ERP/MRP order lineage,
-- - certified incoming material,
-- - inspection and release,
-- - delivery and pricing commitments,
-- - plant-control artifacts such as PLC routines, HMI screens, and SOP sections.
--
-- The point is not to model a whole plant. The point is to make one typed line
-- where ontology, process, business, and operational-control artifacts have to
-- stay aligned under review/promotion.
--
-- Code refs, software coverage policy, and codegen plans are intentionally
-- outside this domain representation and live in behavior/tooling overlays.

module RegulatedProductionLine

schema RegulatedLine:
  object Context
  object Time

  object Customer
  object Product
  object SalesOrder
  object WorkOrder
  object Recipe
  object ProcessStep
  object Machine
  object PLCRoutine
  object HMIScreen
  object SOPSection
  object Supplier
  object MaterialLot
  object SupplierCertificate
  object Inspection
  object ReleaseDecision
  object Shipment
  object PriceTerm
  object DeliveryWindow

  relation SalesOrderForProduct(order: SalesOrder, product: Product, customer: Customer, ctx: Context @context, time: Time @temporal)
  relation WorkOrderForSalesOrder(work_order: WorkOrder, order: SalesOrder, recipe: Recipe, ctx: Context @context, time: Time @temporal)
  relation RecipeStep(recipe: Recipe, step: ProcessStep, sequence: Time)
  relation StepRunsOn(step: ProcessStep, machine: Machine)
  relation StepControlledBy(step: ProcessStep, plc: PLCRoutine)
  relation StepDisplayedOn(step: ProcessStep, hmi: HMIScreen)
  relation StepGovernedBy(step: ProcessStep, sop: SOPSection)
  relation LotSuppliedBy(lot: MaterialLot, supplier: Supplier)
  relation LotHasCertificate(lot: MaterialLot, cert: SupplierCertificate)
  relation WorkOrderConsumesLot(work_order: WorkOrder, lot: MaterialLot, ctx: Context @context, time: Time @temporal)
  relation InspectionForWorkOrder(inspection: Inspection, work_order: WorkOrder, step: ProcessStep, ctx: Context @context, time: Time @temporal)
  relation InspectionDecision(inspection: Inspection, decision: ReleaseDecision, ctx: Context @context, time: Time @temporal)
  relation ShipmentFulfills(shipment: Shipment, order: SalesOrder, work_order: WorkOrder, ctx: Context @context, time: Time @temporal)
  relation DeliveryCommitment(order: SalesOrder, window: DeliveryWindow, price: PriceTerm, ctx: Context @context, time: Time @temporal)

theory RegulatedLineRules on RegulatedLine:
  constraint key WorkOrderForSalesOrder(work_order, order, ctx, time)
  constraint key LotHasCertificate(lot, cert)
  constraint key InspectionForWorkOrder(inspection, work_order, step, ctx, time)
  constraint key ShipmentFulfills(shipment, order, work_order, ctx, time)
  constraint key DeliveryCommitment(order, window, price, ctx, time)

instance RegulatedLineSeed of RegulatedLine:
  Context = {Plan, Released, Observed, Audit}
  Time = {Seq_10, Seq_20, T0, T1}

  Customer = {Customer_Alpha}
  Product = {SolventBlend_A}
  SalesOrder = {SalesOrder_1001}
  WorkOrder = {WorkOrder_5001}
  Recipe = {Recipe_Blend_A}
  ProcessStep = {ChargeReactor, ReleaseBatch}
  Machine = {BlendTrain_01}
  PLCRoutine = {PLC_ChargeSequence}
  HMIScreen = {HMI_BlendOverview}
  SOPSection = {SOP_BlendRelease}
  Supplier = {Supplier_Prime}
  MaterialLot = {Lot_MB_042}
  SupplierCertificate = {Cert_MB_042}
  Inspection = {Inspection_9001}
  ReleaseDecision = {ReleasedForShipment}
  Shipment = {Shipment_7001}
  PriceTerm = {Price_USD_4200}
  DeliveryWindow = {DeliveryWeek_42}

  SalesOrderForProduct = {
    (order=SalesOrder_1001, product=SolventBlend_A, customer=Customer_Alpha, ctx=Plan, time=T0)
  }

  WorkOrderForSalesOrder = {
    (work_order=WorkOrder_5001, order=SalesOrder_1001, recipe=Recipe_Blend_A, ctx=Plan, time=T0),
    (work_order=WorkOrder_5001, order=SalesOrder_1001, recipe=Recipe_Blend_A, ctx=Released, time=T1)
  }

  RecipeStep = {
    (recipe=Recipe_Blend_A, step=ChargeReactor, sequence=Seq_10),
    (recipe=Recipe_Blend_A, step=ReleaseBatch, sequence=Seq_20)
  }

  StepRunsOn = {
    (step=ChargeReactor, machine=BlendTrain_01),
    (step=ReleaseBatch, machine=BlendTrain_01)
  }

  StepControlledBy = {
    (step=ChargeReactor, plc=PLC_ChargeSequence),
    (step=ReleaseBatch, plc=PLC_ChargeSequence)
  }

  StepDisplayedOn = {
    (step=ChargeReactor, hmi=HMI_BlendOverview),
    (step=ReleaseBatch, hmi=HMI_BlendOverview)
  }

  StepGovernedBy = {
    (step=ChargeReactor, sop=SOP_BlendRelease),
    (step=ReleaseBatch, sop=SOP_BlendRelease)
  }

  LotSuppliedBy = {
    (lot=Lot_MB_042, supplier=Supplier_Prime)
  }

  LotHasCertificate = {
    (lot=Lot_MB_042, cert=Cert_MB_042)
  }

  WorkOrderConsumesLot = {
    (work_order=WorkOrder_5001, lot=Lot_MB_042, ctx=Released, time=T1)
  }

  InspectionForWorkOrder = {
    (inspection=Inspection_9001, work_order=WorkOrder_5001, step=ReleaseBatch, ctx=Observed, time=T1)
  }

  InspectionDecision = {
    (inspection=Inspection_9001, decision=ReleasedForShipment, ctx=Audit, time=T1)
  }

  ShipmentFulfills = {
    (shipment=Shipment_7001, order=SalesOrder_1001, work_order=WorkOrder_5001, ctx=Released, time=T1)
  }

  DeliveryCommitment = {
    (order=SalesOrder_1001, window=DeliveryWeek_42, price=Price_USD_4200, ctx=Plan, time=T0)
  }
