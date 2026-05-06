# LLM Query Integration

**Diataxis:** How-to
**Audience:** users, tool authors, and agent harness authors

LLM-assisted Axiograph workflows are typed tool workflows. The model may help
draft questions, queries, definitions, or proposals, but Axiograph remains the
authority for type checking, execution, trust contracts, and promotion.

Use this rule:

- humans ask in natural language, `.cq`, or AxQL;
- tools lower to `query_ir_v1` / `PreparedQueryV1`;
- Rust elaborates, typechecks, runs, and reports typed metadata;
- Lean verification is optional and only applies to supported certified
  fragments;
- model output stays weak/advisory until accepted through review, CQ/trust
  gates, and semantic VCS.

Do not copy old conceptual JSON query snippets as a protocol. JSON exists at
typed tool boundaries, not as the authoring experience.

## Preferred Surfaces

| Use Case | Preferred Surface | Why |
| --- | --- | --- |
| Agent asks and runs queries | `axiograph mcp` or DB server typed endpoints | Host-managed MCP/API lifecycle, typed tool schemas, no custom JSON-RPC |
| Human explores in the terminal | REPL `ask`, `q --elaborate`, `q --typecheck` | Fast feedback, inferred types, typed holes, refinement handles |
| CQ/BDD/DDD authoring | `.cq` plus behavior-case/overlay tools | Question-first, domain readable, lowerable to typed query checks |
| Weak definition discovery | `discover define` / `semantic_definition_query` | Advisory grounding for “define this process/rule/function” prompts |
| Promotion-sensitive query results | `PreparedQueryV1` plus certificate policy | Shared query lifecycle and fail-closed verifier policy |

See also:

- `docs/reference/LLM_REPL_PLUGIN.md`
- `docs/reference/SOFTWARE_AUTHORING_TOOLS.md`
- `docs/howto/CANONICAL_SEMANTIC_SPINE.md`
- `docs/reference/QUERY_LANG.md`

## MCP And Tool-Loop Flow

Run the semantic MCP server for Cursor, Codex, Claude Code, or another MCP host:

```bash
axiograph mcp --axi examples/software_authoring/OrderFulfillmentDomain.axi
```

The host should call typed tools rather than asking the model to invent raw
AxQL. The useful query/exploration loop is:

1. `semantic_definition_query` for weak process/function/business-rule discovery.
2. `axql_explore` or CQ authoring tools for candidate query shapes and typed
   holes.
3. `axql_elaborate` for type inference, prepared metadata, and repair handles.
4. `axql_run` only after the query is well-typed enough to execute.
5. Certificate policy `emit`, `verify`, or `require_verified` only when the
   canonical `.axi`, accepted anchor, certifiable fragment, and verifier are
   available.

For software-authoring flows, prefer the dedicated read-only authoring MCP:

```bash
axiograph authoring mcp
```

That server exposes overlay checks, weak coverage probes, behavior-case planning,
software coverage, codegen previews, and integration metadata without writing
files.

## REPL Flow

The REPL is useful when a human wants tight feedback:

```text
axiograph> import_axi examples/software_authoring/OrderFulfillmentDomain.axi
axiograph> ask which accepted orders are eligible to ship?
axiograph> q --elaborate select ?order where ?order is Order, ?order -OrderEligibleForShipment-> ?eligibility limit 20
axiograph> q --typecheck select ?order where ?order is Order limit 20
```

Use `ask` for natural-language-ish exploration. Use `q --elaborate` when you
want the typed query plan, inferred variable types, and refinement handles. Use
`q --typecheck` in scripts when execution is not needed.

## CQ-First Authoring

Competency questions should be readable domain questions first, not raw AxQL
first. A `.cq` file can express the authoring intent:

```text
ask accepted orders can be shipped only when payment is captured
about OrderEligibleForShipment
given accepted_order
expect shipment_eligible
```

Then lower and check it through the authoring tools:

```bash
axiograph authoring competency-questions \
  --axi examples/software_authoring/OrderFulfillmentDomain.axi \
  --cq examples/software_authoring/order_fulfillment.cq \
  --out build/examples/software_authoring/competency_questions_authoring.json
```

Raw AxQL remains available as a precise lowering/debug format, but it is not the
primary way to ask business-domain coverage questions.

## Definition Queries

For weak discovery, let the tool classify and ground the prompt:

```bash
axiograph discover define examples/software_authoring/OrderFulfillmentDomain.axi \
  --prompt "define the shipment eligibility business rule" \
  --include-queries
```

This returns candidate ontology refs, likely relations/paths/CQs, ambiguity
notes, and suggested next queries. It is deliberately weak: it cannot satisfy
promotion gates or enforced software coverage.

## Typed Query Lifecycle

Every promotion-sensitive query should follow one lifecycle:

```text
QueryIrV1 -> PreparedQueryV1 -> ValidatedQueryAnswer -> CertifiedQueryAnswer
```

The shared certificate policy is:

- `none`: runtime type/execution report only.
- `emit`: emit verifier-facing witness when the fragment supports it.
- `verify`: run the verifier when available, otherwise report verifier failure.
- `require_verified`: fail closed unless canonical `.axi`, accepted anchor,
  certifiable fragment, Lean verifier, and anchor match are all present.

This policy is shared across CLI, REPL, server `/query`, MCP tools, CQ runners,
behavior-case reports, and Lean fixtures.

## Strong And Weak Outputs

Use the output class correctly:

- **Strong query/report claim:** typed over canonical `.axi`, anchored,
  well-typed, executed under declared context/world assumptions, and optionally
  verified for a supported fragment.
- **Weak discovery claim:** grounded by candidate refs, text, embeddings,
  evidence, or model output, but not accepted or fully checked.
- **Unknown:** the ontology lacks enough typed structure to decide.
- **Conflicted:** multiple accepted/review/evidence-plane interpretations are
  active.

Do not collapse these into a scalar confidence score. Agents should surface
trust class, anchors, caveats, residual obligations, and next actions.

## Minimal End-To-End Example

```bash
axiograph check validate examples/software_authoring/OrderFulfillmentDomain.axi

axiograph check theory examples/software_authoring/OrderFulfillmentDomain.axi \
  --closure-tier finite_fragment \
  --json \
  --out build/examples/software_authoring/theory_check.json

axiograph discover define examples/software_authoring/OrderFulfillmentDomain.axi \
  --prompt "define the reserve-credit process" \
  --include-queries \
  --out build/examples/software_authoring/definition_query.json

axiograph authoring competency-questions \
  --axi examples/software_authoring/OrderFulfillmentDomain.axi \
  --cq examples/software_authoring/order_fulfillment.cq \
  --out build/examples/software_authoring/competency_questions_authoring.json

axiograph discover behavior-case examples/software_authoring/OrderFulfillmentDomain.axi \
  --request examples/software_authoring/order_fulfillment_behavior_case.json \
  --cq-file examples/software_authoring/order_fulfillment.cq \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/software_authoring/behavior_case_report.json
```

For the complete DDD/fDDD software-authoring path, run:

```bash
examples/software_authoring/run_authoring_flow.sh
```

## Integration Boundaries

- MCP uses the `rmcp`-backed server surfaces.
- LSP uses `lsp-server` and `lsp-types`.
- HTTP clients should use typed DB-server endpoints and maintained HTTP client
  libraries.
- Command plugins are local adapter/debug boundaries: one typed request on
  stdin, one typed response on stdout. They are not the product protocol.
- PathDBExport snapshots are storage/debug/parser parity only and must not be
  used as query, certificate, or semantic authority.
