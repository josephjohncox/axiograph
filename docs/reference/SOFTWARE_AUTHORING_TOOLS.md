# Software Authoring Tools

**Diátaxis:** Reference
**Audience:** ontology authors, tool integrators, contributors

Axiograph has one typed ontology-authoring service. CLI, LSP, MCP, and HTTP
adapters all deserialize `authoring_workspace_request_v1`, call
`AuthoringWorkspaceService::execute`, and serialize
`authoring_workspace_report_v1`. The adapters contain no separate authoring
semantics or report families.

Software coverage and code generation remain tooling overlays. They consume the
ontology through compiled references; they do not extend canonical `.axi`.

## Authority

The authoring service reads a workspace-relative canonical `.axi` root and its
import closure. `CanonicalCompiler` produces the one compiled kernel snapshot.
The service derives PathDB and runtime indexes from that snapshot for finite
queries, CQ execution, runtime diagnostics, and repair suggestions.

The authority boundary does not move:

- canonical accepted `.axi` plus its compiled kernel IR is the meaning plane;
- PathDB and runtime indexes are derived execution substrates;
- Rust authoring checks are untrusted operational checks; and
- only the import closure of `lean/Axiograph/VerifyMain.lean` is the trusted
  checker.

## Unified Request

A request names files relative to one configured workspace root. Absolute paths,
`..`, paths that resolve outside the workspace, non-regular files, and oversized
sources are rejected before semantic processing.

```json
{
  "version": "authoring_workspace_request_v1",
  "operation": "promotion_review",
  "axi_path": "examples/software_authoring/OrderFulfillmentDomain.axi",
  "baseline_axi_path": "examples/software_authoring/OrderFulfillmentDomain.axi",
  "cq_path": "examples/software_authoring/order_fulfillment.cq",
  "schema": "OrderFulfillment",
  "query_ir_v1": {
    "version": 1,
    "select_vars": ["order"],
    "where_atoms": [
      { "kind": "type", "term": "?order", "type": "Order" }
    ],
    "limit": 10
  },
  "focus_variable": "order"
}
```

`axi_text`, `baseline_axi_text`, and `cq_text` may replace root-buffer contents
without writing a file. The compiler still resolves imports beneath the
workspace root. This is the LSP unsaved-buffer path, not a second compiler.

Operations:

| Operation | Behavior |
| --- | --- |
| `inspect` | Runs every applicable read-only check requested by the payload. |
| `apply_repair` | Applies one emitted query or olog refinement handle and rechecks. |
| `validate` | Compiles canonical `.axi`, checks finite category formation, and runs finite runtime-theory admissibility. |
| `promotion_review` | Runs the same checks and returns fail-closed protected-main gate status. It does not mutate AxiStore. |

## Unified Report

Every adapter returns `authoring_workspace_report_v1`. The report contains:

- the workspace-relative source and exact ordered module closure;
- repository, candidate snapshot, root revision, and compiled IR digests;
- structured diagnostics;
- query, olog, and CQ typed holes;
- one `RuntimeRefinementCandidateV1` repair currency;
- canonical `CompetencyQuestionV1` records plus finite execution results;
- `PreparedQueryMetadataV1`, elaborated query IR, plan, exploration, and trust;
- optional applied query or olog repair results;
- finite kernel payload diff and typed olog evolution previews;
- finite runtime-theory admissibility results;
- a protected-main promotion review; and
- explicit scope and non-claims.

CQ authoring no longer has a parallel DTO/report family. `.cq` parsing,
lowering, execution, diagnostics, and repair use the canonical
`CompetencyQuestionV1` and prepared-query services.

## Finite Evolution Semantics

If a request supplies `baseline_axi_path`, the service compares baseline and
candidate `KernelSnapshotIr` payload fingerprints. It classifies logical module,
schema, object, relation-object, role, theory, constraint, path-equation,
rewrite, instance, and fact ids as preserved, changed, added, or removed.

This is exact equality of encoded finite compiled payloads. It does not prove:

- categorical equivalence or a universal property;
- naturality or complete functorial transport;
- univalence, higher-path equality, or general HoTT semantics;
- termination or confluence of arbitrary rewrite systems; or
- ontology closure or open-world completeness.

Typed olog checks add the supported finite path fragment: relation objects,
projection arrows, subtype-compatible role fillers, compositional path
endpoints, and path-equation endpoint equality. Unsupported higher semantics
remain explicit residual obligations.

### Primary regulated-shipment authoring request

`examples/regulated_shipment/authoring_request.json` compares the accepted
baseline with the candidate, runs three CQs, prepares and explains the bounded
`ShipmentContainsBatch / BatchHasCertificate` query, and emits the finite
payload evolution preview. Its behavior-case overlay then generates a Rust test
whose receipt cites the regulated-shipment bounded context and all covered key
rules. The workflow compiles and executes that generated test; it does not
pretend that skeleton generation implements the dispatch service.

## Promotion Review

The service fails protected-main review closed. It reports four gate decisions:

1. canonical validation and compiled IR;
2. competency questions;
3. finite runtime-theory admissibility; and
4. trusted checker.

Omitted CQs block the CQ gate. Runtime-theory blockers or residual obligations
block the theory gate. The trusted-checker gate remains blocked because the
read-only adapters do not produce or accept a substitute for a VerifyMain
receipt.

`candidate_reviewable=true` means the finite runtime checks found no blocking
report diagnostic. It does not mean the candidate is accepted.
`protected_main_eligible` remains false until a separate trusted workflow builds
a complete `PromotionPlan` and calls
`AxiStore::promote(expected_generation, plan)`.

## CLI

Run the checked-in example request:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring workspace \
  --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --out build/examples/software_authoring/authoring_workspace_report.json
```

Inspect adapter capabilities and launch metadata:

```bash
axiograph authoring lsp-capabilities --out authoring_capabilities.json
axiograph authoring integration-manifest --workspace . --out authoring_integrations.json
```

The remaining software-overlay commands are separate from ontology authoring:

```bash
axiograph authoring codegen-plan --overlay overlay.json --out codegen_plan.json
axiograph authoring materialize-skeletons \
  --behavior-report behavior_report.json \
  --out-dir generated \
  --out materialization.json
axiograph authoring continuous-check \
  --behavior-report behavior_report.json \
  --repo-root . \
  --strict-coverage \
  --require-code-refs \
  --require-runtime-theory \
  --out coverage_gate.json
```

## LSP

```bash
axiograph authoring lsp \
  --workspace . \
  --axi examples/software_authoring/OrderFulfillmentDomain.axi
```

The LSP publishes `.axi` diagnostics from the unified report. If `--axi` is
set, it also checks unsaved `.cq` buffers against that workspace-relative root.
`workspace/executeCommand` accepts one `authoring_workspace_request_v1` under
`axiograph.authoring.workspace`. Code actions invoke that same command.

The LSP is read-only.

## MCP

```bash
axiograph authoring mcp --workspace .
```

The rmcp server publishes one read-only tool:

```text
axiograph_authoring_workspace
```

Its input schema is `authoring_workspace_request_v1`; its structured result is
`authoring_workspace_report_v1`. The former per-feature authoring MCP tools were
removed rather than retained as compatibility shims.

## HTTP

```bash
axiograph authoring serve --workspace . --listen 127.0.0.1:8787
```

Endpoints:

- `GET /healthz`
- `GET /authoring/capabilities`
- `POST /authoring`

`POST /authoring` accepts the same request used by CLI, LSP, and MCP. The server
is read-only and enforces a 1 MiB request-body limit.

## Standalone Software-Authoring Crate

`axiograph-software-authoring` now contains only software coverage, overlay
codegen planning, and explicit skeleton materialization:

```bash
axiograph-software-authoring codegen-plan --overlay overlay.json --json
axiograph-software-authoring continuous-check --behavior-report report.json --json
axiograph-software-authoring materialize-skeletons \
  --behavior-report report.json --out-dir generated --json
axiograph-software-authoring tool-specs --json
```

Its duplicate CQ, LSP, MCP, capability, and integration-manifest commands were
deleted. Use the main `axiograph authoring` workspace service for ontology
authoring and editor/agent/server integration.

## Non-Claims

- Diagnostics and repair handles are runtime guidance, not proof terms.
- Prepared-query certifiability is metadata, not a certificate.
- CQ satisfaction covers the supplied finite questions and data only.
- Evolution previews are finite structural comparisons, not general semantic
  equivalence proofs.
- Promotion review is read-only and cannot advance accepted state.
- Generated skeletons are implementation obligations, not accepted code or
  proofs.
