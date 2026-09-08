# Software Authoring Tools

**Diátaxis:** Reference
**Audience:** ontology authors, tool integrators, contributors

Axiograph has one typed ontology-authoring service. CLI, LSP, MCP, and HTTP
adapters all deserialize `authoring_workspace_request_v1`, call
`AuthoringWorkspaceService::execute_response`, and share one presentation
contract. The default is compact `authoring_workspace_response_v1`; explicit
`presentation.detail=full` returns the unchanged canonical
`authoring_workspace_report_v1`. The adapters contain no separate authoring
semantics. This service still lives in the CLI binary, not a public embedding API.

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

Every adapter can explicitly return `authoring_workspace_report_v1` with
`"presentation": {"detail": "full"}`. This canonical artifact contains:

- the workspace-relative source and exact ordered module closure;
- repository, candidate snapshot, root revision, and compiled IR digests;
- structured diagnostics;
- query, olog, CQ, and runtime-theory typed holes;
- finite dependent-refinement summaries covering object membership,
  role-indexed witnesses, checked finite constraints, contexts, residuals, and
  lifecycle state;
- one `RuntimeRefinementCandidateV2` repair currency, including deterministic
  source-artifact/theory-obligation handles bound to exact lifecycle,
  obligation, and subject refs;
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

## Source-located canonical errors (bounded EQ-06 / EQ-07)

Unknown **object or relation-object carriers of relation roles**, including bases
inside `indexed(...)` and `refined(...)`, now carry an optional diagnostic
`location`. The compiler remains first-error/fail-fast, not an accumulating
validator. Subtypes, generators, other semantic errors, parse/import-resolution
failures, and unsupported or unverifiable source occurrences remain unlocated.
Absent `location` explicitly means no token-location claim; legacy `path`/`line`
fields alone do not establish token precision.

The location contains the resolved absolute filename, module name, exact source
`revision_digest`, and a **syntactic** schema/relation/role occurrence address
(indices plus labels and object-versus-relation carrier kind). This is not a
checked `KernelRef`, accepted snapshot, or promotion authority. It describes the
same bounded byte image used by the compiler, including an unsaved root override
or disk-backed import; no file is reopened for coordinates or excerpts.

- `byte_start..byte_end`: zero-based, half-open UTF-8 bytes in that exact image.
- `start` / `end`: one-based physical `line` and Unicode scalar `column`, plus
  zero-based `lsp_line` and UTF-16-code-unit `lsp_character`.
- `excerpt`: at most 240 Unicode scalars of the original physical line, without
  CR/LF; `excerpt_byte_start` locates its start and `excerpt_truncated` declares
  clipping. CRLF byte lengths are preserved in source ranges.
- `suggested_name`: advisory only, or null. Suggestions consider the actual
  failing schema's object or relation namespace, require a unique nearest name
  within two edits, and refuse ties. Work is bounded to 4,096 declarations and
  names/targets of at most 128 UTF-8 bytes (targets shorter than four are not
  suggested). Suggestions never write, repair, or promote the source.

The parser captures carrier slices during its existing recursive parse and maps
multiline declaration segments back to original byte ranges. A monotonic segment
cursor bounds mapping work to O(carriers + segments), including ordinary parses
that discard the map. Comments, repeated spellings and synthetic join spaces are
not searched for a matching token.
Source maps live outside accepted AST, kernel IR, semantic digests and checker
or certificate serialization. Valid semantic identities and wire payloads are
unchanged. Canonical-invalid full/compact reports retain failed validation and
promotion blockers; failed closure compilation still issues no paging cursors.

Rust error consumers inspect `KernelCompileError::RoleCarrier.cause` directly
for the original unknown-target leaf and its structured context. CLI consumers
inspect `CanonicalSourceDiagnostic.cause` and `location`. These wrappers preserve
the original Display text and do not repeat identical semantic messages through
`Error::source()`; outer operation/filename contexts remain in pretty chains.
Only these two leaf classes are wrapped. Unrelated compiler errors retain their
original anyhow type and chains; generic source-chain downcasting is not the
interface for accessing the supported role-carrier cause.

LSP publishes located errors to their owning file URI using those UTF-16 ranges.
Unlocated errors use an empty required LSP range with `data.sourceLocated=false`;
that placeholder is not a source location. Edits/close clear prior publications,
including imported-file owners. Imports remain disk-backed: if an imported file
has a different open buffer, its disk coordinates are not applied to that buffer;
the importing document receives an explicitly unlocated diagnostic instead.
File URI aliases are compared by the same workspace-validated canonical path,
not URI spelling. A unique matching open image retains its client-facing URI;
multiple simultaneously open aliases are ambiguous and remain unlocated.
Editing an imported buffer invalidates previous disk-backed ranges; revalidate
importers after saving. A byte/count-rejected open or edit removes any cached
revision for that client URI and invalidates all precise session publications.
A single sticky flag (not an unbounded rejected-URI map) then keeps diagnostics
explicitly unlocated until the LSP session restarts. Compilation failures remain
visible; this conservative fallback does not turn rejected editor text into an
import overlay or a successful validation. New unsaved files, unsaved import
overlays and broad multi-error/editor workflows remain unsupported; EQ-06/EQ-07
are partial.

Runnable read-only Company/Compny error example (temporary workspace, no store):

```bash
cargo build --manifest-path rust/Cargo.toml --locked --offline -p axiograph-cli
bash examples/software_authoring/run_source_diagnostics.sh
```

The existing CLI transport exits successfully when it emits the report; consumers
must inspect `ok=false`, validation and promotion blockers. The example reports
`Company.axi`, line 5, columns 32–38, the `Compny` token and advisory `Company`.
It intentionally does not automatically correct the fixture.

## Compact presentation and follow-up pages

Omitting `presentation` defaults to `summary` on CLI, MCP, HTTP, and LSP
execute-command responses. Diagnostics notifications still use the canonical
report internally. Existing software-authoring and regulated-shipment machine
request fixtures explicitly select `full` to preserve their consumers.

```json
"presentation": {
  "detail": "standard",
  "sections": ["diagnostics", "stable_runtime_refs"],
  "limit": 20
}
```

- `summary`: aggregate status with no selected artifact items.
- `standard`: the same aggregates plus the first diagnostics and repairs pages.
- `full`: the canonical full report, not an envelope; no sections or cursor allowed.
- `sections`: a typed, duplicate-free selection overriding the preset; `[]`
  explicitly selects none. Unknown sections/fields fail. The inventory is:
  `diagnostics`, `stable_runtime_refs`, `repairs`, `olog_holes`, `query_holes`,
  `competency_holes`, `theory_holes`, `dependent_refinements`, `evolution_previews`,
  `competency_questions`, `runtime_theory_reports`, `validation`, `prepared_query`,
  `query_explanation`, `applied_query_repair`, `checked_olog`, `applied_olog_repair`,
  `competency_evaluation`. Optional singleton artifacts are zero-or-one-item pages.
- `limit`: positive integer, 1–100, default 20, per selected section. Negative,
  fractional, zero, and oversized limits fail; full artifacts are not paginated.

Every compact response retains overall `ok`, error/warning totals, source identity,
exact ordered closure, trust/non-claims, finite coverage, runtime and dependent
residual totals, CQ totals/gate, the complete promotion review/blockers, and next
actions. These aggregates never depend on which items are visible. `source=null`
means compilation failed; `requested_path` and `input_identity` still identify the
request and exact root bytes, **not** an accepted compiler handle.

`sections` always inventories all sections with `selected`, `total`, `offset`,
`returned`, `omitted`, `truncated`, `items`, and `next_cursor`. Omitted counts are
collection entries, not deduplicated ontology subjects; artifacts can overlap.
Top-level `omitted_items` and `truncated` make hidden material explicit. A summary's
omitted nonempty sections carry an offset-zero cursor, so the first detail request
can be bound to the summary. To follow it, repeat the original semantic request
with exactly that section and its cursor. Each selected page supplies the next
cursor until exhausted. Concatenating pages gives the canonical collection order.

Cursors are stateless, versioned SHA-256 **consistency bindings**, not authentication,
secrets, proof receipts, or accepted handles. Clients can recompute them; a valid
in-range offset only selects an otherwise accessible read-only page. They commit to
canonical workspace root, exact candidate and baseline import closures, CQ bytes,
and all semantic request fields. Every follow-up reruns ordinary path, source,
resource, compiler, and promotion checks. Source edits (including unsaved buffers
and byte-only import edits), changed CQ/baseline/query/request/workspace/section,
malformed/oversized/overflow cursors, invalid checksums, and out-of-range offsets
fail closed. Detail and limit may vary; section must match the cursor, and the
cursor itself and other presentation choices are excluded from input identity.
The ordering/binding version is `authoring-page-v1`; changing collection order
requires changing this version. Compilation failures cannot establish the closure:
`follow_up_available=false`, no cursor is issued, and existing cursors are rejected.
An unbound fresh request or explicit full report can still inspect those failures.

Pagination bounds **entries**, not the byte size of an indivisible canonical
artifact. Selected validation/query/evolution objects can contain large nested
collections; request them deliberately or use explicit full. HTTP retains its
16 MiB response bound and protocol input limits remain unchanged. Compact projection
serializes only selected slices, not a full JSON tree followed by key deletion;
it still performs full semantic evaluation. This is a payload improvement, not a
CPU/memory scalability claim. The regression compares `serde_json::to_vec` on both
real full and summary fixture reports (minified UTF-8) with a 12,000-byte summary
ceiling and >90% reduction. This fixture ceiling is not a universal workspace bound.

### Import-aware derived query projection

Schema-only imports and dependent instances now use the reusable PathDB
`axi_module_import::derive_package_query_index(snapshot, exact_sources)` seam.
It returns a fresh in-memory DB, named meta-plane, runtime citation index, import
summary, and runtime-fact-ID → canonical-fact-ID citations. It does not read or
publish stores, hydrate a bare image, mint receipts, or compile a second source
of semantic authority. Full workspace authoring remains inside the CLI; this is
not a public workspace embedding API.

For example, `Base.axi` may contain only:

```axi
module Base
schema Shared:
  object Person
  relation Parent(child: Person, parent: Person)
```

An `Extension.axi` can import it and provide the data:

```axi
module Extension
import Base
instance Family of Shared:
  Person = {Alice, Bob}
  Parent = {p: (child=Alice, parent=Bob)}
```

Named AxQL such as `select ?x where ?x is Shared.Person limit 10` and CQs over
`Shared.Parent` use the imported schema metadata. Entirely metadata-only packages
are supported with zero imported data instances, not a fabricated data success.
All schema metadata is installed before theories and instance data; imported
theories attach to the canonical referenced schema while retaining their owner.
Instances retain their original module provenance and runtime fact-ID spelling.
Canonical schema/instance IDs select declarations; canonical fact references link
actual allocated fact nodes, including forward references, never same-named
placeholder objects. The shared importer previously made such placeholders for
relation-valued roles; two-phase allocation/linking corrects that derived graph
defect without changing accepted bytes or canonical IDs.

The named execution adapter is deliberately restricted and fail-closed:

- Schema, theory and instance labels must be globally unique in the projection.
  The canonical compiler already rejects duplicate visible schema labels; it can
  accept repeated theory/instance labels that this derived adapter cannot encode.
- Object labels may repeat across distinct schemas; qualify queries with
  `Schema.Type`. Existing unqualified type queries retain their union behavior.
- Execution types must not collide with tuple types, the `AxiMeta*` namespace,
  virtual `Morphism`/`Homotopy` types, or dotted qualification syntax. Execution
  arrows/roles may not use `axi_*` or dotted names. Relation/generator labels
  within a schema may not collide; unique convenience arrows cannot collide with
  role or virtual projection labels. Repeated relation labels across distinct
  schemas use the existing `Schema.Relation` execution labels. Repeated execution
  labels involving any explicit generator (relation/generator or generator/generator)
  reject before DB construction: named AxQL qualification resolves relations only.
- Explicit generators with relation-object endpoints remain unsupported. Uniquely
  named object generators use unqualified execution labels; subtype membership
  retains existing behavior. Distinct canonical
  facts that collapse to one runtime fact ID reject rather than silently merge.
- Opaque/review-only constraints remain visible residuals, not checked proofs.

Capability rejection is `authoring_query_projection_unsupported`, not a canonical
compiler failure: full and compact reports retain the exact canonical source
anchor and canonical-valid flags, remain `ok=false`, and block promotion. No
partial DB escapes. General identity-aware namespaces remain open under EQ-04.

Exact root/import bytes, ordered closure anchors, bounded discovery, and unsaved
root handling are unchanged. The incoming source slice may be permuted: membership
and exact revisions must match, and the canonical closure determines execution
order. Missing, extra, duplicate or changed sources reject. Candidate/baseline
schema-only import edits, including comments-only changes, invalidate cursors.

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
remain explicit residual obligations. Theory-hole diagnostics identify their
runtime status, exact subjects, lifecycle, residuals, and repair handle ids.
They also state `untrusted_rust_runtime_admissibility` and `not_certified`
explicitly: a repair handle is source-directed runtime guidance, not a proof
term or a `VerifyMain` receipt.

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

Omitted CQs block the CQ gate. Every non-checked runtime-theory status blocks the theory gate: review-only,
evidence-excluded, residual, blocked, resolver-required transport, or nonempty
residual-id state. The trusted-checker gate remains blocked because the
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

Compact review and explicit collection/full switches (override request presentation):

```bash
axiograph authoring workspace --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json --detail summary
axiograph authoring workspace --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --detail standard --section diagnostics,stable_runtime_refs --page-limit 10
# Follow one emitted section cursor with the identical semantic request:
axiograph authoring workspace --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json \
  --detail summary --section stable_runtime_refs --page-limit 10 --cursor "$CURSOR"
# Canonical machine artifact, byte-identical to the pre-projection full report:
axiograph authoring workspace --workspace . \
  --request examples/software_authoring/authoring_workspace_request.json --detail full
```

For an executable HTTP client that prints a summary then concatenates bounded
stable-reference pages, start `authoring serve` as below and run:

```bash
python3 examples/software_authoring/compact_authoring_client.py \
  --request examples/software_authoring/authoring_workspace_request.json
```

Inspect adapter capabilities and launch metadata:

```bash
axiograph authoring lsp-capabilities --out authoring_capabilities.json
axiograph authoring integration-manifest --workspace . --out authoring_integrations.json
```

`discover behavior-case` computes `RuntimeTheoryCheckSummaryV1` from the same
canonical input and embeds its nested scope, trace, transport summary,
residual ids, and structured non-claims. It rejects request-supplied replacement
summaries. Continuous coverage consumes this shared current shape and fails
closed on every non-checked status.

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

Its input schema is `authoring_workspace_request_v1`; its default structured result
is `authoring_workspace_response_v1`. Explicit full requests return
`authoring_workspace_report_v1`; invalid requests return `{ "error": "..." }`
with MCP `isError=true`. Output schemas close and type-check the compact envelope,
aggregates, source/trust/promotion metadata, page metadata, and full top-level
fields. Selected canonical artifact objects and most full nested domain artifacts
are deliberately opaque in this schema. Offline tests use the maintained
`jsonschema` validator on real successful/failed modes and malformed fixtures.
This is **partial schema coverage**, not complete generated OpenAPI/domain SDK
coverage. The former per-feature authoring MCP tools were removed rather than
retained as compatibility shims.

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
