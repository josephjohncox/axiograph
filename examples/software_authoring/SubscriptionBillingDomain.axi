-- Pure domain ontology for subscription billing and entitlement grants.
--
-- Application services, code refs, fDDD context maps, coverage policy, and
-- codegen plans live in `subscription_billing_tooling_overlay.json`.

module SubscriptionBillingDomain

schema SubscriptionBilling:
  object Context
  object Time
  object Account
  object Subscription
  object Plan
  object Invoice
  object Payment
  object Entitlement
  object BillingProcess
  object BillingPolicy
  object BusinessInvariant

  relation AccountHasSubscription(account: Account, subscription: Subscription, ctx: Context, time: Time)
  relation SubscriptionUsesPlan(subscription: Subscription, plan: Plan, ctx: Context, time: Time)
  relation SubscriptionGeneratesInvoice(subscription: Subscription, invoice: Invoice, ctx: Context, time: Time)
  relation InvoicePaidBy(invoice: Invoice, payment: Payment, ctx: Context, time: Time)
  relation PaidInvoiceGrantsEntitlement(invoice: Invoice, entitlement: Entitlement, ctx: Context, time: Time)
  relation SubscriptionHasEntitlement(subscription: Subscription, entitlement: Entitlement, ctx: Context, time: Time)
  relation AccountGrantedEntitlement(account: Account, entitlement: Entitlement, ctx: Context, time: Time)
  relation ProcessRequiresInvariant(process: BillingProcess, invariant: BusinessInvariant)
  relation PolicyRequiresInvariant(policy: BillingPolicy, invariant: BusinessInvariant)

theory SubscriptionBillingRules on SubscriptionBilling:
  constraint key AccountHasSubscription(account, subscription, ctx, time)
  constraint key SubscriptionUsesPlan(subscription, plan, ctx, time)
  constraint key SubscriptionGeneratesInvoice(subscription, invoice, ctx, time)
  constraint key InvoicePaidBy(invoice, payment, ctx, time)
  constraint key PaidInvoiceGrantsEntitlement(invoice, entitlement, ctx, time)
  constraint key SubscriptionHasEntitlement(subscription, entitlement, ctx, time)
  constraint key AccountGrantedEntitlement(account, entitlement, ctx, time)
  constraint key ProcessRequiresInvariant(process, invariant)
  constraint key PolicyRequiresInvariant(policy, invariant)

  -- A paid invoice grants the subscription entitlement used by product access.
  equation paid_invoice_entitles_subscription:
    trans(step(subscription, SubscriptionGeneratesInvoice, invoice), step(invoice, PaidInvoiceGrantsEntitlement, entitlement)) =
    step(subscription, SubscriptionHasEntitlement, entitlement)

  -- Product-access code should normalize subscription entitlement to the account
  -- entitlement read model it serves.
  rewrite subscription_to_account_entitlement:
    orientation: forward
    vars: account: Account, subscription: Subscription, entitlement: Entitlement
    lhs: trans(step(account, AccountHasSubscription, subscription), step(subscription, SubscriptionHasEntitlement, entitlement))
    rhs: step(account, AccountGrantedEntitlement, entitlement)

instance SubscriptionBillingSeed of SubscriptionBilling:
  Context = {Accepted, Review, Evidence}
  Time = {T0, T1}

  Account = {Account_Acme}
  Subscription = {Sub_Pro}
  Plan = {Plan_Pro}
  Invoice = {Invoice_2026_04}
  Payment = {Payment_2026_04}
  Entitlement = {Entitlement_API}
  BillingProcess = {IssueInvoice, GrantEntitlement}
  BillingPolicy = {PaidInvoiceRequired, ActiveSubscriptionRequired}
  BusinessInvariant = {PaidInvoiceBeforeAccess, EntitlementBoundToActiveSubscription}

  AccountHasSubscription = {
    (account=Account_Acme, subscription=Sub_Pro, ctx=Accepted, time=T0)
  }

  SubscriptionUsesPlan = {
    (subscription=Sub_Pro, plan=Plan_Pro, ctx=Accepted, time=T0)
  }

  SubscriptionGeneratesInvoice = {
    (subscription=Sub_Pro, invoice=Invoice_2026_04, ctx=Accepted, time=T0)
  }

  InvoicePaidBy = {
    (invoice=Invoice_2026_04, payment=Payment_2026_04, ctx=Accepted, time=T1)
  }

  PaidInvoiceGrantsEntitlement = {
    (invoice=Invoice_2026_04, entitlement=Entitlement_API, ctx=Accepted, time=T1)
  }

  SubscriptionHasEntitlement = {
    (subscription=Sub_Pro, entitlement=Entitlement_API, ctx=Accepted, time=T1)
  }

  AccountGrantedEntitlement = {
    (account=Account_Acme, entitlement=Entitlement_API, ctx=Accepted, time=T1)
  }

  ProcessRequiresInvariant = {
    (process=IssueInvoice, invariant=PaidInvoiceBeforeAccess),
    (process=GrantEntitlement, invariant=EntitlementBoundToActiveSubscription)
  }

  PolicyRequiresInvariant = {
    (policy=PaidInvoiceRequired, invariant=PaidInvoiceBeforeAccess),
    (policy=ActiveSubscriptionRequired, invariant=EntitlementBoundToActiveSubscription)
  }
