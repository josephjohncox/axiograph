-- Supply Chain as Higher Category
--
-- Models manufacturing supply chains with HoTT structure:
-- - Objects: Entities (suppliers, factories, warehouses, customers)
-- - 1-morphisms: Material/info flows
-- - 2-morphisms: Process equivalences (different routes, same outcome)
--
-- HoTT enables:
-- - Path independence for logistics (multiple routes, same delivery)
-- - Supplier substitution via equivalence
-- - Bill of Materials as a functor
-- - Process verification as path equality

module SupplyChainHoTT

schema SupplyChain:
  -- ==========================================================================
  -- Supply Chain Entities (Objects)
  -- ==========================================================================
  object Supplier
  object Factory
  object Warehouse
  object Customer

  -- Generic node type
  object Node
  subtype Supplier < Node
  subtype Factory < Node
  subtype Warehouse < Node
  subtype Customer < Node

  -- ==========================================================================
  -- Flows (1-Morphisms)
  -- ==========================================================================
  object Material
  object Quantity
  object LeadTime
  object Cost

  -- A flow is a directed transfer
  relation Flow(from: Node, to: Node, material: Material, qty: Quantity, time: LeadTime)

  -- Flow composition (sequential transfers)
  relation FlowCompose(f1: Flow, f2: Flow, result: Flow)

  -- ==========================================================================
  -- Process Equivalences (2-Morphisms)
  -- ==========================================================================
  object Route          -- A sequence of flows
  object RouteProof     -- Proof that two routes are equivalent

  -- Two routes from A to B are equivalent if same material arrives
  relation RouteEquivalence(
    from: Node,
    to: Node,
    route1: Route,
    route2: Route,
    proof: RouteProof
  )

  -- This is path independence: supplier → factory via different warehouses
  -- should deliver the same product

  -- ==========================================================================
  -- Bill of Materials (Functor Structure)
  -- ==========================================================================
  object Product        -- Finished product
  object Component      -- Component/subassembly
  object BOMRelation    -- Part → Assembly relation

  relation BOM(product: Product, component: Component, qty: Quantity)

  -- BOM is a functor: Component → Product preserving structure
  -- If components are equivalent, products are equivalent

  -- ==========================================================================
  -- Supplier Equivalence
  -- ==========================================================================
  -- Two suppliers are equivalent if they can deliver the same material
  -- with same quality (within tolerance)

  object SupplierCapability
  object QualityLevel

  relation SupplierProvides(supplier: Supplier, cap: SupplierCapability)
  relation SupplierQuality(supplier: Supplier, material: Material, quality: QualityLevel)

  relation SupplierEquiv(
    s1: Supplier,
    s2: Supplier,
    material: Material,
    qualityMatch: RouteProof
  )

  -- Via univalence: equivalent suppliers can be substituted!

  -- ==========================================================================
  -- Process Verification
  -- ==========================================================================
  object ProcessStep
  object ProcessSpec    -- Expected outcome

  relation ProcessSteps(factory: Factory, step: ProcessStep)
  relation ProcessProduces(step: ProcessStep, material: Material)
  relation ProcessRequires(step: ProcessStep, input: Material)

  -- Two process chains are equivalent if they satisfy the same spec
  relation ProcessEquiv(
    chain1: Route,
    chain2: Route,
    spec: ProcessSpec,
    proof: RouteProof
  )

  -- ==========================================================================
  -- Inventory as State
  -- ==========================================================================
  object InventoryState
  object Location

  relation Inventory(loc: Location, material: Material, qty: Quantity)

  -- Flow changes inventory (like stock-flow in economics)
  relation FlowChangesInventory(
    flow: Flow,
    sourceLoc: Location,
    targetLoc: Location,
    deltaSrc: Quantity,
    deltaTgt: Quantity
  )

  -- Conservation: material is neither created nor destroyed in transport

theory SupplyChainLaws on SupplyChain:
  -- Flow composition is associative
  equation flow_assoc:
    FlowCompose(FlowCompose(a, b, ab), c, result) =
    FlowCompose(a, FlowCompose(b, c, bc), result)

  -- Route equivalence is an equivalence relation
  constraint symmetric RouteEquivalence
  constraint transitive RouteEquivalence

  -- Supplier equivalence is symmetric
  constraint symmetric SupplierEquiv

  -- BOM defines a unique product structure
  constraint key BOM(product, component)

  -- Inventory deltas are well-defined per flow + endpoints.
  -- Canonical form: represent multi-field functional dependency as a composite key.
  constraint key FlowChangesInventory(flow, sourceLoc, targetLoc)

  -- Process equivalence respects composition
  -- If P1 ≃ P2 and Q1 ≃ Q2, then P1;Q1 ≃ P2;Q2

instance ManufacturingExample of SupplyChain:
  -- Suppliers
  Supplier = {
    RawMetal_A,    -- Primary supplier
    RawMetal_B,    -- Backup supplier (equivalent!)
    Cutting_Tools,
    Coolant_Supply
  }

  -- Factories
  Factory = {
    Machining_Plant,
    Assembly_Plant,
    QC_Center
  }

  -- Warehouses
  Warehouse = {
    RawMaterial_WH,
    WIP_WH,
    Finished_WH
  }

  Customer = {Customer_X, Customer_Y}

  Node = {
    RawMetal_A, RawMetal_B, Cutting_Tools, Coolant_Supply,
    Machining_Plant, Assembly_Plant, QC_Center,
    RawMaterial_WH, WIP_WH, Finished_WH,
    Customer_X, Customer_Y
  }

  -- Materials
  Material = {
    Steel_Billet,
    Aluminum_Bar,
    Carbide_Insert,
    Coolant,
    Machined_Part,
    Assembled_Product
  }

  Quantity = {Q0, Q10, Q100, Q1000}
  LeadTime = {Days_1, Days_3, Days_7, Days_14}
  Cost = {Low, Medium, High}

  -- Flows (the 1-morphisms!)
  Flow = {
    -- Raw material flows
    (from=RawMetal_A, to=RawMaterial_WH, material=Steel_Billet, qty=Q1000, time=Days_7),
    (from=RawMetal_B, to=RawMaterial_WH, material=Steel_Billet, qty=Q1000, time=Days_14),

    -- Production flows
    (from=RawMaterial_WH, to=Machining_Plant, material=Steel_Billet, qty=Q100, time=Days_1),
    (from=Machining_Plant, to=WIP_WH, material=Machined_Part, qty=Q100, time=Days_3),

    -- Assembly flows
    (from=WIP_WH, to=Assembly_Plant, material=Machined_Part, qty=Q100, time=Days_1),
    (from=Assembly_Plant, to=Finished_WH, material=Assembled_Product, qty=Q100, time=Days_3),

    -- Delivery flows
    (from=Finished_WH, to=Customer_X, material=Assembled_Product, qty=Q10, time=Days_1)
  }

  -- Routes (paths through the supply chain)
  Route = {
    -- Route 1: Supplier A → WH → Machining → Assembly → Customer
    Route_Via_SupplierA,
    -- Route 2: Supplier B → WH → Machining → Assembly → Customer
    Route_Via_SupplierB,
    -- Route 3: Direct from supplier to machining (skip WH)
    Route_Direct
  }

  RouteProof = {
    SameMaterial,       -- Materials are identical
    QualityEquiv,       -- Quality within tolerance
    LeadTimeTradeoff,   -- Different time, same outcome
    CostEquiv           -- Different cost, same product
  }

  -- ROUTE EQUIVALENCES (2-morphisms!)
  RouteEquivalence = {
    -- Via Supplier A ≃ Via Supplier B (same steel arrives)
    (from=RawMetal_A, to=Machining_Plant,
     route1=Route_Via_SupplierA,
     route2=Route_Via_SupplierB,
     proof=SameMaterial),

    -- WH staging ≃ Direct delivery (for just-in-time)
    (from=RawMetal_A, to=Machining_Plant,
     route1=Route_Via_SupplierA,
     route2=Route_Direct,
     proof=LeadTimeTradeoff)
  }

  -- SUPPLIER EQUIVALENCE (univalence for sourcing!)
  SupplierCapability = {Steel_Supply, Aluminum_Supply, Tooling_Supply}
  QualityLevel = {Q_Standard, Q_Premium, Q_Budget}

  SupplierProvides = {
    (supplier=RawMetal_A, cap=Steel_Supply),
    (supplier=RawMetal_B, cap=Steel_Supply),
    (supplier=Cutting_Tools, cap=Tooling_Supply)
  }

  SupplierQuality = {
    (supplier=RawMetal_A, material=Steel_Billet, quality=Q_Premium),
    (supplier=RawMetal_B, material=Steel_Billet, quality=Q_Premium)
  }

  -- RawMetal_A ≃ RawMetal_B (for Steel_Billet)
  -- This means we can substitute! Dual-sourcing is justified.
  SupplierEquiv = {
    (s1=RawMetal_A, s2=RawMetal_B, material=Steel_Billet, qualityMatch=QualityEquiv)
  }

  -- Products and BOM
  Product = {Widget, Gadget}
  Component = {Body, Shaft, Housing, FastenerKit}

  BOM = {
    (product=Widget, component=Body, qty=Q1),
    (product=Widget, component=Shaft, qty=Q1),
    (product=Gadget, component=Housing, qty=Q1),
    (product=Gadget, component=FastenerKit, qty=Q10)
  }

  -- Process steps
  ProcessStep = {
    Rough_Mill,
    Finish_Mill,
    Drill,
    Tap,
    Deburr,
    Inspect,
    Assemble
  }

  ProcessSpec = {ToleranceSpec, SurfaceFinishSpec, AssemblySpec}

  ProcessSteps = {
    (factory=Machining_Plant, step=Rough_Mill),
    (factory=Machining_Plant, step=Finish_Mill),
    (factory=Machining_Plant, step=Drill),
    (factory=QC_Center, step=Inspect),
    (factory=Assembly_Plant, step=Assemble)
  }

  ProcessRequires = {
    (step=Rough_Mill, input=Steel_Billet),
    (step=Finish_Mill, input=Rough_Part),
    (step=Assemble, input=Machined_Part)
  }

  ProcessProduces = {
    (step=Rough_Mill, material=Rough_Part),
    (step=Finish_Mill, material=Machined_Part),
    (step=Assemble, material=Assembled_Product)
  }

  -- PROCESS EQUIVALENCE
  -- Two machining sequences that meet the same spec
  ProcessEquiv = {
    -- Conventional milling ≃ High-speed milling (same part, different process)
    (chain1=ConventionalRoute, chain2=HSMRoute, spec=ToleranceSpec, proof=SameMaterial)
  }

  -- Inventory locations
  Location = {
    Loc_RawMaterial_WH,
    Loc_WIP_WH,
    Loc_Finished_WH,
    Loc_Machining,
    Loc_Assembly
  }

  -- Current inventory
  InventoryState = {State_T0, State_T1}

  Inventory = {
    (loc=Loc_RawMaterial_WH, material=Steel_Billet, qty=Q1000),
    (loc=Loc_WIP_WH, material=Machined_Part, qty=Q100),
    (loc=Loc_Finished_WH, material=Assembled_Product, qty=Q10)
  }
