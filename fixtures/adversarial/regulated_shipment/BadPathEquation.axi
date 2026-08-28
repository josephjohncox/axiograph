module BadRegulatedShipmentPath

schema RegulatedShipment:
  object Shipment
  object Batch
  object CertificateOfAnalysis
  relation ShipmentContainsBatch(shipment: Shipment, batch: Batch)
  relation BatchHasCertificate(batch: Batch, certificate: CertificateOfAnalysis)
  relation ShipmentHasCertificate(shipment: Shipment, certificate: CertificateOfAnalysis)

theory BadPathRules on RegulatedShipment:
  equation mismatched_certificate_trace:
    trans(step(shipment, ShipmentContainsBatch, batch), step(batch, BatchHasCertificate, certificate)) =
    step(batch, BatchHasCertificate, certificate)

instance BadPathSeed of RegulatedShipment:
  Shipment = {Shipment_RX_1007}
  Batch = {Batch_RX_42}
  CertificateOfAnalysis = {CoA_RX_42}
  ShipmentContainsBatch = {(shipment=Shipment_RX_1007, batch=Batch_RX_42)}
  BatchHasCertificate = {(batch=Batch_RX_42, certificate=CoA_RX_42)}
  ShipmentHasCertificate = {(shipment=Shipment_RX_1007, certificate=CoA_RX_42)}
