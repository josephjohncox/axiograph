# Reconciliation

**Diataxis:** Explanation  
**Audience:** contributors

Axiograph has two different reconciliation problems. They must not share an
authority surface.

1. **Evidence reconciliation** compares proposed facts, source credibility,
   confidence, timestamps, and votes. Its output remains evidence until review.
2. **Semantic reconciliation** decides how two reviewed ontology histories
   become one accepted canonical `.axi` state. Its accepted record is
   `axiograph_store::SemReconciliationV2`.

Evidence scores can inform review. They cannot move protected main, rewrite the
accepted tree, or substitute for typed semantic decisions.

## Accepted Semantic Merge

The accepted flow is:

```text
exact left/right .axi closures + independently named branch tips
  -> CanonicalCompiler for each candidate
  -> reviewed typed parent candidates
  -> payload-fingerprint comparison over KernelRefV2
  -> typed keep/drop/introduce/transport decisions
  -> reviewed merged candidate
  -> canonical + CQ + trust + runtime-theory gates
  -> AxiStore::materialize_merge
  -> exact two-parent SemCommitV2 on protected main
```

The runtime merge engine is untrusted. AxiStore is the sole persistence and
lineage authority, and the `VerifyMain` import closure remains the trusted
checker for certificate families it actually imports.

## Why Ref Sets Are Insufficient

A revision-scoped semantic address does not by itself prove that its payload was
preserved. A relation can retain a recognizable local key while its role order,
role kind, indexed/refined type, projection target, or theory obligations
change. The same problem occurs with equations, rewrites, instance carriers,
and facts.

`CompiledKernelSnapshot::payload_fingerprints()` therefore emits one
`KernelPayloadFingerprintV2` for every addressable `KernelRefV2`. The digest
covers the complete compiled payload and its kind. A reconciliation compares
these typed ref/fingerprint pairs, not names or ref membership alone.

## Reviewed Candidates

Each reviewed parent candidate binds:

- its exact semantic commit;
- accepted snapshot and tree;
- root module;
- compiled kernel IR object;
- the complete sorted payload-fingerprint index;
- exact canonical-validation, competency-question, trust, and runtime-theory
  report digests;
- reviewer identity and review time.

The reviewed merged candidate binds the same data except for the future commit
id. The merge commit is derived only after the reconciliation is immutable.

AxiStore does not trust these fields because they deserialize successfully. It
loads every exact `.axi` revision from the accepted tree, recompiles the closure
with `CanonicalCompiler`, reproduces the kernel IR bytes and payload index, and
compares all anchors before materialization.

## Typed Decisions

Every parent payload occurrence and every merged payload has exactly one
accounting decision:

- **keep** — source and target typed refs and payload fingerprints are exactly
  equal;
- **drop** — a reviewed parent payload is explicitly omitted;
- **introduce** — a merged payload has no preserved source and is explicitly
  added; or
- **transport** — a source payload maps to a different typed result and cites
  an immutable witness digest.

`both` origin is used when the same exact payload occurs in both parents. It
covers both occurrences without producing the result twice. Duplicate,
missing, wrong-origin, or unaccounted decisions fail closed.

A transport digest is not self-authenticating mathematical truth. It is a
commitment to evidence that must be checked by the configured checker for the
supported fragment. Unsupported transport remains blocked or must be expressed
honestly as reviewed drop/introduce decisions with the resulting trust scope.

## Gates

Protected main requires exactly four passed gates:

1. canonical validation;
2. competency questions;
3. trust; and
4. runtime theory.

The report digest on each `SemCommitV2` gate must equal the corresponding digest
in the accepted build manifest. The reviewed merged candidate must bind the
same four digests. A same-kind gate pointing at a different report is rejected.

The trust gate normally cites a receipt from the configured trusted checker.
Its scope is only the checked certificate fragment. Co-location with runtime
reports does not turn those reports into Lean proofs.

## Exact Two-Parent Materialization

Generic `promote` rejects merge commits. The only protected-main merge writer is
`AxiStore::materialize_merge`.

It authenticates, in the state-advance transaction:

- the first parent as the exact current `heads/main` tip;
- the second parent as the exact current tip of the named non-main source ref;
- the corresponding ordered snapshot parents;
- the reconciliation's reviewed left/right candidates in the same order;
- the unique maximal common ancestor;
- the reviewed result and all four gates; and
- the complete immutable object closure.

Stale source refs, reversed parents, one-parent merge shapes, ambiguous merge
bases, forged payload indexes, and substituted trust reports reject without
advancing accepted state.

## Evidence Reconciliation

Source weighting, Bayesian updates, voting, temporal decay, and conflict
classification belong to the evidence plane. They may produce proposals,
review queues, or typed evidence attachments. They do not directly create
`SemReconciliationV2` decisions.

A reviewer may cite evidence when choosing a semantic decision, but accepted
meaning still comes from canonical `.axi`, the compiled kernel IR, exact gates,
and the authenticated AxiStore transaction.

## Formal Scope

The implemented merge check is a finite decidable payload-union fragment. It
establishes exact accounting under compiled typed refs and authenticated local
lineage. It does not establish:

- arbitrary categorical pushouts or colimit completeness;
- equivalence of arbitrary category presentations;
- general dependent or Π/Σ transport;
- HoTT univalence, higher inductive types, or arbitrary higher paths;
- open-world entailment or ontology closure;
- source truth or evidence exhaustiveness;
- signatures, author non-repudiation, or hostile-kernel security; or
- correctness of Rust, SQLite, or the filesystem.

Those are separate claims and require separate formalization and trust review.
