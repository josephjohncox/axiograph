-- Draft `.axi` module generated from `proposals.json`.
--
-- This output is a pre-vetted raw proposal fixture for teaching draft-module and
-- typed review flows. It is *untrusted* evidence-plane material, not accepted
-- ontology truth. Review before promotion.
--
-- Design notes:
-- - Entities become object inhabitants.
-- - Relations become binary tuples: `Rel(from, to)`.
-- - If proposals include a `context` attribute on relations, we preserve it:
--     - relation decls gain `@context Context`
--     - tuples add `ctx=...`
-- - Missing or heterogeneous endpoint types become explicit `TypeHole_*` review obligations.
-- - Optional constraints are inferred *extensionally* from current tuples.
--
-- Re-generate (from repo root):
--   PATH=/opt/homebrew/bin:$PATH cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- discover draft-module \
--     examples/schema_discovery/fixtures/sql_schema_proposals.json \
--     --out build/SqlSchema.proposals.axi \
--     --module SqlSchema_Proposals \
--     --schema SqlSchema \
--     --instance SqlSchemaInstance \
--     --infer-constraints

module SqlSchema_Proposals

schema SqlSchema:

  -- Object types observed in proposals plus explicit typed holes.
  object SqlColumn
  object SqlTable

  -- Binary relations observed in proposals.
  relation SqlForeignKey(from: SqlTable, to: SqlTable)
  relation SqlHasColumn(from: SqlTable, to: SqlColumn)

theory SqlSchemaExtensional on SqlSchema:
  -- Extensional constraints inferred from current tuples (best-effort).
  -- Treat these as hypotheses: they may not generalize as new data arrives.

  -- Keys: make fact atoms like `SqlForeignKey(from=a, to=b)` eligible for key pruning.
  constraint key SqlForeignKey(from, to)
  constraint key SqlForeignKey(from)
  constraint functional SqlForeignKey.from -> SqlForeignKey.to
  constraint key SqlForeignKey(to)
  constraint functional SqlForeignKey.to -> SqlForeignKey.from

  -- Keys: make fact atoms like `SqlHasColumn(from=a, to=b)` eligible for key pruning.
  constraint key SqlHasColumn(from, to)
  constraint key SqlHasColumn(to)
  constraint functional SqlHasColumn.to -> SqlHasColumn.from

instance SqlSchemaInstance of SqlSchema:
  SqlColumn = {
    Orders_amount_cents,
    Orders_id,
    Orders_user_id,
    Users_id,
    Users_name
  }

  SqlTable = {
    Orders,
    Users
  }

  SqlForeignKey = {
    (from=Orders, to=Users)
  }

  SqlHasColumn = {
    (from=Orders, to=Orders_amount_cents),
    (from=Orders, to=Orders_id),
    (from=Orders, to=Orders_user_id),
    (from=Users, to=Users_id),
    (from=Users, to=Users_name)
  }
