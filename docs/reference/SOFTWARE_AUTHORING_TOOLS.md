# Software Authoring Tools

This is the stable map for Axiograph software-authoring and codegen surfaces.
The domain `.axi` representation stays pure; DDD/fDDD context maps, behavior
cases, implementation surfaces, coverage policy, code refs, generated skeletons,
plugins, and editor integrations are tooling overlays that use the ontology.

## Crates

- `axiograph-tooling-overlays`: typed overlay schemas and read-only reports for
  overlay validation, weak definition queries, coverage queries, codegen plans,
  and policy-driven coverage.
- `axiograph-software-authoring`: reusable library plus CLI for continuous
  software coverage, generated skeleton materialization, plugin metadata, and
  LSP/editor capability metadata.
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

The standalone crate exposes the same production-named tool:

```bash
axiograph-software-authoring codegen-plan --overlay overlay.json --json
axiograph-software-authoring materialize-skeletons --behavior-report behavior_report.json --out-dir generated --json
axiograph-software-authoring continuous-check --behavior-report behavior_report.json --json
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
for `.axi` parsing and stale behavior-case/tooling schemas, and exposes
read-only commands for overlay checking, weak definition queries, coverage
queries, software coverage, codegen planning, and capability discovery. It does
not write files; skeleton materialization remains an explicit CLI action.

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
