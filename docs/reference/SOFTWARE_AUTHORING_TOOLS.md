# Software Authoring Tools

This is the stable map for Axiograph software-authoring and codegen surfaces.
The domain `.axi` representation stays pure; DDD/fDDD context maps, behavior
cases, implementation surfaces, coverage policy, code refs, generated skeletons,
plugins, and editor integrations are tooling overlays that use the ontology.

## Crates

- `axiograph-tooling-overlays`: typed overlay schemas and read-only reports for
  overlay validation, weak definition queries, coverage queries, codegen plans,
  and policy-driven coverage. Reports include typed ref summaries, mapped
  implementation surfaces, codegen language plans, runtime-theory sidecar
  status, shared authoring-flow profile summaries, and next-action guidance.
- `axiograph-software-authoring`: reusable library plus CLI for continuous
  software coverage, generated skeleton materialization, plugin metadata, and
  LSP/editor capability metadata. Continuous checks read behavior-case reports
  and verify codegen language coverage, code refs, semantic coverage, CQ status,
  and optional runtime-theory sidecars. They embed the same
  `AuthoringFlowReportV1` shape used by overlay-backed software coverage.
- `axiograph-example-software-authoring`: pedagogical example crate showing how
  application/domain packages consume the library for continuous semantic
  coverage checks.
- `axiograph-cli`: unified `axiograph authoring ...`, `axiograph discover ...`,
  `axiograph check ...`, MCP, and DB-server surfaces.

## CLI Surfaces

Primary authoring commands:

```bash
axiograph authoring codegen-plan --overlay overlay.json --out codegen_plan.json
axiograph authoring materialize-skeletons --behavior-report behavior_report.json --out-dir generated --out materialization.json
axiograph authoring continuous-check --behavior-report behavior_report.json --repo-root . --out coverage_gate.json
axiograph authoring tool-specs --out software_authoring_tools.json
axiograph authoring lsp-capabilities --out software_authoring_lsp.json
axiograph authoring integration-manifest --out software_authoring_integrations.json
axiograph authoring lsp
axiograph authoring mcp
```

Recommended user flow:

```bash
axiograph check validate examples/software_authoring/OrderFulfillmentDomain.axi
axiograph check theory examples/software_authoring/OrderFulfillmentDomain.axi --closure-tier finite_fragment
axiograph discover define examples/software_authoring/OrderFulfillmentDomain.axi --prompt "define the shipment eligibility business rule" --include-queries
axiograph discover overlay-check examples/software_authoring/OrderFulfillmentDomain.axi --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json
axiograph discover coverage-query examples/software_authoring/OrderFulfillmentDomain.axi --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json --query examples/software_authoring/order_fulfillment_coverage_query.json
axiograph discover behavior-case examples/software_authoring/OrderFulfillmentDomain.axi --request examples/software_authoring/order_fulfillment_behavior_case.json --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json --out build/examples/software_authoring/behavior_case_report.json
axiograph check software-coverage examples/software_authoring/OrderFulfillmentDomain.axi --behavior-case examples/software_authoring/order_fulfillment_behavior_case.json --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json --out build/examples/software_authoring/software_coverage.json
axiograph authoring codegen-plan --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json --out build/examples/software_authoring/codegen_plan.json
axiograph authoring continuous-check --behavior-report build/examples/software_authoring/behavior_case_report.json --repo-root . --out build/examples/software_authoring/continuous_coverage.json
axiograph authoring continuous-check --behavior-report build/examples/software_authoring/behavior_case_report.json --repo-root . --strict-coverage --out build/examples/software_authoring/enforced_continuous_coverage.json
axiograph authoring continuous-check --behavior-report build/examples/software_authoring/behavior_case_report.json --repo-root . --strict-coverage --require-code-refs --require-runtime-theory --out build/examples/software_authoring/ci_continuous_coverage.json
```

The CLI reports should always leave users with an obvious next action: resolve
typed holes, validate overlays, run weak coverage probes, promote accepted
ontology changes, or materialize generated skeletons through an explicit CLI
write step.

Report contracts:

- Overlay validation resolves `OverlayRefV1` values against compiled IR ids and
  returns a ref summary, serialized `KernelRefV1` handles, stable kernel-ref
  labels, and suggestions for unresolved refs.
- Codegen plans return per-language file hints, required-by-policy status,
  mapped implementation surface ids, and ontology-ref labels.
- Coverage queries stay weak/exploratory even if the request asks for enforced
  mode; enforcement belongs to software coverage and continuous-check gates.
- Continuous coverage consumes a generated `BehaviorCaseReportV1`, reports
  typed refs from receipts/slices/coverage, checks required generated languages,
  and interprets `RuntimeTheoryCheckSummaryV1` sidecars when present.
- `AuthoringFlowReportV1` is embedded at `authoring_flow` inside both
  `continuous_software_coverage_report_v1` variants. It is the shared summary
  agents should read for source (`continuous_check` or
  `overlay_software_coverage`), pass/status, coverage gaps, and profile.
- Coverage profiles are `advisory`, `strict`, and `ci`. `advisory` reports gaps
  and next actions; `strict` fails closed on explicit strict/enforced policy;
  `ci` means strict coverage plus required code refs, runtime-theory sidecars,
  and unresolved-obligation failure.
- Strict or enforced gates may fail on uncovered semantic rules, missing code
  refs, missing generated languages, blocking runtime-theory judgments, or
  residual runtime-theory obligations.

The standalone crate exposes the same production-named tool:

```bash
axiograph-software-authoring codegen-plan --overlay overlay.json --json
axiograph-software-authoring materialize-skeletons --behavior-report behavior_report.json --out-dir generated --json
axiograph-software-authoring continuous-check --behavior-report behavior_report.json --json
axiograph-software-authoring continuous-check --behavior-report behavior_report.json --strict-coverage --require-code-refs --require-runtime-theory --json
axiograph-software-authoring integration-manifest --json
axiograph-software-authoring lsp
axiograph-software-authoring mcp
```

The example crate is intentionally thin: it demonstrates library consumption
from an application package, not a separate semantic authority:

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-software-authoring \
  --bin axiograph-software-authoring-example -- \
  continuous-check \
  --behavior-report build/examples/software_authoring/behavior_case_report.json \
  --repo-root . \
  --out build/examples/software_authoring/example_crate_continuous_coverage.json
```

## MCP And Server Surfaces

MCP/tool-loop tools are read-only. They can plan, validate, and report, but do
not write generated files. For Cursor, Codex, Claude Code, and other MCP-aware
hosts, run the stdio MCP process as a host-managed background server:

```bash
axiograph authoring mcp
```

The standalone binary exposes the same server:

```bash
axiograph-software-authoring mcp
```

The MCP server uses the official `rmcp = 1.5.0` Rust SDK with stdio transport
and MCP stdio framing. It handles the host lifecycle used by MCP clients:
`initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`,
and host-managed process exit/stdin close. Its advertised tool names use the
stable `axiograph_authoring_*` prefix:

MCP tool metadata advertises the embedded `authoring_flow_report_v1` contract
and the `advisory`/`strict`/`ci` profiles. It does not add a separate write or
flow command; hosts call the existing read-only coverage tools and inspect the
`authoring_flow` field.

Any local JSON-RPC dispatcher code is test harness only. Production MCP
entrypoints are `rmcp`-backed stdio servers.

The main `axiograph mcp` semantic-query server follows the same rule: launched
stdio MCP uses `rmcp`, while any hand-rolled JSON-RPC helpers are test harnesses
for semantic tool-shape and dispatch behavior.

- `axiograph_authoring_lsp_capabilities`
- `axiograph_authoring_integration_manifest`
- `axiograph_authoring_codegen_plan`
- `axiograph_authoring_overlay_check`
- `axiograph_authoring_definition_query`
- `axiograph_authoring_coverage_query`
- `axiograph_authoring_software_coverage`

The in-process semantic tool-loop names remain:

- `semantic_overlay_check`
- `semantic_behavior_case_plan`
- `semantic_software_coverage`
- `semantic_codegen_plan`
- `semantic_overlay_refs`
- `semantic_coverage_query`
- `semantic_weak_coverage_probe`
- `semantic_definition_query`

DB-server read-only endpoints mirror the useful authoring tools:

- `POST /semantic/overlay-check`
- `POST /semantic/software-coverage`
- `POST /semantic/codegen-plan`
- `POST /semantic/coverage-query`
- `POST /semantic/definition-query`

The DB HTTP server follows the same maintained-crate rule as MCP and LSP:
`hyper` owns HTTP serving/framing and `http-body-util` owns body collection and
response bodies. Axiograph-owned code should stay focused on route dispatch,
request validation, typed reports, and snapshot/query semantics rather than
generic HTTP parsing.

File materialization stays CLI-only because generated files are reviewable
workspace mutations, not MCP/server side effects.

## Plugin And Editor/LSP Surfaces

The reference stdio adapter is:

```bash
scripts/axiograph_software_authoring_plugin.py
```

Protocol: `axiograph_software_authoring_plugin_v1`.

The Rust tool emits editor capability metadata, a host integration manifest, an
SDK-backed stdio LSP server, and a read-only stdio MCP server:

```bash
axiograph authoring lsp-capabilities
axiograph authoring integration-manifest
axiograph authoring lsp
axiograph authoring mcp
axiograph-software-authoring lsp-capabilities --json
axiograph-software-authoring integration-manifest --json
axiograph-software-authoring lsp
axiograph-software-authoring mcp
```

The LSP server uses `lsp-server = 0.7.9` for stdio transport/framing and
`lsp-types = 0.97` for protocol capability types. The production server no
longer owns hand-rolled LSP frame parsing; Axiograph-specific code is limited to
domain diagnostics, command dispatch, and typed authoring reports. It supports
`initialize`, `textDocument/didOpen`, `textDocument/didChange`,
`textDocument/codeAction`, and `workspace/executeCommand`. It emits diagnostics
for `.axi` parsing, stale behavior-case/tooling schemas, coverage policy shape,
and runtime-theory sidecar presence where the host supplies enough context. It
exposes read-only commands for overlay checking, weak definition queries,
coverage queries, software coverage, codegen planning, and capability
discovery. It does not write files; skeleton materialization remains an
explicit CLI action.

MCP and LSP remain read-only planning/checking surfaces. Do not add
write-capable MCP tools for generated files. Planned read-only additions are
tracked in `docs/roadmaps/ROADMAP_RUNTIME_THEORY_AND_TYPED_WORKFLOWS.md`:
refinement-handle listing for unresolved overlay refs, runtime-theory residual
obligation summaries, and quick links from code actions to the exact CLI
materialization command.

The integration manifest is the portable launcher contract for editor and agent
hosts. It declares both commands as stdio, host-managed background processes:

```json
{
  "lsp": { "command": "axiograph", "args": ["authoring", "lsp"] },
  "mcp": { "command": "axiograph", "args": ["authoring", "mcp"] }
}
```

Hosts may translate that into their local configuration format, but the
Axiograph contract stays the same: LSP is for editor diagnostics and code
actions; MCP is for read-only agent tools; CLI is required for file writes.
The manifest and LSP capability metadata also name `authoring_flow_report_v1`
so hosts can render one coverage/profile card regardless of whether the payload
came from overlay software coverage or continuous-check.

Generic host examples live in:

- `examples/software_authoring/host_integrations/axiograph_authoring_mcp_stdio.json`
- `examples/software_authoring/host_integrations/axiograph_authoring_lsp_stdio.json`

Primary protocol reference: `https://modelcontextprotocol.io/specification/2025-03-26/basic/transports`

## Non-Claims

- Generated skeletons are implementation obligations and review artifacts, not
  accepted code, proofs, or completeness claims.
- Weak definition and coverage queries help authoring and agent planning, but
  cannot satisfy promotion gates.
- Runtime authoring diagnostics are not Lean certification.
