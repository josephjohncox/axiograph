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
| `no_unsafe_external_cache_manifest_v1.json` | Closed semantic Kani distribution inventory for the no-unsafe ownership rule; JSON whitespace and object-member order are insignificant. |
| `generate_no_unsafe_external_cache_manifest.py` | Bounded offline archive verifier and descriptor-confined exclusive evidence-file generator. Supported global PAX values persist. A local path overrides the global path for one ordinary member. Unsupported semantic and sparse PAX keys fail closed. |
| `no_unsafe_row_cases_v1.json` | Closed assignments for all 81 normative no-unsafe regression rows and their required execution levels. |
| `run_no_unsafe_row_evidence.py` | Executes every assignment separately for its row and records commands, allowed environments, exits, output, subtests, and actual passing assertion calls. It rejects stored observations that do not match execution. |
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
