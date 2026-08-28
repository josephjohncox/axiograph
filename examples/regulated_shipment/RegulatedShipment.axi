-- Canonical regulated pharmaceutical shipment scenario.
--
-- This evolves the accepted baseline with explicit customs review and a
-- dependent dispatch review whose payload cites a context-indexed
-- ShipmentContainsBatch fact. It remains a finite teaching fixture, not a
-- complete pharmaceutical, customs, or cold-chain ontology.

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
  object CustomsDeclaration
  object Jurisdiction
  object DispatchDecision

  relation ShipmentContainsBatch(shipment: Shipment @data, batch: Batch @data, ctx: Context @context, time: Time @temporal)
  relation BatchHasCertificate(batch: Batch @data, certificate: CertificateOfAnalysis @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentHasCertificate(shipment: Shipment @data, certificate: CertificateOfAnalysis @data, ctx: Context @context, time: Time @temporal)
  relation BatchReleasedBy(batch: Batch @data, release: ReleaseAuthorization @data, reviewer: QualityReviewer @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentReleasedBy(shipment: Shipment @data, release: ReleaseAuthorization @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentHasTemperatureRecord(shipment: Shipment @data, record: TemperatureRecord @data, band: TemperatureBand @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentUsesLane(shipment: Shipment @data, lane: Lane @data, carrier: Carrier @data, ctx: Context @context, time: Time @temporal)
  relation ShipmentDeclaredIn(shipment: Shipment @data, declaration: CustomsDeclaration @data, jurisdiction: Jurisdiction @data, ctx: Context @context, time: Time @temporal)
  relation DispatchReview(ctx: Context @context, contained_batch: indexed(relation(ShipmentContainsBatch); ctx) @data, reviewer: refined(QualityReviewer; enum(QA_Lee|QA_Mora)) @data, decision: DispatchDecision @data)

  function lane_carrier: Lane -> Carrier
  function carrier_jurisdiction: Carrier -> Jurisdiction
  function lane_jurisdiction: Lane -> Jurisdiction

theory RegulatedShipmentRules on RegulatedShipment:
  constraint key ShipmentContainsBatch(shipment, batch, ctx, time)
  constraint key BatchHasCertificate(batch, certificate, ctx, time)
  constraint key ShipmentHasCertificate(shipment, certificate, ctx, time)
  constraint key BatchReleasedBy(batch, release, reviewer, ctx, time)
  constraint key ShipmentReleasedBy(shipment, release, ctx, time)
  constraint key ShipmentHasTemperatureRecord(shipment, record, band, ctx, time)
  constraint key ShipmentUsesLane(shipment, lane, carrier, ctx, time)
  constraint key ShipmentDeclaredIn(shipment, declaration, jurisdiction, ctx, time)
  constraint key DispatchReview(ctx, contained_batch)

  equation lane_jurisdiction_factorization:
    lane_carrier;carrier_jurisdiction =
    lane_jurisdiction

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
  CustomsDeclaration = {Declaration_CA_RX_1007}
  Jurisdiction = {Canada}
  DispatchDecision = {ApprovedForDispatch, HoldForInvestigation}

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

  ShipmentDeclaredIn = {
    declaration_rx1007: (shipment=Shipment_RX_1007, declaration=Declaration_CA_RX_1007, jurisdiction=Canada, ctx=Reviewed, time=T1)
  }

  DispatchReview = {
    (ctx=Released, contained_batch=contains_rx42, reviewer=QA_Lee, decision=ApprovedForDispatch),
    (ctx=Blocked, contained_batch=contains_rx43, reviewer=QA_Mora, decision=HoldForInvestigation)
  }

  lane_carrier = {(source=Lane_Boston_Toronto, target=Carrier_Northstar)}
  carrier_jurisdiction = {(source=Carrier_Northstar, target=Canada)}
  lane_jurisdiction = {(source=Lane_Boston_Toronto, target=Canada)}
