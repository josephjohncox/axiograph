-- Economic Flows as Higher Category
--
-- Models economic transactions using categorical structure:
-- - Objects: Economic agents (firms, households, government)
-- - 1-morphisms: Transactions (money, goods, services)
-- - 2-morphisms: Transaction equivalences (different paths, same result)
--
-- HoTT enables:
-- - Path independence of value flows (conservation laws)
-- - Reversibility analysis (which transactions can be undone?)
-- - Composition of complex financial instruments
-- - Equivalence of economic strategies

module EconomicFlows

schema Economy:
  -- ==========================================================================
  -- Economic Agents (Objects)
  -- ==========================================================================
  object Household
  object Firm
  object Bank
  object Government
  object ForeignSector
  
  -- Aggregate agent type
  object Agent
  subtype Household < Agent
  subtype Firm < Agent
  subtype Bank < Agent
  subtype Government < Agent
  subtype ForeignSector < Agent

  -- ==========================================================================
  -- Flows (1-Morphisms)
  -- ==========================================================================
  object FlowType
  object Amount
  object Time

  -- A flow is a directed transfer between agents
  relation Flow(from: Agent, to: Agent, flowType: FlowType, amount: Amount, time: Time)

  -- Flow composition: consecutive flows
  relation FlowCompose(f1: FlowType, f2: FlowType, result: FlowType)

  -- ==========================================================================
  -- Stock-Flow Consistency (Conservation Laws)
  -- ==========================================================================
  object Account
  object StockType

  -- Every agent has accounts (stocks)
  relation HasAccount(agent: Agent, account: Account, stockType: StockType)

  -- Flows change stocks: flow from A to B decreases A's stock, increases B's
  relation FlowChangesStock(
    flow: FlowType,
    sourceAccount: Account,
    targetAccount: Account,
    deltaSource: Amount,
    deltaTarget: Amount
  )

  -- Conservation: sum of all flows at any time is zero
  -- This is a path-independence condition!

  -- ==========================================================================
  -- Transaction Equivalences (2-Morphisms)
  -- ==========================================================================
  object TransactionPath
  
  -- Two sequences of transactions are equivalent if they result in same state
  relation PathEquivalence(
    path1: TransactionPath,
    path2: TransactionPath,
    witness: Text
  )

  -- Example: (borrow then spend) ≡ (credit card purchase)
  -- Different transaction sequences, same economic outcome

  -- ==========================================================================
  -- Financial Instruments (Complex Flows)
  -- ==========================================================================
  object Instrument
  object ContractTerms

  -- An instrument bundles multiple flows with conditions
  relation InstrumentFlows(instrument: Instrument, flow: FlowType, condition: ContractTerms)

  -- Instrument equivalence: two instruments with same net effect
  relation InstrumentEquiv(i1: Instrument, i2: Instrument, proof: Text)

  -- ==========================================================================
  -- Reversibility (Groupoid Structure)
  -- ==========================================================================
  
  -- Some flows have inverses (can be reversed)
  relation FlowInverse(flow: FlowType, inverse: FlowType)
  
  -- Reversibility condition: flow ; inverse ≡ identity
  -- This makes certain economic subsystems into groupoids!

  -- Support types
  object Text

theory EconomicLaws on Economy:
  -- Conservation: flows from A to B have matching inverses
  constraint functional FlowInverse.flow -> FlowInverse.inverse

  -- Path equivalence is an equivalence relation
  constraint symmetric PathEquivalence
  constraint transitive PathEquivalence

  -- Flow composition is associative (category law)
  equation flow_assoc:
    FlowCompose(FlowCompose(a, b, ab), c, result) =
    FlowCompose(a, FlowCompose(b, c, bc), result)

  -- Identity flow: doing nothing
  equation flow_identity:
    FlowCompose(f, Identity, f) = f

  -- Inverse law (for reversible flows)
  equation flow_inverse:
    FlowCompose(f, FlowInverse(f, inv).inv, Identity) = Identity

  -- Stock-flow consistency: deltas sum to zero
  constraint key FlowChangesStock(flow, sourceAccount, targetAccount)

instance SimpleEconomy of Economy:
  -- Agents
  Household = {Household_A, Household_B}
  Firm = {Firm_X, Firm_Y}
  Bank = {Bank_Z}
  Government = {Gov}
  ForeignSector = {Foreign}

  Agent = {Household_A, Household_B, Firm_X, Firm_Y, Bank_Z, Gov, Foreign}

  -- Flow types (the 1-morphisms!)
  FlowType = {
    -- Real flows
    Labor,          -- Household -> Firm
    Goods,          -- Firm -> Household
    
    -- Financial flows
    Wages,          -- Firm -> Household (money for labor)
    Consumption,    -- Household -> Firm (money for goods)
    Savings,        -- Household -> Bank
    Loans,          -- Bank -> Firm
    LoanRepayment,  -- Firm -> Bank
    Interest,       -- Firm -> Bank, Bank -> Household
    
    -- Government flows
    Taxes,          -- Agent -> Government
    Transfers,      -- Government -> Household
    GovSpending,    -- Government -> Firm
    
    -- Identity (doing nothing)
    Identity
  }

  -- Flow composition (how flows combine)
  FlowCompose = {
    -- Work then get paid: Labor ; Wages ≡ Employment
    (f1=Labor, f2=Wages, result=Employment),
    
    -- Borrow then spend: Loans ; Consumption ≡ CreditPurchase
    (f1=Loans, f2=Consumption, result=CreditPurchase),
    
    -- Earn then save: Wages ; Savings ≡ Accumulation
    (f1=Wages, f2=Savings, result=Accumulation),
    
    -- Tax then transfer: Taxes ; Transfers ≡ Redistribution
    (f1=Taxes, f2=Transfers, result=Redistribution)
  }

  Amount = {Zero, Small, Medium, Large}
  Time = {T1, T2, T3, T4}

  -- Example flows
  Flow = {
    -- Household A works for Firm X
    (from=Household_A, to=Firm_X, flowType=Labor, amount=Medium, time=T1),
    (from=Firm_X, to=Household_A, flowType=Wages, amount=Medium, time=T1),
    
    -- Household A buys from Firm Y
    (from=Household_A, to=Firm_Y, flowType=Consumption, amount=Small, time=T2),
    (from=Firm_Y, to=Household_A, flowType=Goods, amount=Small, time=T2),
    
    -- Household A saves at Bank Z
    (from=Household_A, to=Bank_Z, flowType=Savings, amount=Small, time=T3),
    
    -- Bank Z lends to Firm Y
    (from=Bank_Z, to=Firm_Y, flowType=Loans, amount=Medium, time=T4)
  }

  -- Reversible flows (make this a groupoid!)
  FlowInverse = {
    (flow=Loans, inverse=LoanRepayment),
    (flow=Savings, inverse=Withdrawal),
    (flow=GovSpending, inverse=Taxes)  -- Fiscal identity
  }

  -- Transaction path equivalences (2-morphisms!)
  TransactionPath = {
    -- Path 1: Borrow -> Invest -> Earn -> Repay
    BorrowInvestRepay,
    -- Path 2: Save -> Withdraw -> Invest -> Earn
    SaveInvestEarn
  }

  Text = {SameNetWorth, FiscalBalance, CircularFlow}

  PathEquivalence = {
    -- Both paths end with same net worth change (path independence!)
    (path1=BorrowInvestRepay, path2=SaveInvestEarn, witness=SameNetWorth)
  }

  -- Financial instruments
  Instrument = {
    Bond,           -- Promise to pay future amounts
    Stock,          -- Ownership share
    Derivative,     -- Value derived from other instruments
    Mortgage        -- Loan secured by real estate
  }

  ContractTerms = {
    FixedRate,
    VariableRate,
    Callable,
    Convertible
  }

  -- Instrument equivalence (financial engineering!)
  InstrumentEquiv = {
    -- A callable bond ≡ bond + put option (in terms of cash flows)
    (i1=CallableBond, i2=BondPlusPut, proof=SameCashFlows)
  }

