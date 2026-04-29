-- Chemical-plant operations core.
--
-- This module is the accepted base slice for the semantic-merge example. It
-- models the stable domain spine shared by procurement/certification and
-- simulation/safety review branches.

module PlantOperationsCore

schema PlantOps:
  object Context
  object Time

  object MaterialLot
  object Supplier
  object Reactor
  object ProductionRun
  object Product
  object WorkOrder
  object Recipe
  object ProcessStep

  relation LotSuppliedBy(lot: MaterialLot, supplier: Supplier)
  relation RunUsesLot(run: ProductionRun, lot: MaterialLot, ctx: Context, time: Time)
  relation RunInReactor(run: ProductionRun, reactor: Reactor, ctx: Context, time: Time)
  relation WorkOrderBuildsProduct(order: WorkOrder, product: Product, ctx: Context, time: Time)
  relation WorkOrderUsesRecipe(order: WorkOrder, recipe: Recipe, ctx: Context, time: Time)
  relation RecipeStep(recipe: Recipe, step: ProcessStep, sequence: Time)

theory PlantOpsRules on PlantOps:
  constraint key LotSuppliedBy(lot, supplier)
  constraint key RunUsesLot(run, lot, ctx, time)
  constraint key RunInReactor(run, reactor, ctx, time)
  constraint key WorkOrderBuildsProduct(order, product, ctx, time)
  constraint key WorkOrderUsesRecipe(order, recipe, ctx, time)

instance PlantOpsSeed of PlantOps:
  Context = {Plan, Released, Observed, Audit}
  Time = {Seq_10, Seq_20, T0, T1}

  MaterialLot = {Lot_A17}
  Supplier = {NorthDockChem}
  Reactor = {R101}
  ProductionRun = {Run_042}
  Product = {SolventBlend_A}
  WorkOrder = {WO_5001}
  Recipe = {Recipe_Blend_A}
  ProcessStep = {ChargeReactor, HoldAtTemperature}

  LotSuppliedBy = {
    (lot=Lot_A17, supplier=NorthDockChem)
  }

  RunUsesLot = {
    (run=Run_042, lot=Lot_A17, ctx=Released, time=T1)
  }

  RunInReactor = {
    (run=Run_042, reactor=R101, ctx=Released, time=T1)
  }

  WorkOrderBuildsProduct = {
    (order=WO_5001, product=SolventBlend_A, ctx=Plan, time=T0)
  }

  WorkOrderUsesRecipe = {
    (order=WO_5001, recipe=Recipe_Blend_A, ctx=Plan, time=T0)
  }

  RecipeStep = {
    (recipe=Recipe_Blend_A, step=ChargeReactor, sequence=Seq_10),
    (recipe=Recipe_Blend_A, step=HoldAtTemperature, sequence=Seq_20)
  }
