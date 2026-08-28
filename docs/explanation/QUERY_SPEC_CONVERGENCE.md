# Query + Spec Convergence

**Diataxis:** Explanation  
**Audience:** contributors

This note summarizes the current path toward a more unified query + spec
language in Axiograph.

## Summary

Axiograph does **not** need a greenfield replacement language to converge query
and spec work. The current repo already contains most of the required seams:

- `query_ir_v1`
- AxQL
- typed authoring and olog checking
- competency-question generation/evaluation/repair
- typed refinement handles
- proposal validation and CQ gating
- semantic evolution previews
- canonical `.axi` parsing / formatting / import / export

The likely next step is **convergence by compilation and shared contracts**, not
replacement by one monolithic new DSL.

## Internal convergence points

### Query IR and AxQL

`query_ir_v1` is already the machine-facing contract, and AxQL is the execution
core. That means a future query+spec surface should usually lower into those
existing forms rather than bypass them.

### Typed refinement

Typed refinement is already shared across:

- query repair,
- olog authoring,
- migration transport,
- reconciliation,
- and competency-question repair.

This is the strongest current convergence seam.

### Competency questions and proposal validation

Competency questions already work as executable acceptance tests over the same
typed query/report surfaces. Proposal validation already feeds into CQ gating
and evolution previews.

### Canonical `.axi`

Canonical `.axi` remains the reviewable meaning plane. Any unified query+spec
surface should remain subordinate to that meaning plane rather than replacing it.

## Best external precedents

### CQL

Best precedent for:

- query + constraints + migration in one semantic family,
- typed transforms,
- and declarative data migration.

What to import:

- one semantic core for querying, constraints, and typed transforms.

What not to import:

- category-heavy surface syntax as the default user-facing language.

### Gherkin / Cucumber

Best precedent for:

- executable scenarios,
- rule grouping,
- examples tables,
- and business-readable behavior descriptions.

What to import:

- readable scenario/spec blocks that can compile into typed IR.

What not to import:

- step-definition indirection as the source of truth.

### TypeQL

Best precedent for:

- typed schema-aware query UX,
- role-aware query composition,
- schema-first thinking.

### GraphQL

Best precedent for:

- self-describing query surfaces,
- introspection,
- predictable shapes,
- editor/autocomplete friendliness.

## Practical direction

The best practical direction is:

1. keep `query_ir_v1`, AxQL, typed refinement, CQ gating, and canonical `.axi`
   as the real semantic machinery,
2. add more readable scenario/spec/front-door surfaces only when they compile
   into those existing contracts,
3. avoid replacing the current IR stack with a prose-first or query-only system.

## Non-goals

- No step-definition-driven semantic core.
- No greenfield query-language rewrite.
- No split between “query language” and “spec language” if they can compile into
  the same IR family.
- No category-theory-first syntax for average programmers.

## External references

- CQL README: https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/README.md
- CQL simplified query syntax: https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/resources/open/docs/QueryExpRawSimple.md
- CQL query front / constraints: https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/resources/open/docs/QueryExpFront.md
- CQL check pragma: https://github.com/CategoricalData/CQL/blob/6b4d0f7d7f0a3759738e7bd0a2a908beb4c8b92e/resources/open/docs/PragmaExpCheck.md
- CQL papers: https://categoricaldata.net/papers.html
- Cucumber BDD docs: https://docs.cucumber.io/bdd/
- Example Mapping: https://docs.cucumber.io/bdd/example-mapping/
- Gherkin reference: https://cucumber.io/docs/gherkin/
- Cucumber JS examples: https://github.com/cucumber/cucumber-js
- TypeQL reference: https://typedb.com/docs/typeql-reference/
- TypeQL schema define: https://typedb.com/docs/typeql-reference/schema/define/
- GraphQL schema docs: https://graphql.org/learn/schema/
- GraphQL introspection docs: https://graphql.org/learn/introspection/
