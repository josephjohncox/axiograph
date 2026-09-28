# Documentation Claim Status

**Diataxis:** Reference
**Audience:** users and contributors
**Claim status:** `current_implementation`

This page defines the status labels for current Axiograph documentation. A claim
inherits the nearest **Claim status** declaration. An inline status label overrides
the inherited label.

## Status Labels

| Label | Meaning | Required source |
| --- | --- | --- |
| `design_target` | A planned capability or acceptance condition. It is not current behavior. | A roadmap requirement or design page. |
| `current_implementation` | Behavior present in the current source tree. | A current reference plus an executable gate or source path. |
| `runtime_check` | A bounded result from Rust, a script, or another untrusted runtime. | The command, tested source identity, limits, and result. |
| `operational_evidence` | Build, package, release, receipt, or deployment integrity evidence. | An immutable receipt or an accepted release record. |
| `trusted_formal_result` | A result checked inside the `VerifyMain` Lean import closure. | The checker path, certificate kind, theorem scope, and exact input anchor. |
| `historical_baseline` | A point-in-time observation retained for audit history. | The original source identity and date or release identity. |

A checklist marker is not a trust label. The marker records implementation
progress under that roadmap's rules. Parent acceptance controls marker changes
when the roadmap requires it.

`current_implementation` does not mean `trusted_formal_result`. A Rust type, an
AxiStore receipt, or a passing runtime test cannot become a Lean result through
wording. A `trusted_formal_result` also does not remove runtime resource limits.

## Required Scope

Each current claim must identify its status directly or inherit it from its
section. Mixed-status pages must state which sections use each label.

A runtime result must name its executable gate and tested source. A bounded
fixture result applies only to that fixture and bound. A skipped or blocked
prerequisite is not a pass.

A formal result must name the checked fragment. It must not imply general DTT,
HoTT, ontology closure, open-world completeness, or reversible data semantics.
The authoritative formal boundary is the import closure of
`lean/Axiograph/VerifyMain.lean`. See [Trusted Kernel](TRUSTED_KERNEL.md) and
[Lean Theory Evaluation](LEAN_THEORY_EVALUATION.md).

Operational evidence must stay separate from semantic proof. The accepted
[v20260908.0.0 baseline](RELEASE_BASELINE_V20260908.md) authenticates one release.
It does not prove query semantics or authorize another publication.

Historical evidence must remain identifiable as historical. The
[70c568b audit](ENGINEERING_AUDIT_70C568B.md) records its original source state.
Current roadmaps can add dated correction notes, but they must not rewrite the
baseline observation.

## Authority Boundaries

Canonical accepted `.axi` bytes are the reviewable meaning plane. The compiled
IR is the shared Rust semantic package. Rust remains outside the trusted formal
boundary.

SQLite is the only durable `.axpd` format. AxiStore is the only publication and
opening authority for durable `.axpd` images. Receipts are opaque operational
credentials, not semantic certificates.

PathDB and external graph systems are derived execution or projection substrates.
Retrieval scores, model output, browser state, and backend readback remain evidence.
They cannot authorize accepted-state mutation.

## Validation

Run `make verify-doc-claims` after a claim-label or roadmap-status change. The
gate checks complete page declarations; each requirement's marker identity and
authorized state against an independently pinned digest of the frozen keyed
ledger; the accepted release identity; trust-boundary text; and visible Markdown
links in each applicable durable index. Its negative tests reject missing or
wrapped unknown labels, coordinated roadmap and self-authenticating ledger
swaps, stale release identity, plain-text index mentions, links hidden in HTML
comments or code fences, missing links, missing destinations, and broken
rendered anchors.

Run `make book` for the complete rendered documentation gate. This command uses
the pinned mdBook version and rejects missing files, assets, or local anchors.

## Durable Evidence Index

Use these versioned pages before supplemental ignored build artifacts:

- [Engineering quality roadmap](../roadmaps/ROADMAP_ENGINEERING_QUALITY.md) for current requirements and append-only validation history.
- [Engineering quality execution plan](../roadmaps/ENGINEERING_QUALITY_EXECUTION_PLAN.md) for the frozen ledger and unit order.
- [Released baseline v20260908.0.0](RELEASE_BASELINE_V20260908.md) for parent-accepted operational evidence.
- [Trusted Kernel](TRUSTED_KERNEL.md) for the checked formal boundary.
- [Lean Theory Evaluation](LEAN_THEORY_EVALUATION.md) for exact theorem and replay scope.
- [Agent Context](AGENT_CONTEXT.md) for current architecture and authority rules.
