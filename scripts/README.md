# Scripts

`scripts/` contains operational demos, stress runners, and developer utilities.
The user-facing teaching path starts in `examples/README.md` and
`examples/catalog.json`.

Use this directory when you need to exercise a specific runtime seam:

- server/API demos that necessarily send JSON HTTP request bodies,
- REPL/debug demos that intentionally show lowered AxQL,
- performance runners over synthetic PathDB snapshots,
- proposal-adapter and bounded-rollout research runners,
- ops-style offline demos.

Top-level script roles:

| Script | Role |
| --- | --- |
| `db_server_llm_viz_demo.sh` | Local LLM/viz sandbox over accepted-plane state plus synthetic evidence overlays. |
| `approximate_tacit_query_demo.sh` | Synthetic approximate-query/confidence-filter debug demo. |
| `predictive_proposal_demo.sh` | Predictive-proposal adapter and planning/eval runner over a small ontology. |
| `check_no_unsafe.py` | Filesystem-complete first-party Rust scan with exact-EOF bounded directory reads, declared-byte precharge, and two byte-pinned external cache exceptions. |
| `check_documentation_claims.py` | Checks complete claim-status declarations, roadmap marker identities and states against an independently pinned baseline, accepted release identity, trust wording, and visible required Markdown links in each applicable documentation index. |
| `no_unsafe_external_cache_manifest_v1.json` | Closed semantic Kani distribution inventory for the no-unsafe ownership rule; JSON whitespace and object-member order are insignificant. |
| `generate_no_unsafe_external_cache_manifest.py` | Bounded offline archive verifier and descriptor-confined exclusive evidence-file generator. Supported global PAX values persist. A local path overrides the global path for one ordinary member. Unsupported semantic and sparse PAX keys fail closed. |
| `no_unsafe_row_cases_v5.json` | Digest-pinned cases for all 81 rows in the pinned no-unsafe regression matrix. Each local predicate fixes one row-bound invocation, exact stimulus, reviewed level, source assertion-site occurrence, and pinned arguments. Present normative causes must occur in those arguments and match the same matrix row. Occurrences are unique across the map, and redundant predicates are removed. TYPE-05 combines local FIFO/socket CLI and zero-data-open cases with the hosted device proof. CHECK-03 combines local link/non-directory/escape CLI cases with the hosted mount proof. |
| `run_no_unsafe_row_evidence.py` | Executes each unique row-bound invocation and derives case records from observed passing assertions. It records repeated calls separately and consumes each matched occurrence once. It validates matrix and map identities, method hashes, stimuli, levels, assertion sites and arguments, actual child commands, and the hosted receipt. This is finite operational evidence, not proof of arbitrary Python semantics or transcript authenticity. |
| `verify_no_unsafe_row_evidence.py` | Reads a schema-v5 evidence package and the fixed reviewed case map, revalidates every case and source binding, and emits a digest summary without rewriting the package. `--negative-checks` requires malformed variants of actual executed evidence to reject, including duplicate reuse and a map-valid semantic wrong-cause mutation with map identity disabled. |
| `perf_*.sh` | Release-mode performance runners over synthetic derived PathDB workloads. |
| `*_demo.sh` | Operational or research integration demos; check the script header before treating one as a teaching flow. |

Do not treat every script here as a canonical authoring example. New teaching
flows should prefer:

- canonical `.axi` modules for domain representation,
- `.cq` files for competency-question authoring,
- tooling overlays for DDD/fDDD, BDD, implementation surfaces, coverage policy,
  and codegen,
- typed reports as the boundary between tools, MCP/LSP, CI, and agents.

Lowered API JSON and lowered AxQL are acceptable in this directory only when
they are the actual server/debug surface being demonstrated. They should not be
introduced as the default way to author ontology, CQ, coverage, or
software-engineering semantics.
