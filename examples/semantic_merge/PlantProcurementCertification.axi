-- Procurement/certification review slice for chemical-plant operations.
--
-- This branch adds supplier certificate, delivery, and release-document
-- obligations. It should merge cleanly with a simulation branch because it
-- extends a different bounded-context slice over the same accepted base.

module PlantProcurementCertification

schema PlantProcurement:
  object Context
  object Time

  object MaterialLot
  object Supplier
  object SupplierCertificate
  object Delivery
  object ReceivingInspection
  object ReleaseDocument
  object WorkOrder

  relation LotSuppliedBy(lot: MaterialLot, supplier: Supplier)
  relation LotHasCertificate(lot: MaterialLot, cert: SupplierCertificate, ctx: Context, time: Time)
  relation DeliveryContainsLot(delivery: Delivery, lot: MaterialLot, ctx: Context, time: Time)
  relation InspectionForDelivery(inspection: ReceivingInspection, delivery: Delivery, ctx: Context, time: Time)
  relation ReleaseDocumentForLot(doc: ReleaseDocument, lot: MaterialLot, cert: SupplierCertificate, ctx: Context, time: Time)
  relation WorkOrderReleasedBy(order: WorkOrder, doc: ReleaseDocument, ctx: Context, time: Time)

theory ProcurementRules on PlantProcurement:
  constraint key LotHasCertificate(lot, cert, ctx, time)
  constraint key DeliveryContainsLot(delivery, lot, ctx, time)
  constraint key InspectionForDelivery(inspection, delivery, ctx, time)
  constraint key ReleaseDocumentForLot(doc, lot, cert, ctx, time)
  constraint key WorkOrderReleasedBy(order, doc, ctx, time)

instance ProcurementSeed of PlantProcurement:
  Context = {Plan, Released, Observed, Audit}
  Time = {T0, T1}

  MaterialLot = {Lot_A17}
  Supplier = {NorthDockChem}
  SupplierCertificate = {Cert_A17_COA}
  Delivery = {Delivery_778}
  ReceivingInspection = {ReceivingInspection_090}
  ReleaseDocument = {ReleaseDoc_A17}
  WorkOrder = {WO_5001}

  LotSuppliedBy = {
    (lot=Lot_A17, supplier=NorthDockChem)
  }

  LotHasCertificate = {
    (lot=Lot_A17, cert=Cert_A17_COA, ctx=Audit, time=T1)
  }

  DeliveryContainsLot = {
    (delivery=Delivery_778, lot=Lot_A17, ctx=Observed, time=T1)
  }

  InspectionForDelivery = {
    (inspection=ReceivingInspection_090, delivery=Delivery_778, ctx=Audit, time=T1)
  }

  ReleaseDocumentForLot = {
    (doc=ReleaseDoc_A17, lot=Lot_A17, cert=Cert_A17_COA, ctx=Audit, time=T1)
  }

  WorkOrderReleasedBy = {
    (order=WO_5001, doc=ReleaseDoc_A17, ctx=Released, time=T1)
  }
