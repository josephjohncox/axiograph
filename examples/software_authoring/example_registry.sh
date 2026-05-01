#!/usr/bin/env bash
# Parser-free registry for the bundled software-authoring shell examples.

software_authoring_example_ids() {
  printf '%s\n' \
    order_fulfillment \
    subscription_billing \
    process_control
}

software_authoring_resolve_example_id() {
  local selector="${1:-order_fulfillment}"
  local basename="${selector##*/}"

  case "${selector}" in
    order_fulfillment|subscription_billing|process_control)
      printf '%s\n' "${selector}"
      return 0
      ;;
  esac

  case "${basename}" in
    *)
      printf 'unknown software-authoring example selector: %s\n' "${selector}" >&2
      return 1
      ;;
  esac
}

software_authoring_example_field() {
  local id="$1"
  local field="$2"

  case "${id}:${field}" in
    order_fulfillment:title)
      printf '%s\n' 'Order fulfillment reserve-credit and shipment eligibility'
      ;;
    order_fulfillment:axi)
      printf '%s\n' 'examples/software_authoring/OrderFulfillmentDomain.axi'
      ;;
    order_fulfillment:overlay)
      printf '%s\n' 'examples/software_authoring/order_fulfillment_tooling_overlay.json'
      ;;
    order_fulfillment:behavior_case)
      printf '%s\n' 'examples/software_authoring/order_fulfillment_behavior_case.json'
      ;;
    order_fulfillment:cq_file)
      printf '%s\n' 'examples/software_authoring/order_fulfillment.cq'
      ;;
    subscription_billing:title)
      printf '%s\n' 'Subscription billing entitlement after paid invoice'
      ;;
    subscription_billing:axi)
      printf '%s\n' 'examples/software_authoring/SubscriptionBillingDomain.axi'
      ;;
    subscription_billing:overlay)
      printf '%s\n' 'examples/software_authoring/subscription_billing_tooling_overlay.json'
      ;;
    subscription_billing:behavior_case)
      printf '%s\n' 'examples/software_authoring/subscription_billing_behavior_case.json'
      ;;
    subscription_billing:cq_file)
      printf '%s\n' 'examples/software_authoring/subscription_billing.cq'
      ;;
    process_control:title)
      printf '%s\n' 'Process-control charge authorization with simulator/HMI/PLC surfaces'
      ;;
    process_control:axi)
      printf '%s\n' 'examples/software_authoring/ProcessControlDomain.axi'
      ;;
    process_control:overlay)
      printf '%s\n' 'examples/software_authoring/process_control_tooling_overlay.json'
      ;;
    process_control:behavior_case)
      printf '%s\n' 'examples/software_authoring/process_control_behavior_case.json'
      ;;
    process_control:cq_file)
      printf '%s\n' 'examples/software_authoring/process_control.cq'
      ;;
    *)
      printf 'unknown software-authoring example field: %s.%s\n' "${id}" "${field}" >&2
      return 1
      ;;
  esac
}

software_authoring_coverage_query_args() {
  case "$1" in
    order_fulfillment)
      printf '%s\n' \
        --term 'reserve credit' \
        --term 'shipment eligibility' \
        --term 'accepted payment' \
        --relation OrderHasReservation \
        --relation ReservationApprovesPayment \
        --relation OrderEligibleForShipment \
        --cq-name reservation_normalizes_to_payment \
        --cq-name accepted_order_is_shipment_eligible \
        --code-ref services/orders/internal/credit/reserve_credit.go \
        --code-ref apps/checkout/src/reserve-credit.ts \
        --code-ref workers/shipping/src/eligibility.rs \
        --surface-hint checkout \
        --surface-hint shipping \
        --max-matches 8
      ;;
    subscription_billing)
      printf '%s\n' \
        --term 'paid invoice' \
        --term 'product access' \
        --term 'account entitlement' \
        --relation InvoicePaidBy \
        --relation PaidInvoiceGrantsEntitlement \
        --relation AccountGrantedEntitlement \
        --cq-name paid_invoice_grants_subscription_entitlement \
        --cq-name account_has_product_access \
        --code-ref services/billing/internal/payments/apply_payment.go \
        --code-ref workers/entitlements/src/grant_entitlement.rs \
        --code-ref apps/billing/src/apply-payment.ts \
        --surface-hint billing \
        --surface-hint entitlement \
        --surface-hint 'product access' \
        --max-matches 8
      ;;
    process_control)
      printf '%s\n' \
        --term 'charge authorization' \
        --term 'material certificate' \
        --term 'safe charge window' \
        --term 'PLC interlock' \
        --term 'HMI release' \
        --relation BatchHasCertifiedMaterial \
        --relation SimulationSupportsAction \
        --relation InterlockAllowsAction \
        --relation BatchClearedForCharge \
        --cq-name batch_has_certified_material \
        --cq-name batch_cleared_for_charge \
        --code-ref services/erp/internal/materials/certification.go \
        --code-ref simulators/reactor_charge/model.py \
        --code-ref plc/safety/interlocks/reactor_charge.st \
        --code-ref apps/hmi/src/charge-release.tsx \
        --surface-hint erp \
        --surface-hint simulator \
        --surface-hint plc \
        --surface-hint hmi \
        --max-matches 10
      ;;
    *)
      printf 'unknown software-authoring example id: %s\n' "$1" >&2
      return 1
      ;;
  esac
}

software_authoring_definition_query_rows() {
  local id="$1"

  case "${id}" in
    order_fulfillment)
      printf '%s\n' \
        'define_reserve_credit_process|define the reserve-credit process|process|order fulfillment|true|6' \
        'define_shipment_eligibility_business_rule|define the shipment eligibility business rule|business_rule|accepted checkout and shipping|true|6' \
        'define_checkout_function|define the checkout function that must be covered|function|reserve-credit endpoint|true|6'
      ;;
    subscription_billing)
      printf '%s\n' \
        'define_invoice_settlement_process|define the invoice settlement process|process|subscription billing|true|6' \
        'define_paid_invoice_business_rule|define the paid invoice before access business rule|business_rule|entitlement grant|true|6' \
        'define_product_access_function|define the product access function that must be covered|function|account entitlement|true|6'
      ;;
    process_control)
      printf '%s\n' \
        'define_charge_authorization_process|define the reactor charge authorization process|process|process control|true|8' \
        'define_safe_charge_window_rule|define the safe charge window business rule|business_rule|simulation and interlock evidence|true|8' \
        'define_hmi_release_surface|define the HMI charge release surface that must be covered|implementation_surface|HMI and PLC authorization|true|8'
      ;;
    *)
      printf 'unknown software-authoring example id: %s\n' "${id}" >&2
      return 1
      ;;
  esac
}
