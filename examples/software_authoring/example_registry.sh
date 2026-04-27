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
    order_fulfillment_definition_queries.json)
      printf '%s\n' order_fulfillment
      ;;
    subscription_billing_definition_queries.json)
      printf '%s\n' subscription_billing
      ;;
    process_control_definition_queries.json)
      printf '%s\n' process_control
      ;;
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
    order_fulfillment:coverage_query)
      printf '%s\n' 'examples/software_authoring/order_fulfillment_coverage_query.json'
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
    subscription_billing:coverage_query)
      printf '%s\n' 'examples/software_authoring/subscription_billing_coverage_query.json'
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
    process_control:coverage_query)
      printf '%s\n' 'examples/software_authoring/process_control_coverage_query.json'
      ;;
    *)
      printf 'unknown software-authoring example field: %s.%s\n' "${id}" "${field}" >&2
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
