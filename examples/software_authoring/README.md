# Software Authoring With Pure Domain AXI And Tooling Overlays

This example demonstrates ontology-driven software authoring without polluting
the domain representation. The `.axi` file models order-fulfillment domain
facts and theory only. DDD/fDDD context maps, behavior planning, implementation
surfaces, code refs, coverage policy, and codegen live in JSON tooling overlays.
Those JSON files are versioned typed tool payloads such as
`tooling_overlay_bundle_v1` and `behavior_case_check_request_v1`, not anonymous
ad hoc snippets. The teaching flow starts from `.axi`, `.cq`, direct advisory
definition prompts, and direct advisory coverage flags. Keep JSON here as tool
input: user-authored ontology stays in `.axi`, and executable CQs should prefer
`.cq` text files when they are not embedded in a behavior-case request.

## Files

- `OrderFulfillmentDomain.axi` is the pure canonical domain ontology.
- `order_fulfillment_tooling_overlay.json` maps the ontology to fDDD context,
  implementation surfaces, code refs, coverage policy, and codegen hints.
- `order_fulfillment_behavior_case.json` is a domain-only behavior case.
- `order_fulfillment.cq` is the executable CQ suite for that behavior case.
- Coverage-query commands are weak/advisory probes over terms, refs, code
  paths, and surfaces. They intentionally do not require users to author AxQL.
- Weak definition-query commands contain authoring prompts for planning and
  discovery. They intentionally do not require users to write saved JSON bundles.
- `SubscriptionBillingDomain.axi` demonstrates API/worker codegen planning for
  paid invoices, product access, and entitlement grants.
- `ProcessControlDomain.axi` demonstrates ERP, simulator, HMI, PLC, and
  process-control coverage without embedding tooling concepts in `.axi`.
- `software_authoring_examples.json` is the typed suite catalog for all
  software-authoring/codegen examples.
- `example_registry.sh` mirrors that suite for shell runners. It exists so the
  examples do not require `jq`, Python, or another JSON-filter helper. When a
  registry changes, update both the typed JSON suite and this parser-free shell
  registry.
- `host_integrations/` contains generic stdio launch examples for MCP and LSP
  hosts such as Cursor, Codex, Claude Code, and editor language-client plugins.
  They are pedagogical launcher shapes, not custom protocol specifications.

## Flow At A Glance

The intended learning path is:

1. Validate the canonical domain `.axi`.
2. Check the supported runtime-theory fragment.
3. Check question-first `.cq` competency questions.
4. Ask advisory definition questions for authoring and planning context.
5. Validate the tooling overlay against compiled IR ids.
6. Run advisory coverage and behavior-case reports.
7. Run software coverage, codegen planning, and continuous advisory/strict/CI
   gates.
8. Materialize generated skeleton previews only into a review directory.

## Teaching Path

Use this order when teaching or debugging the flow:

| Entrypoint | Use When | Scope |
| --- | --- | --- |
| Direct CLI commands below | You are learning, debugging, or building a new domain. | Public front door: `.axi`, `.cq`, weak prompts, overlays, coverage, behavior, codegen. |
| `authoring run` | You want one compact walkthrough report for one cataloged example. | Single-command teaching surface over the same `.axi`, `.cq`, overlay, coverage, behavior, and codegen reports. |
| `run_codegen_examples.sh` | You want to exercise every bundled domain/codegen example. | Full suite over order fulfillment, subscription billing, and process control. |
| `run_authoring_flow.sh` | You want the detailed order-fulfillment walkthrough with intermediate JSON artifacts. | Pedagogical script for the longest path. |

Run every software-authoring/codegen example in the suite:

```bash
./examples/software_authoring/run_codegen_examples.sh
```

Run one example from the suite:

```bash
EXAMPLE_ID=subscription_billing \
  ./examples/software_authoring/run_codegen_examples.sh
```

Run the full scripted path:

```bash
./examples/software_authoring/run_authoring_flow.sh
```

This writes reports to `build/examples/software_authoring/` and generated test
skeleton previews to `build/examples/software_authoring/generated-tests/`.
It also runs the `axiograph-example-software-authoring` crate to show how a
domain/application package can consume the authoring library for continuous
semantic coverage checks.
Coverage reports now embed an `AuthoringFlowReportV1` at `authoring_flow` with
the active profile: `advisory`, `strict`, or `ci`.
Strict and CI gates are expected to fail closed in some teaching runs; the
scripts keep going only after confirming the fail-closed report was written.

The shell runners do not require an external JSON parser or adapter script.
They call the Rust CLI/library surfaces directly, including the unified
`authoring workspace` request, `authoring tool-specs`,
`authoring lsp-capabilities`, `authoring integration-manifest`, and
`authoring codegen-plan`. The full flow runs validation, runtime theory checks,
question-first CQ execution and repair, advisory definition lookup, overlay validation,
advisory coverage queries, behavior-case reporting, software coverage,
codegen planning, advisory continuous coverage, strict continuous coverage, and
explicit skeleton materialization. The full flow also runs a CI-profile
continuous check through the existing `authoring continuous-check` command with
`--strict-coverage`, `--require-code-refs`, and `--require-runtime-theory`.
Automation and CI should call the same commands with `strict` or `ci` profiles;
the compact teaching flow and the enforcement flow differ by policy, not by a
separate adapter language.

Emit a single combined authoring-suite report for one example:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring run \
  --suite examples/software_authoring/software_authoring_examples.json \
  --example order_fulfillment \
  --profile advisory \
  --repo-root . \
  --out-dir build/examples/software_authoring/order_fulfillment_authoring_run \
  --out build/examples/software_authoring/order_fulfillment_authoring_run.json
```

When `--out-dir` is set, `authoring run` writes the same intermediate artifacts
that an agent would inspect: `overlay_validation.json`, `behavior_case_report.json`,
`overlay_coverage.json`, `continuous_coverage.json`, `coverage_query.json`, and
`definition_queries.json`. The definition-query artifact is produced from
inline catalog prompts and the same weak `DefinitionQueryV1` report builder used
by `discover define`; it is not a saved prompt bundle.

Validate the domain ontology:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/software_authoring/OrderFulfillmentDomain.axi
```

Inspect runtime-theory admissibility for the supported finite fragment:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check theory examples/software_authoring/OrderFulfillmentDomain.axi \
  --closure-tier finite_fragment
```

Run question-first CQs, query preparation, diagnostics, repairs, finite evolution
preview, validation, and promotion review through the same workspace service used
by LSP, MCP, and HTTP:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring workspace \
  --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --out build/examples/software_authoring/authoring_workspace_report.json
```

Ask an advisory definition question. This is useful for discovery and authoring,
not for correctness gates:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover define examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --prompt "define the shipment eligibility business rule" \
  --kind-hint business_rule \
  --include-queries
```

Validate the overlay against compiled IR ids:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover overlay-check examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json
```

Run an advisory coverage query:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover coverage-query examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --term "shipment eligibility" \
  --relation OrderEligibleForShipment \
  --cq-name accepted_order_is_shipment_eligible \
  --surface-hint shipping \
  --max-matches 8
```

Generate a behavior-case report from domain behavior plus overlay tooling:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover behavior-case examples/software_authoring/OrderFulfillmentDomain.axi \
  --request examples/software_authoring/order_fulfillment_behavior_case.json \
  --cq-file examples/software_authoring/order_fulfillment.cq \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/order_fulfillment_behavior_case_report.json
```

Run continuous software coverage through the core CLI:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check software-coverage examples/software_authoring/OrderFulfillmentDomain.axi \
  --behavior-case examples/software_authoring/order_fulfillment_behavior_case.json \
  --cq-file examples/software_authoring/order_fulfillment.cq \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/order_fulfillment_software_coverage.json
```

Plan generated test skeletons without writing files:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring codegen-plan \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/software_authoring/codegen_plan.json
```

Run the generated behavior report through advisory and strict continuous gates:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring continuous-check \
  --behavior-report build/examples/order_fulfillment_behavior_case_report.json \
  --repo-root . \
  --out build/examples/software_authoring/continuous_coverage.json

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring continuous-check \
  --behavior-report build/examples/order_fulfillment_behavior_case_report.json \
  --repo-root . \
  --strict-coverage \
  --out build/examples/software_authoring/enforced_continuous_coverage.json
```

The same command can emit the CI profile without changing command names.
`discover behavior-case` computes and embeds `RuntimeTheoryCheckSummaryV1`
from the canonical `.axi` input. `--require-runtime-theory` therefore checks the
same anchored summary rather than relying on a separately copied sidecar.

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring continuous-check \
  --behavior-report build/examples/order_fulfillment_behavior_case_report.json \
  --repo-root . \
  --strict-coverage \
  --require-code-refs \
  --require-runtime-theory \
  --out build/examples/software_authoring/ci_continuous_coverage.json
```

The continuous gate preserves the summary's nested finite scope, admissibility
trace, transport counts, residual ids, and structured non-claims. Review-only,
evidence-excluded, residual, blocked, resolver-required, or malformed summary
states fail closed. No completeness or ontology-closure field is synthesized.

Run the same generated behavior report through the pedagogical example crate:

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-example-software-authoring \
  --bin axiograph-software-authoring-example -- \
  continuous-check \
  --behavior-report build/examples/order_fulfillment_behavior_case_report.json \
  --repo-root . \
  --out build/examples/order_fulfillment_example_crate_continuous_coverage.json
```

The unified CLI can materialize generated skeleton previews from a behavior
report into an isolated review directory:

```bash
cargo run --manifest-path rust/Cargo.toml \
  -p axiograph-cli -- \
  authoring materialize-skeletons \
  --behavior-report build/examples/order_fulfillment_behavior_case_report.json \
  --out-dir build/examples/software_authoring/generated-tests \
  --out build/examples/software_authoring/materialize_skeletons.json
```

Run only the advisory definition-query pass:

```bash
./examples/software_authoring/run_definition_queries.sh
```

Pass an example id to run a different bundled prompt set without requiring a
JSON parser in the shell runner:

```bash
./examples/software_authoring/run_definition_queries.sh \
  build/examples/software_authoring/definitions \
  subscription_billing
```

Run the host-integration metadata and local background processes:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring integration-manifest --workspace .

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring mcp --workspace .

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring lsp --workspace . \
  --axi examples/software_authoring/OrderFulfillmentDomain.axi

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring serve --workspace . --listen 127.0.0.1:8787
```

The files under `host_integrations/` are intentionally generic. Most MCP hosts
expect a block shaped like `mcpServers.<name>.command` plus `args`; most editor
language clients expect a command, args, language id, and document selector.
Use `axiograph authoring integration-manifest` as the canonical generated
contract when adapting these examples to a specific host. MCP framing and tool
lifecycle are owned by the `rmcp`-backed server; editor protocol framing and
capability types are owned by `lsp-server`/`lsp-types`; DB HTTP serving is owned
by the maintained HTTP stack; HTTP callers should use maintained clients against
typed endpoints. Axiograph examples should stay focused on typed semantic
reports and explicit CLI materialization.

## What This Teaches

- `.axi` represents domain meaning; methods and tooling use it but do not get
  embedded into it by default.
- Strong/enforced checks need accepted anchors, typed overlay refs, explicit
  coverage policy, required generated language previews, and no unresolved
  required semantic or runtime-theory obligations.
- Weak definition and coverage tools are useful for exploration, authoring, and
  agent planning, but they do not satisfy promotion gates.
- fDDD context maps become typed overlays over canonical IR ids, so bounded
  contexts can drive behavior cases, semantic slices, and merge/rebase planning
  without becoming domain facts.
- Codegen previews are implementation obligations and planning artifacts, not
  accepted code or proof objects. `codegen-plan` is read-only; materialization
  is an explicit CLI write into a review directory.
- The example crate demonstrates library consumption from an application/domain
  package; the core CLI and typed report schemas remain the reusable contract.
- `AuthoringFlowReportV1` is the shared profile summary embedded by overlay
  software coverage and standalone continuous-check reports, so agents can read
  one `authoring_flow` field for advisory, strict, and CI posture.
- Process-control and host-integration examples are backend-adjacent only in
  the sense that they model implementation surfaces and launch contracts.
  Backend-native stores remain read-only projections from compiled IR, with
  mutation authority and promotion staying in Axiograph.
- MCP and LSP integrations are host-managed background processes. MCP is for
  read-only agent tools; LSP is for editor feedback and code actions; file
  materialization remains CLI-only. These examples launch maintained protocol
  servers; they do not define a custom JSON-RPC dialect.
- Multiple codegen examples intentionally share one suite catalog and runner,
  but the reusable authoring surface is still direct: add canonical `.axi`,
  `.cq` competency questions, direct advisory definition/coverage prompts, a typed
  overlay payload, and behavior-case scenarios. Add a catalog entry only when
  the new domain should join the bundled teaching and verification suite.

## Non-Claims

- Runtime theory admissibility is not Lean certification or a closure claim.
- Definition-query output is not a correctness claim.
- Missing code refs are warnings under the example policy; strict CI can set
  `strict_coverage`, `require_code_refs`, and `require_runtime_theory`.
- Modeling Axiograph tooling inside `.axi` belongs in a future
  `AxiographMeta.axi` self-validation package, not in business examples.
