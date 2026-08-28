-- Accepted baseline for the regulated pharmaceutical shipment scenario.
--
-- The finite seed models one releasable shipment and one blocked shipment. The
-- schema keeps shipments, batches, certificates, release decisions, and
-- temperature evidence as relation objects with explicit context/time fibers.

module RegulatedShipment

schema RegulatedShipment:
  object Context
  object Time
  object Shipment
  object Batch
  object CertificateOfAnalysis
  object ReleaseAuthorization
  object QualityReviewer
  object TemperatureRecord
  object TemperatureBand
  object Carrier
  object Lane

  relation ShipmentContainsBatch(shipment: Shipment @data, batch: Batch @data, ctx: Context @context, time: Time @temporal)
  relation BatchHasCertificate(batch: Batch @data, certificate: CertificateOfAnalysis @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentHasCertificate(shipment: Shipment @data, certificate: CertificateOfAnalysis @data, ctx: Context @context, time: Time @temporal)
  relation BatchReleasedBy(batch: Batch @data, release: ReleaseAuthorization @data, reviewer: QualityReviewer @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentReleasedBy(shipment: Shipment @data, release: ReleaseAuthorization @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentHasTemperatureRecord(shipment: Shipment @data, record: TemperatureRecord @data, band: TemperatureBand @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentUsesLane(shipment: Shipment @data, lane: Lane @data, carrier: Carrier @data, ctx: Context @context, time: Time @temporal)

theory RegulatedShipmentRules on RegulatedShipment:
  constraint key ShipmentContainsBatch(shipment, batch, ctx, time)
  constraint key BatchHasCertificate(batch, certificate, ctx, time)
  constraint key ShipmentHasCertificate(shipment, certificate, ctx, time)
  constraint key BatchReleasedBy(batch, release, reviewer, ctx, time)
  constraint key ShipmentReleasedBy(shipment, release, ctx, time)
  constraint key ShipmentHasTemperatureRecord(shipment, record, band, ctx, time)
  constraint key ShipmentUsesLane(shipment, lane, carrier, ctx, time)

  equation shipment_certificate_trace:
    trans(step(shipment, ShipmentContainsBatch, batch), step(batch, BatchHasCertificate, certificate)) =
    step(shipment, ShipmentHasCertificate, certificate)

  rewrite contained_batch_certificate_to_shipment_certificate:
    orientation: forward
    vars: shipment: Shipment, batch: Batch, certificate: CertificateOfAnalysis
    lhs: trans(step(shipment, ShipmentContainsBatch, batch), step(batch, BatchHasCertificate, certificate))
    rhs: step(shipment, ShipmentHasCertificate, certificate)

instance RegulatedShipmentSeed of RegulatedShipment:
  Context = {Evidence, Reviewed, Released, Blocked}
  Time = {T0, T1, T2}
  Shipment = {Shipment_RX_1007, Shipment_RX_1008}
  Batch = {Batch_RX_42, Batch_RX_43}
  CertificateOfAnalysis = {CoA_RX_42}
  ReleaseAuthorization = {Release_RX_42}
  QualityReviewer = {QA_Lee, QA_Mora}
  TemperatureRecord = {TempLog_RX_1007, TempLog_RX_1008}
  TemperatureBand = {ColdChain_2C_8C, Excursion_Above_8C}
  Carrier = {Carrier_Northstar}
  Lane = {Lane_Boston_Toronto}

  ShipmentContainsBatch = {
    contains_rx42: (shipment=Shipment_RX_1007, batch=Batch_RX_42, ctx=Released, time=T1),
    contains_rx43: (shipment=Shipment_RX_1008, batch=Batch_RX_43, ctx=Blocked, time=T1)
  }

  BatchHasCertificate = {
    certified_rx42: (batch=Batch_RX_42, certificate=CoA_RX_42, ctx=Reviewed, time=T1)
  }

  ShipmentHasCertificate = {
    shipment_certificate_rx42: (shipment=Shipment_RX_1007, certificate=CoA_RX_42, ctx=Released, time=T1)
  }

  BatchReleasedBy = {
    batch_release_rx42: (batch=Batch_RX_42, release=Release_RX_42, reviewer=QA_Lee, ctx=Reviewed, time=T1)
  }

  ShipmentReleasedBy = {
    shipment_release_rx42: (shipment=Shipment_RX_1007, release=Release_RX_42, ctx=Released, time=T1)
  }

  ShipmentHasTemperatureRecord = {
    temp_ok_rx1007: (shipment=Shipment_RX_1007, record=TempLog_RX_1007, band=ColdChain_2C_8C, ctx=Evidence, time=T2),
    temp_bad_rx1008: (shipment=Shipment_RX_1008, record=TempLog_RX_1008, band=Excursion_Above_8C, ctx=Evidence, time=T2)
  }

  ShipmentUsesLane = {
    lane_rx1007: (shipment=Shipment_RX_1007, lane=Lane_Boston_Toronto, carrier=Carrier_Northstar, ctx=Released, time=T1),
    lane_rx1008: (shipment=Shipment_RX_1008, lane=Lane_Boston_Toronto, carrier=Carrier_Northstar, ctx=Blocked, time=T1)
  }
