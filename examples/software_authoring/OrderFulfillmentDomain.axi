-- Pure domain ontology for order fulfillment.
--
-- DDD/fDDD context maps, software coverage targets, code refs, codegen plans,
-- coverage policy, and continuous checks intentionally live in
-- `order_fulfillment_tooling_overlay.json`, not in this `.axi` representation.

module OrderFulfillmentDomain

schema OrderFulfillment:
  object Context
  object Time
  object Order
  object CreditReservation
  object Payment
  object Shipment
  object FulfillmentProcess
  object FulfillmentPolicy
  object BusinessInvariant

  relation OrderHasReservation(order: Order, reservation: CreditReservation, ctx: Context @context, time: Time @temporal)
  relation ReservationApprovesPayment(reservation: CreditReservation, payment: Payment, ctx: Context @context, time: Time @temporal)
  relation OrderHasPayment(order: Order, payment: Payment, ctx: Context @context, time: Time @temporal)
  relation PaymentAuthorizesShipment(payment: Payment, shipment: Shipment, ctx: Context @context, time: Time @temporal)
  relation OrderEligibleForShipment(order: Order, shipment: Shipment, ctx: Context @context, time: Time @temporal)
  relation ProcessRequiresInvariant(process: FulfillmentProcess, invariant: BusinessInvariant)
  relation PolicyRequiresInvariant(policy: FulfillmentPolicy, invariant: BusinessInvariant)
  relation ReservationSatisfiesInvariant(reservation: CreditReservation, invariant: BusinessInvariant, ctx: Context @context, time: Time @temporal)

theory OrderFulfillmentRules on OrderFulfillment:
  constraint key OrderHasReservation(order, reservation, ctx, time)
  constraint key ReservationApprovesPayment(reservation, payment, ctx, time)
  constraint key OrderHasPayment(order, payment, ctx, time)
  constraint key PaymentAuthorizesShipment(payment, shipment, ctx, time)
  constraint key OrderEligibleForShipment(order, shipment, ctx, time)
  constraint key ProcessRequiresInvariant(process, invariant)
  constraint key PolicyRequiresInvariant(policy, invariant)
  constraint key ReservationSatisfiesInvariant(reservation, invariant, ctx, time)

  -- A paid order is eligible for shipment when payment authorizes the shipment.
  equation paid_order_authorizes_shipment:
    trans(step(order, OrderHasPayment, payment), step(payment, PaymentAuthorizesShipment, shipment)) =
    step(order, OrderEligibleForShipment, shipment)

  -- A reservation that approves a payment can be normalized into the accepted
  -- payment fact used by downstream checkout and shipment behavior cases.
  rewrite reservation_to_payment:
    orientation: forward
    vars: order: Order, reservation: CreditReservation, payment: Payment
    lhs: trans(step(order, OrderHasReservation, reservation), step(reservation, ReservationApprovesPayment, payment))
    rhs: step(order, OrderHasPayment, payment)

instance OrderFulfillmentSeed of OrderFulfillment:
  Context = {Accepted, Review, Evidence}
  Time = {T0, T1}

  Order = {Order_1001}
  CreditReservation = {Reservation_9001}
  Payment = {Payment_9001}
  Shipment = {Shipment_7001}
  FulfillmentProcess = {ReserveCredit, CheckoutShipmentEligibility}
  FulfillmentPolicy = {ReservedPaymentBeforeShipment, PaidOrdersCanShip}
  BusinessInvariant = {PaymentReservedBeforeShipment, ShipmentRequiresAuthorizedPayment}

  OrderHasReservation = {
    (order=Order_1001, reservation=Reservation_9001, ctx=Accepted, time=T0)
  }

  ReservationApprovesPayment = {
    (reservation=Reservation_9001, payment=Payment_9001, ctx=Accepted, time=T0)
  }

  OrderHasPayment = {
    (order=Order_1001, payment=Payment_9001, ctx=Accepted, time=T1)
  }

  PaymentAuthorizesShipment = {
    (payment=Payment_9001, shipment=Shipment_7001, ctx=Accepted, time=T1)
  }

  OrderEligibleForShipment = {
    (order=Order_1001, shipment=Shipment_7001, ctx=Accepted, time=T1)
  }

  ProcessRequiresInvariant = {
    (process=ReserveCredit, invariant=PaymentReservedBeforeShipment),
    (process=CheckoutShipmentEligibility, invariant=ShipmentRequiresAuthorizedPayment)
  }

  PolicyRequiresInvariant = {
    (policy=ReservedPaymentBeforeShipment, invariant=PaymentReservedBeforeShipment),
    (policy=PaidOrdersCanShip, invariant=ShipmentRequiresAuthorizedPayment)
  }

  ReservationSatisfiesInvariant = {
    (reservation=Reservation_9001, invariant=PaymentReservedBeforeShipment, ctx=Accepted, time=T0)
  }
