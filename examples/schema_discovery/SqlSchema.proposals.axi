-- Candidate `.axi` module produced from `proposals.json` (schema discovery).
--
-- This file is intentionally small and reviewable. It demonstrates:
-- - bootstrapping a schema from structured source proposals (SQL DDL-like),
-- - importing into PathDB to get a meta-plane for schema-directed AxQL planning,
-- - extensional constraints (keys/functionals) as an experiment.
--
-- Re-generate (from repo root):
--   cd rust
--   cargo run -p axiograph-cli -- discover draft-module \
--     ../examples/schema_discovery/sql_schema_proposals.json \
--     --out ../build/SqlSchema.proposals.axi \
--     --module SqlSchema_Proposals \
--     --schema SqlSchema \
--     --instance SqlSchemaInstance \
--     --infer-constraints

module SqlSchema_Proposals

schema SqlSchema:
  -- Safe fallback supertype for heterogeneous endpoints.
  object Entity

  -- Types observed in proposals.
  object SqlTable
  object SqlColumn

  subtype SqlTable < Entity
  subtype SqlColumn < Entity

  -- Relations observed in proposals.
  relation SqlHasColumn(from: SqlTable, to: SqlColumn)
  relation SqlForeignKey(from: SqlTable, to: SqlTable)

theory SqlSchemaExtensional on SqlSchema:
  -- Extensional constraints inferred from the observed tuples (hypotheses).

  -- Basic key (supports key-based pruning of fact atoms).
  constraint key SqlHasColumn(from, to)
  -- Extensional: each column belongs to a single table.
  constraint key SqlHasColumn(to)
  constraint functional SqlHasColumn.to -> SqlHasColumn.from

  constraint key SqlForeignKey(from, to)
  -- Extensional: one FK per table in this tiny example.
  constraint key SqlForeignKey(from)
  constraint functional SqlForeignKey.from -> SqlForeignKey.to

instance SqlSchemaInstance of SqlSchema:
  SqlTable = {Users, Orders}

  SqlColumn = {
    Users_id,
    Users_name,
    Orders_id,
    Orders_user_id,
    Orders_amount_cents
  }

  SqlHasColumn = {
    (from=Users, to=Users_id),
    (from=Users, to=Users_name),
    (from=Orders, to=Orders_id),
    (from=Orders, to=Orders_user_id),
    (from=Orders, to=Orders_amount_cents)
  }

  SqlForeignKey = {
    (from=Orders, to=Users)
  }

