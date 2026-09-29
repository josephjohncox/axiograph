# LLM Query Integration

**Diataxis:** How-to
**Audience:** users, tool authors, and agent-integration authors

LLM-assisted Axiograph workflows are typed tool workflows. The model may help
draft questions, queries, definitions, or proposals, but Axiograph remains the
authority for type checking, execution, trust contracts, and promotion.

Use this rule:

- humans ask in natural language, `.cq`, or AxQL;
- tools lower `query_ir_v1` into `CompiledFiniteQuery`;
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
| Promotion-sensitive query results | `CompiledFiniteQuery` plus certificate policy | Single query lifecycle and fail-closed V4 verifier policy |

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

For typed authoring flows, run the workspace-aware read-only MCP adapter:

```bash
axiograph authoring mcp --workspace .
```

It exposes one `axiograph_authoring_workspace` tool. The tool accepts the same
`authoring_workspace_request_v1` used by CLI, LSP, and HTTP and returns
`authoring_workspace_report_v1`. Overlay coverage and codegen remain explicit
CLI/tooling-overlay workflows rather than a second authoring MCP protocol.

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

Competency questions should be readable domain questions first, not lowered AxQL
first. A `.cq` file can express the authoring intent:

```text
ask accepted orders can be shipped only when payment is captured
about OrderEligibleForShipment
given accepted_order
expect shipment_eligible
```

Then lower, prepare, execute, and explain it through the shared workspace service:

```bash
axiograph authoring workspace \
  --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --out build/examples/software_authoring/authoring_workspace_report.json
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

## Interpret Semantic Search Scores

The `semantic_search` tool response is versioned as
`axiograph_semantic_search_response_v2`. Read the nested fields by method, not
as one provider confidence:

```json
{
  "version": "axiograph_semantic_search_response_v2",
  "authority": "evidence_only",
  "scores": {
    "fusion": {"value": 0.82, "method": "max_available_fusion_v1"},
    "token": {"value": 0.41, "method": "normalized_token_hash_dot_exhaustive_v1"},
    "embedding": {
      "value": 0.82,
      "method": "normalized_embedding_cosine_exhaustive_v1",
      "source": {"backend": "openai", "model": "example-model"}
    }
  }
}
```

The fragment shows one hit's `scores` object. `token` or `embedding` is `null`
only when that method did not score the hit. Axiograph keeps every computed
component through fusion and applies result limits to the fused ranking, so
`null` does not mean that a computed score fell outside a hidden candidate
window. `fusion` is the maximum available value, not a calibrated confidence.
The tool uses exhaustive snapshot-local scans and requires
`methods.ann_used` to be `false`.

Do not send the old unversioned `similarity_ollama` field to a V2 decoder. The
decoder uses position-specific score-method types and closed top-level method
descriptors. It rejects legacy or unknown fields, swapped token/embedding/fusion
methods, contradictory scan descriptors, and `ann_used=true` instead of guessing
their meaning.

A token-hash score requires a finite, non-zero token vector. Indexed text with
no token terms has no token score, and a nonempty query with no token terms
fails closed. Neither case becomes a normalized token score of zero.

Provider calls return query vectors only. Axiograph computes the displayed
embedding cosine locally. Stored vectors and provider query vectors must have a
finite, non-zero norm; resolved rows are not publicly mutable, and scoring
rechecks both norms when it computes the cosine denominator. A zero vector fails
closed instead of becoming a cosine score of zero. These fields are retrieval
evidence and cannot issue a certificate, change canonical `.axi`, or bypass
typed review and promotion.

## Typed Query Lifecycle

Every promotion-sensitive query should follow one lifecycle:

```text
QueryIrV1 -> CompiledFiniteQuery -> QueryAnswer<Validated> -> QueryAnswer<CertificateEmitted> -> QueryAnswer<LeanVerified>
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

axiograph authoring workspace \
  --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --out build/examples/software_authoring/authoring_workspace_report.json

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
- Derived PathDB rows and `.axpd` images must not be used as certificate or
  semantic authority.
