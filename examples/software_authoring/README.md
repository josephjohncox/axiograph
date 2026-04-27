# Software Authoring With Pure Domain AXI And Tooling Overlays

This example demonstrates ontology-driven software authoring without polluting
the domain representation. The `.axi` file models order-fulfillment domain
facts and theory only. DDD/fDDD context maps, behavior planning, implementation
surfaces, code refs, coverage policy, and codegen live in JSON tooling overlays.

## Files

- `OrderFulfillmentDomain.axi` is the pure canonical domain ontology.
- `order_fulfillment_tooling_overlay.json` maps the ontology to fDDD context,
  implementation surfaces, code refs, coverage policy, and codegen hints.
- `order_fulfillment_behavior_case.json` is a domain-only behavior case.
- `order_fulfillment_coverage_query.json` is a weak/advisory coverage query.
- `order_fulfillment_definition_queries.json` contains weak definition prompts
  for authoring and agent planning.
- `SubscriptionBillingDomain.axi` demonstrates API/worker codegen planning for
  paid invoices, product access, and entitlement grants.
- `ProcessControlDomain.axi` demonstrates ERP, simulator, HMI, PLC, and
  process-control coverage without embedding tooling concepts in `.axi`.
- `software_authoring_examples.json` is the typed suite catalog for all
  software-authoring/codegen examples.
- `example_registry.sh` keeps the shell runners parser-free; the JSON fixtures
  remain the typed machine-readable examples consumed by Axiograph commands and
  tests.
- `host_integrations/` contains generic stdio launch examples for MCP and LSP
  hosts such as Cursor, Codex, Claude Code, and editor language-client plugins.

## Teaching Path

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

The shell runners do not require an external JSON parser or adapter script.
They call the Rust CLI/library surfaces directly, including `authoring tool-specs`,
`authoring lsp-capabilities`, `authoring integration-manifest`, and
`authoring codegen-plan`.

Validate the domain ontology:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/software_authoring/OrderFulfillmentDomain.axi
```

Check theory closure for the supported runtime fragment:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check theory examples/software_authoring/OrderFulfillmentDomain.axi \
  --closure-tier finite_fragment
```

Ask a weak definition question. This is useful for discovery and authoring, not
for correctness gates:

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

Run an exploratory coverage query:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover coverage-query examples/software_authoring/OrderFulfillmentDomain.axi \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --query examples/software_authoring/order_fulfillment_coverage_query.json
```

Generate a behavior-case report from domain behavior plus overlay tooling:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover behavior-case examples/software_authoring/OrderFulfillmentDomain.axi \
  --request examples/software_authoring/order_fulfillment_behavior_case.json \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/order_fulfillment_behavior_case_report.json
```

Run continuous software coverage through the core CLI:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check software-coverage examples/software_authoring/OrderFulfillmentDomain.axi \
  --behavior-case examples/software_authoring/order_fulfillment_behavior_case.json \
  --overlay examples/software_authoring/order_fulfillment_tooling_overlay.json \
  --out build/examples/order_fulfillment_software_coverage.json
```

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

Run only the weak definition-query pass:

```bash
./examples/software_authoring/run_definition_queries.sh
```

Pass an example id to run a different bundled prompt set without requiring a
JSON parser in the shell harness:

```bash
./examples/software_authoring/run_definition_queries.sh \
  build/examples/software_authoring/definitions \
  subscription_billing
```

Run the host-integration metadata and local background processes:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring integration-manifest

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring mcp

cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  authoring lsp
```

The files under `host_integrations/` are intentionally generic. Most MCP hosts
expect a block shaped like `mcpServers.<name>.command` plus `args`; most editor
language clients expect a command, args, language id, and document selector.
Use `axiograph authoring integration-manifest` as the canonical generated
contract when adapting these examples to a specific host.

## What This Teaches

- `.axi` represents domain meaning; methods and tooling use it but do not get
  embedded into it by default.
- Strong/enforced checks need accepted anchors, typed overlay refs, explicit
  coverage policy, and no unresolved required obligations.
- Weak definition and coverage tools are useful for exploration, authoring, and
  agent planning, but they do not satisfy promotion gates.
- fDDD context maps become typed overlays over canonical IR ids, so bounded
  contexts can drive behavior cases, semantic slices, and merge/rebase planning
  without becoming domain facts.
- Codegen previews are implementation obligations and planning artifacts, not
  accepted code or proof objects.
- The example crate demonstrates library consumption from an application/domain
  package; the core CLI and typed report schemas remain the reusable contract.
- MCP and LSP integrations are host-managed background processes. MCP is for
  read-only agent tools; LSP is for editor feedback and code actions; file
  materialization remains CLI-only.
- Multiple codegen examples intentionally share one suite manifest and runner,
  so adding another domain should mean adding canonical `.axi`, overlay JSON,
  behavior-case JSON, definition prompts, and a manifest entry.

## Non-Claims

- Runtime theory closure is not Lean certification.
- Definition-query output is not a correctness claim.
- Missing code refs are warnings under the example policy; strict CI can set
  `strict_coverage`, `require_code_refs`, and `require_runtime_theory`.
- Modeling Axiograph tooling inside `.axi` belongs in a future
  `AxiographMeta.axi` self-validation package, not in business examples.
