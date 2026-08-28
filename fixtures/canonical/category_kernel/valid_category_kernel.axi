module CategoryKernelValid

schema Logistics:
  object Shipment
  object Batch
  object Certificate
  object Context
  object Person
  object Reviewer
  subtype Reviewer < Person as reviewer_to_person
  relation Contains(shipment: Shipment @data, batch: Batch @data, ctx: Context @context)
  relation Certification(batch: Batch @data, certificate: Certificate @evidence, ctx: Context @context)
  relation Dispatch(ctx: Context @context, containment: indexed(relation(Contains); ctx) @evidence, reviewer: refined(Reviewer; enum(Alice|Bob)) @data)
  function shipment_to_batch: Shipment -> Batch
  function batch_to_certificate: Batch -> Certificate
  function shipment_to_certificate: Shipment -> Certificate

theory RouteLaws on Logistics:
  equation route_factor:
    shipment_to_batch;batch_to_certificate = shipment_to_certificate
