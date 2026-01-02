-- PhysicsMeasurements.axi
--
-- A canonical `.axi` module for *large-scale* measurement / observation logs.
--
-- Why this exists
-- ===============
--
-- The accepted plane is the meaning plane: it defines *what* a measurement
-- record is (schema + theory), not the entire raw dataset.
--
-- For realistic workloads, we want to ingest:
-- - tens/hundreds of thousands of observations,
-- - with explicit context/world scoping,
-- - with evidence/provenance links (DocChunks),
-- without bloating the canonical `.axi` snapshots.
--
-- The intended workflow is:
--
--   1) Promote this module into the accepted plane (append-only).
--   2) Commit observations into the PathDB WAL as evidence-plane overlays
--      (`proposals.json` + `chunks.json`) using `axiograph db accept pathdb-commit`.
--   3) Query/visualize across planes; optionally require certificates for
--      high-value answers (Lean verifies).
--
-- Design choice: “values” are not stored as raw floats in the canonical surface.
-- -----------------------------------------------------------------------------
-- The `.axi` v1 surface is identifier-only (no literals).
-- For large logs, turning every numeric value into a first-class node is often
-- too expensive, so the intended pattern is:
--
-- - store the *typed* part as edges (run/quantity/unit/bin/context/time),
-- - store the raw numeric value as an attribute on the fact node in the WAL
--   overlay (e.g. `value_f64="3.14159"`), and/or keep an approximation via
--   `ScalarBin` to preserve typed queryability.
--
-- This keeps the type layer useful while staying scalable.

module PhysicsMeasurements

schema PhysicsMeasurements:
  -- ==========================================================================
  -- Context/world and time
  -- ==========================================================================
  object Context
  object Time

  -- ==========================================================================
  -- Measurement “domain”
  -- ==========================================================================
  object Run         -- a single experimental run / simulation run / dataset shard
  object Quantity    -- what is being measured (position, curvature, temperature, …)
  object Unit
  object ScalarBin   -- coarse discretization (typed approximation of a numeric)

  -- Support types (identifier-only surface; content comes from DocChunks).
  object Text

  relation QuantityDescription(quantity: Quantity, text: Text)
  relation UnitSymbol(unit: Unit, text: Text)
  relation QuantityHasCanonicalUnit(quantity: Quantity, unit: Unit)

  -- ==========================================================================
  -- Observation facts (reified records)
  -- ==========================================================================
  -- The observation is a typed record:
  --   MeasurementObs(run, quantity, unit, value_bin, ctx, time)
  --
  -- Context and time are first-class scoping axes:
  -- - `@context Context` adds a `ctx: Context` field
  -- - `@temporal Time` adds a `time: Time` field
  --
  -- In PathDB, every imported observation becomes a fact-node with field edges,
  -- plus a derived traversal edge:
  --   run -MeasurementObs-> quantity
  --
  -- so users can query either:
  -- - record-shaped facts (`MeasurementObs(run=…, quantity=…, …)`), or
  -- - path-style edges (`name("Run_0") -MeasurementObs-> ?q`).
  relation MeasurementObs(
    run: Run,
    quantity: Quantity,
    unit: Unit,
    value_bin: ScalarBin
  ) @context Context @temporal Time

theory PhysicsMeasurementsRules on PhysicsMeasurements:
  -- Metadata hygiene
  constraint key QuantityDescription(quantity, text)
  constraint key UnitSymbol(unit, text)

  -- Each quantity chooses a canonical unit (typed “field”).
  constraint functional QuantityHasCanonicalUnit.quantity -> QuantityHasCanonicalUnit.unit
  constraint key QuantityHasCanonicalUnit(quantity, unit)

  -- Determinism of observations per (run, quantity, ctx, time).
  --
  -- Intuition:
  --   within a given world/context and timestamp, a run produces at most one
  --   observation for a given quantity (unit/bin are outputs).
  -- Canonical: express multi-field determinism as a composite key on the
  -- “input” fields (it implies `run,quantity,ctx,time` functionally determine
  -- the remaining fields).
  constraint key MeasurementObs(run, quantity, ctx, time)
  constraint key MeasurementObs(run, quantity, unit, value_bin, ctx, time)

instance PhysicsMeasurementsSeed of PhysicsMeasurements:
  -- A small seed instance:
  -- - provides stable identifiers for common quantities/units/contexts,
  -- - keeps the accepted plane tiny,
  -- - real datasets are added as WAL overlays.

  Context = {ObservedSensors, Simulation, Literature, TacitNotes}
  Time = {T0}

  Run = {Run_Seed_0, Run_Seed_1}

  Unit = {Unit_Meter, Unit_Second, Unit_Kelvin, Unit_Dimensionless}
  Text = {Text_m, Text_s, Text_K, Text_1}

  UnitSymbol = {
    (unit=Unit_Meter, text=Text_m),
    (unit=Unit_Second, text=Text_s),
    (unit=Unit_Kelvin, text=Text_K),
    (unit=Unit_Dimensionless, text=Text_1)
  }

  Quantity = {
    PositionX,
    VelocityX,
    AccelerationX,
    Temperature,
    CurvatureScalar
  }

  QuantityHasCanonicalUnit = {
    (quantity=PositionX, unit=Unit_Meter),
    (quantity=VelocityX, unit=Unit_Dimensionless),     -- demo placeholder (unit algebra is future work)
    (quantity=AccelerationX, unit=Unit_Dimensionless), -- demo placeholder
    (quantity=Temperature, unit=Unit_Kelvin),
    (quantity=CurvatureScalar, unit=Unit_Dimensionless)
  }

  QuantityDescription = {
    (quantity=PositionX, text=Text_m),        -- “measured in meters”
    (quantity=Temperature, text=Text_K)
  }

  -- One seed bin: real bins are typically imported as overlay entities.
  ScalarBin = {Bin_0}

  -- A couple of seed observations (accepted plane stays tiny).
  MeasurementObs = {
    (run=Run_Seed_0, quantity=PositionX, unit=Unit_Meter, value_bin=Bin_0, ctx=ObservedSensors, time=T0),
    (run=Run_Seed_0, quantity=Temperature, unit=Unit_Kelvin, value_bin=Bin_0, ctx=ObservedSensors, time=T0)
  }
