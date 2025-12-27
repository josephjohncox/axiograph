-- Supply chain + modalities + dependent-type-like witnesses (HoTT-ish demo)
--
-- This module is a realistic “knowledge generation + exploration” example:
--
-- - **Modalities (world/context indexing)**:
--     - We model multiple *contexts/worlds* (`Plan`, `Observed`, `Policy`).
--     - Key relations are annotated with `@context Context`, so each tuple is
--       explicitly scoped to a context (no silent closed-world assumptions).
--     - In PathDB, this becomes `fact -axi_fact_in_context-> ctx`, enabling
--       scoped AxQL queries via `ctx use ...` / `in { ... }`.
--
-- - **Dependent types (practical DB encoding)**:
--     - N-ary tuples are imported as *fact nodes* with typed fields.
--       Think: a dependent record “Flow(from: Node, to: Node, ...)”.
--     - Higher structure is explicit:
--         - `RouteEquivalence` is a “2-cell” (path between paths) with a `proof`.
--         - `JustificationPath` is a proof-term object whose “type” is the
--           conclusion obligation it justifies (`Justifies(..., conclusion=...)`).
--
-- - **Power of typing (useful in practice)**:
--     - The REPL query elaborator uses the meta-plane as a type layer:
--         - catches unknown relation/field names early,
--         - infers variable types and inserts implied constraints,
--         - and shows the elaborated query (use `q --elaborate ...`).
--
-- Trust boundary reminder:
-- - Certificates prove **derivability from inputs**, not correctness of inputs.
-- - This module is canonical `.axi` input; PathDB is a derived query substrate.

module SupplyChainModalitiesHoTT

schema SupplyChainModal:
  -- ==========================================================================
  -- Worlds / contexts (modal indexing)
  -- ==========================================================================
  object Context

  -- A simple “possible-worlds” accessibility relation.
  -- (Useful for epistemic-style questions: “in all accessible worlds...”.)
  relation Accessible(from: Context, to: Context)

  -- ==========================================================================
  -- Core supply chain objects (Nodes)
  -- ==========================================================================
  object Supplier
  object Factory
  object Warehouse
  object Customer

  object Node
  subtype Supplier < Node
  subtype Factory < Node
  subtype Warehouse < Node
  subtype Customer < Node

  -- ==========================================================================
  -- 1-cells: flows (as context-scoped dependent records)
  -- ==========================================================================
  object Material
  object Quantity
  object LeadTime

  -- A flow is a directed transfer, scoped to a Context.
  relation Flow(from: Node, to: Node, material: Material, qty: Quantity, time: LeadTime) @context Context

  -- ==========================================================================
  -- 2-cells: equivalences between routes (homotopies / “paths between paths”)
  -- ==========================================================================
  object Route
  object RouteProof

  -- Two routes are considered equivalent (in a context) if they are interchangeable
  -- for a given planning/verification purpose.
  relation RouteEquivalence(
    from: Node,
    to: Node,
    route1: Route,
    route2: Route,
    proof: RouteProof
  ) @context Context

  -- ==========================================================================
  -- Modal knowledge: propositions, evidence, and obligations
  -- ==========================================================================
  object Agent
  object Proposition
  object Evidence
  object Obligation

  -- Optional integration point for tacit knowledge chunks (added in the demo via REPL):
  -- `DocChunk` entities can carry attributes like `text=...`, enabling `fts(...)`.
  object DocChunk

  -- A proposition holds in a world/context (extensional truth-at-world relation).
  relation Holds(world: Context, prop: Proposition)

  -- Evidence supports propositions, and is itself context-scoped.
  relation EvidenceSupports(ev: Evidence, prop: Proposition) @context Context

  -- Evidence may have supporting doc chunks (tooling / discovery convenience).
  relation EvidenceChunk(ev: Evidence, chunk: DocChunk)

  -- Untrusted but useful: evidence suggests an obligation (edges can carry confidence).
  -- This is intended for discovery workflows; promote reviewed suggestions into canonical `.axi`.
  relation EvidenceSuggestsObligation(ev: Evidence, obl: Obligation)

  -- Deontic: obligations that hold at a world/context.
  relation Obligatory(world: Context, obl: Obligation)

  -- ==========================================================================
  -- Proof terms: explicit justifications and equivalences between them
  -- ==========================================================================
  object JustificationPath

  -- A justification path is a proof-term whose “type” is the conclusion obligation.
  relation Justifies(path: JustificationPath, agent: Agent, conclusion: Obligation)

  relation JustificationUsesEvidence(path: JustificationPath, ev: Evidence)

  relation JustificationEquiv(path1: JustificationPath, path2: JustificationPath)

theory SupplyChainModalRules on SupplyChainModal:
  -- Accessibility: treat it as an S4-ish preorder.
  constraint transitive Accessible

  -- Route equivalence is an equivalence relation (up to a chosen semantics).
  constraint symmetric RouteEquivalence
  constraint transitive RouteEquivalence

  -- Evidence is functional: an evidence item can support at most one proposition in a given context.
  constraint key EvidenceSupports(ev, ctx)

  -- Justification equivalence is an equivalence relation.
  constraint symmetric JustificationEquiv
  constraint transitive JustificationEquiv

instance SupplyChainModalDemo of SupplyChainModal:
  -- Contexts/worlds: planning vs observations vs policy.
  Context = {Plan, Observed, Policy}

  Accessible = {
    (from=Plan, to=Plan),
    (from=Plan, to=Observed),
    (from=Observed, to=Observed),
    (from=Policy, to=Policy)
  }

  -- Nodes
  Supplier = {RawMetal_A, RawMetal_B}
  Factory = {Machining_Plant}
  Warehouse = {RawMaterial_WH}
  Customer = {Customer_X}

  Node = {
    RawMetal_A, RawMetal_B,
    Machining_Plant,
    RawMaterial_WH,
    Customer_X
  }

  -- Materials / quantities / lead times
  Material = {Steel_Billet, Machined_Part}
  Quantity = {Q10, Q100, Q1000}
  LeadTime = {Days_1, Days_7, Days_14}

  -- Flows:
  --
  -- In `Plan` we believe Supplier A can deliver in 7 days.
  -- In `Observed` we see Supplier A slipping to 14 days.
  Flow = {
    (from=RawMetal_A, to=RawMaterial_WH, material=Steel_Billet, qty=Q1000, time=Days_7, ctx=Plan),
    (from=RawMetal_B, to=RawMaterial_WH, material=Steel_Billet, qty=Q1000, time=Days_14, ctx=Plan),
    (from=RawMaterial_WH, to=Machining_Plant, material=Steel_Billet, qty=Q100, time=Days_1, ctx=Plan),
    (from=Machining_Plant, to=Customer_X, material=Machined_Part, qty=Q100, time=Days_7, ctx=Plan),

    (from=RawMetal_A, to=RawMaterial_WH, material=Steel_Billet, qty=Q1000, time=Days_14, ctx=Observed),
    (from=RawMaterial_WH, to=Machining_Plant, material=Steel_Billet, qty=Q100, time=Days_1, ctx=Observed)
  }

  -- Routes + route proofs.
  Route = {Route_Via_SupplierA, Route_Via_SupplierB, Route_Direct}
  RouteProof = {SameMaterial, LeadTimeTradeoff}

  -- Route equivalences (2-cells):
  -- In the planning world, routes via Supplier A and B are considered equivalent (same steel arrives).
  -- In the observed world, we *do not* assert this equivalence (it might fail due to delays/quality).
  RouteEquivalence = {
    (from=RawMetal_A, to=Machining_Plant,
     route1=Route_Via_SupplierA,
     route2=Route_Via_SupplierB,
     proof=SameMaterial,
     ctx=Plan)
  }

  -- Modal knowledge objects.
  Agent = {Planner, Auditor}
  Proposition = {SupplierA_Delayed, BackupSupplierAvailable}
  Evidence = {PolicyDoc_0, ERPEvent_0, SensorAlert_0}
  Obligation = {UseBackupSupplier_B, RunIncomingQC}
  DocChunk = {}

  Holds = {
    (world=Plan, prop=BackupSupplierAvailable),
    (world=Observed, prop=SupplierA_Delayed)
  }

  EvidenceSupports = {
    (ev=ERPEvent_0, prop=SupplierA_Delayed, ctx=Observed),
    (ev=PolicyDoc_0, prop=BackupSupplierAvailable, ctx=Policy)
  }

  Obligatory = {
    (world=Plan, obl=RunIncomingQC),
    (world=Plan, obl=UseBackupSupplier_B)
  }

  -- Proof terms (justifications):
  -- Two different “reasons” can justify the same obligation, and we can record
  -- an explicit equivalence between those justifications.
  JustificationPath = {Justification_Policy, Justification_ObservedDelay}

  Justifies = {
    (path=Justification_Policy, agent=Planner, conclusion=UseBackupSupplier_B),
    (path=Justification_ObservedDelay, agent=Planner, conclusion=UseBackupSupplier_B)
  }

  JustificationUsesEvidence = {
    (path=Justification_Policy, ev=PolicyDoc_0),
    (path=Justification_ObservedDelay, ev=ERPEvent_0)
  }

  JustificationEquiv = {
    (path1=Justification_Policy, path2=Justification_ObservedDelay)
  }

