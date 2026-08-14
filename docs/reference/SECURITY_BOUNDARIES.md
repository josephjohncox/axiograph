# Security Boundaries

**Diataxis:** Reference
**Audience:** contributors, operators, adapter authors

This document defines the resource and authority boundary for untrusted runtime
input. It does not enlarge Axiograph's semantic trust boundary. Accepted
canonical `.axi` bytes and their compiled snapshot remain the meaning plane;
Lean checks only the certificate families imported by `VerifyMain`.

## Security Contract

Axiograph applies four rules at an untrusted boundary:

1. Bound bytes, item counts, nesting, depth, time, and concurrency before an
   operation can consume unbounded resources.
2. Read or hash one opened byte image. Do not inspect a pathname and later
   reopen it as the purported same input.
3. Publish files through a private, bounded temporary file and an atomic rename.
   Reject symlinked destinations and special files.
4. Fail closed. An invalid limit, unsupported format version, unknown verified
   flag, saturated queue, unpinned network peer, malformed frame, or incomplete
   receipt is an error rather than a fallback.

The shared implementation lives in `rust/crates/axiograph-security`. It is
semantics-free so the CLI, store, ingestion, tooling, examples, and sync crates
use the same file, JSON, and process primitives.

## Boundary Matrix

| Surface | Enforced boundary |
| --- | --- |
| Regular-file input | Open the final component with no-follow/reparse-point handling; verify regular-file type and size on that handle; stream at most `limit + 1` bytes. |
| Canonical `.axi` | 4 MiB per CLI module, plus line, line-length, syntax-depth, import-count, search-entry, search-depth, and import-closure limits. |
| JSON | Call-specific byte limit, at most 128 nested containers before Serde allocation, then type-specific item/string/aggregate validation. |
| Verified CBOR | 64 MiB envelope, explicit recursion limit 64, exact schema version, zero unknown flags, no trailing value, and type-specific collection/counter/weight limits. |
| Protobuf descriptors | Bounded descriptor bytes, file/node/nesting counts, and bounded external `buf` execution. |
| General CLI output | At most 64 MiB, written to a private `0600` temporary file, synced, and atomically renamed. Larger verifier staging uses its separate 256 MiB executable limit. |
| Child processes | Positive timeout no greater than 10 minutes, bounded stdin/stdout/stderr, at most 16 concurrent children, and descendant termination through a Unix process group or Windows job object. |
| Public HTTP | No ambient proxy or automatic redirect; bounded DNS and response bytes; all DNS answers must be public; requests pin those answers and verify the connected peer. Every redirect is reparsed and revalidated. |
| Local Ollama HTTP | `localhost` or an IP loopback only; bounded DNS; pinned loopback peer; no proxy or redirect. Public OpenAI/Anthropic endpoints require HTTPS and the public-address policy. |
| GitHub clone | Exact credential-free `https://github.com/owner/repository`; public DNS pinning through Git/cURL, redirects and proxies disabled, protocols restricted to HTTPS, ambient Git configuration neutralized, hooks disabled, no submodules, and option-safe refs. |
| Incoming HTTP/MCP/LSP | Hard connection, worker, queue, request, header, body, response, frame, document, and aggregate-document limits. Saturated semaphores reject instead of waiting without bound. |
| Directory traversal | No link following; explicit entry, depth, file, aggregate-byte, chunk, proposal, and edge limits for canonical imports, repository indexing, directory ingestion, and viz asset copying. |
| Immutable `.axpd` | Read and authenticate one bounded immutable byte image; hash and SQLite-deserialize that same image; validate receipt, header, schema, page/count/string/fanout limits before PathDB hydration. |
| Mutable AxiStore catalog | Real final root directory; catalog opened through the canonicalized root with SQLite no-follow; bounded page/catalog/journal sizes, strict schema, `trusted_schema=OFF`, foreign keys, FULL synchronization, and authenticated transactional state/audit invariants. |
| Release archives | No-follow source open, one private snapshot used for hash and parse, strict exact member set and metadata, stored-only ZIP entries, bounded tar/ZIP members and expansion, canonical JSON, and dirfd-relative exclusive no-follow extraction. |

The numbers above are hard maxima, not recommended operating sizes. Narrower
call sites use smaller limits. User-supplied values outside a documented range
reject; code must not silently clamp them.

## Filesystem Scope And TOCTOU

`open_regular_file_bounded` guarantees final-component no-follow behavior and
same-handle type/size validation. It does not decide whether a parent directory
is authorized. Workspace, repository, store, and extraction callers must still
confine paths to their declared root.

`write_file_atomic_bounded` requires an existing real parent directory. It
rejects an existing symlink or special-file destination, writes and syncs a
private file, renames it over the destination, and syncs the parent on Unix.
The rename replaces a raced destination entry; it does not follow that entry.

These primitives do not make an attacker-writable parent directory safe. A
caller that needs authority or confinement must use a directory it owns and
whose parent chain cannot be renamed by an adversary. AxiStore roots and release
extraction destinations are operator-controlled directories.

The verifier bridge closes the hash-to-exec window separately. It reads and
hashes the approved executable once, copies those exact bytes into a private
staging directory, and executes that staged copy. The configured pathname is
never the executable later passed to `Command`.

## Network Classes

Axiograph distinguishes two outbound classes:

- **Public network adapters:** web ingestion, OpenAI, Anthropic, predictive
  proposal HTTP, and GitHub clone. They require public DNS answers and a peer
  address from the pinned answer set. Public LLM and proposal endpoints require
  HTTPS.
- **Explicit local adapter:** Ollama. It accepts only loopback endpoints and
  verifies that the connected peer is one of the pinned loopback answers.

Automatic redirects and environment proxies are disabled in both classes.
Web ingestion follows a small number of redirects itself, validating and
repinning each target. The other adapters do not follow redirects.

This policy deliberately does not treat arbitrary RFC 1918, link-local,
metadata-service, `.local`, or `.internal` addresses as public HTTP targets. A
local custom proposal service should use the bounded command adapter rather
than the public HTTP adapter.

## Deserialization And Allocation

A byte limit alone is not a structural limit. Each format adds the limits its
shape requires:

- JSON checks container depth before deserialization and validates decoded
  vectors, strings, dimensions, and aggregate counts.
- Verified CBOR checks the envelope and content independently, rejects trailing
  CBOR, and validates reconciliation collections, nested evidence, counters,
  and finite weights.
- Embeddings validate item count, dimensions, finite components, metadata, and
  response cardinality/index uniqueness.
- Query and theory inputs bound text, disjuncts, atoms, variables, regex nodes,
  hops, result rows, candidate assignments, finite objects/arrows, and
  explanation depth.
- Repository, document, conversation, RDF, protobuf, overlay, CQ, and proposal
  adapters impose format-specific collection and aggregate limits.

Unsupported verified versions and nonzero flags reject. There is no legacy
reader fallback at a verified boundary.

## Saturation And Query Scope

Two different operations must not be conflated:

- `RuntimeTheoryCheckReportV1` performs a bounded admissibility scan over the
  compiled obligation list. It does not saturate arbitrary theory rules.
- `category_kernel_v3` reconstructs a finite category presentation and performs
  bounded generator-reachability saturation over at most 64 objects and 4,096
  arrows, with equations, congruence, exact formal inverse-law cancellation
  traces, and explanations checked by Lean. Certificate bytes remain subject to
  the verifier's global size and JSON-nesting limits.

AxQL, prepared-query, HTTP query, and certified query paths have hard limits on
query text and structure, path repetition, assignments, concurrency, and result
rows. Reaching a limit produces an error or an explicit truncation state; it
never authorizes a completeness claim outside the declared finite certificate
family.

## Unsafe-Code Policy

Every first-party workspace crate inherits `unsafe_code = "forbid"`. The
checked-in Rust source contains no first-party `unsafe` block or function. Run:

```bash
make check-no-unsafe
```

This is a first-party policy. It is not a claim that transitive dependencies use
no unsafe implementation internally.

## Operator Obligations

Operators must provide:

- a non-attacker-writable workspace/store/output parent when the operation has
  mutation authority;
- explicit approved verifier SHA-256, build id, and positive timeout;
- TLS and system trust-store policy appropriate for public endpoints;
- bounded CLI/server settings within the accepted ranges; and
- a successful release gate for the exact platform artifact being published.

A neighboring checksum file is package metadata, not verifier approval. A
successful Rust check, runtime report, archive validation, or SQLite receipt is
not a Lean proof and does not promote evidence into accepted ontology state.

## Focused Regression Commands

From the repository root:

```bash
cargo test --manifest-path rust/Cargo.toml -p axiograph-security
cargo test --manifest-path rust/Cargo.toml -p axiograph-cli --bin axiograph
cargo test --manifest-path rust/Cargo.toml -p axiograph-llm-sync
cargo test --manifest-path rust/Cargo.toml -p axiograph-store --test materialization
python3 -m unittest scripts.tests.test_validate_release_archive
make check-no-unsafe
make verify-viz
make verify-rustsec
make verify-fuzz
make verify-loom
```

`make verify-viz` runs under Node.js 24.19.0, installs the exact npm lock with
lifecycle scripts disabled, rejects moderate-or-higher advisories, and builds
the production bundle. This blocks the release decision on known frontend
supply-chain findings rather than relying only on default-branch alerts.

`make verify-rustsec` runs cargo-audit 0.22.2 over the exact workspace and
isolated fuzz lockfiles.
The gate permits only RUSTSEC-2026-0194 and RUSTSEC-2026-0195 for the
quick-xml 0.37 parser that Sophia 0.10 reaches through oxrdfxml. Every RDF/XML
byte sequence first passes quick-xml 0.41 over the exact same bytes. That
preflight uses checked attribute iteration, a per-element limit of 64
attributes, the patched namespace declaration limit, a depth limit of 128, an
event limit of 1,000,000, and the existing 8 MiB input limit before Sophia can
see the input. The attribute cap also bounds the older parser's duplicate-check
work on otherwise unique attributes. Adversarial tests cover duplicate and
excess attributes, namespace floods, and excess depth. No other RustSec
vulnerability is allowed. The remaining ttf-parser notice is an informational
unmaintained warning in pdf-extract's lopdf dependency, not a vulnerability.

`make verify-fuzz` uses checked seed corpora and exact nightly/cargo-fuzz
versions. It exercises canonical `.axi`, strict Certificate V2/V3 JSON,
production REPL tokenization, the unknown-field-strict adapter envelope and
bounded/validated predictive-proposal payload shared by command and HTTP, and
authenticated `.axpd` image opening. A cold build has its own 600-second
process-group limit; fuzz execution has hard input, run-count, per-case time,
process time, per-stream output, RSS, and single-crash-artifact bounds. The
POSIX driver kills the dedicated process
group even when the direct child exits first. The working corpus is temporary,
so a verification run cannot rewrite accepted seed artifacts. Release
verification runs this lane on Ubuntu before platform bundle jobs start.

`make verify-loom` explores the real atomic child-slot reservation algorithm
under contention. It checks that concurrent acquisition never exceeds the hard
maximum and that every successful reservation is released.

The focused suites cover symlinks and special files, unknown-length stream
overflow, atomic output publication, JSON/CBOR depth and trailing values,
process output floods/timeouts/descendants/concurrency, URL and redirect policy,
DNS peer pinning, Git URL/ref injection, MCP/LSP frame limits, `.axpd`
corruption/substitution/limits, and archive metadata/extraction attacks.

## Explicit Non-Claims

These controls do not claim:

- memory safety or correctness of every transitive dependency;
- confinement against an attacker who can rename an authorized parent
  directory while the process is using it;
- general sandboxing of plugins beyond process, I/O, time, and concurrency
  limits;
- protection from a compromised operating system, DNS resolver, CA store, Git
  executable, Lean checker binary, or hardware;
- semantic correctness of Rust runtime output;
- open-world completeness, ontology closure, rewrite confluence, arbitrary
  dependent type theory, univalence, higher inductive types, or unrestricted
  HoTT; or
- authority for PathDB, `.axpd`, projections, LLM output, embeddings, runtime
  reports, or release metadata to redefine accepted `.axi` meaning.

For semantic trust, see `docs/reference/TRUSTED_KERNEL.md`. For runtime theory
scope, see `docs/reference/RUNTIME_THEORY_CHECKER.md`.
