module BadRegulatedShipmentReviewer

schema RegulatedShipment:
  object Context
  object Shipment
  object Batch
  object QualityReviewer
  object DispatchDecision
  relation ShipmentContainsBatch(shipment: Shipment @data, batch: Batch @data, ctx: Context @context)
  relation DispatchReview(ctx: Context @context, contained_batch: indexed(relation(ShipmentContainsBatch); ctx) @data, reviewer: refined(QualityReviewer; enum(QA_Lee|QA_Mora)) @data, decision: DispatchDecision @data)

instance BadReviewerSeed of RegulatedShipment:
  Context = {Released}
  Shipment = {Shipment_RX_1007}
  Batch = {Batch_RX_42}
  QualityReviewer = {QA_Lee, QA_Mora, QA_Rogue}
  DispatchDecision = {ApprovedForDispatch}
  ShipmentContainsBatch = {
    contains_rx42: (shipment=Shipment_RX_1007, batch=Batch_RX_42, ctx=Released)
  }
  DispatchReview = {
    (ctx=Released, contained_batch=contains_rx42, reviewer=QA_Rogue, decision=ApprovedForDispatch)
  }
