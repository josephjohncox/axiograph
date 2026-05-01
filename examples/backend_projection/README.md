# Backend Projection Examples

These fixtures show how compiled IR lowers into native-readable backend
artifacts without making the backend the ontology authority.

The examples are intentionally focused:

- `typedb_projection_plan_example.json` sketches the TypeDB mapping that keeps
  relation objects, roles, subtype edges, context/world axes, and caveats
  visible to native TypeQL users.
- `terminusdb_projection_plan_example.json` sketches the TerminusDB/RDF-facing
  mapping that keeps named-graph/world boundaries and branch/history caveats
  visible to WOQL/RDF users.

Mutation authority remains Axiograph semantic VCS. Backend-native writes are
drift until re-imported through proposal, review, CQ/trust gates, and promotion.

Run current backend projection tests with:

```bash
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli backend_pushdown
```
