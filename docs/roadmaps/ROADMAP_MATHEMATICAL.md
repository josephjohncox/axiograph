# Mathematical Roadmap

**Diataxis:** Roadmap  
**Audience:** contributors

This roadmap tracks the mathematical work needed for Axiograph as a typed
ontology workbench. The current trusted proof layer is Lean 4 + mathlib. Rust
provides operational checking and report generation.

## Current Spine

```text
exact canonical .axi bytes + import closure + accepted snapshot handle
  -> CanonicalCompiler
  -> CompiledKernelSnapshot
  -> derived RuntimeSemanticIndex / RuntimeTheoryCheckReportV1
  -> optional Lean certificates
```

## Active Mathematical Commitments

- Schemas are finite category presentations.
- Relation objects plus role projections are canonical internally.
- Instances are finite functor-like interpretations.
- Theory objects are addressable obligations: constraints, equations, rewrites,
  transports, residuals, dependent contexts, worlds, and evidence policies.
- Merge/rebase is typed transport over finite semantic slices.
- Runtime closure is finite and declared; global ontology closure is not claimed.
- Lean verifies selected finite fragments, not the whole Rust runtime.

## Near-Term Work

- [x] Add runtime theory-obligation graphs over compiled `TheoryIr`.
- [x] Add `RuntimeTheoryCheckReportV1` with closure tiers and explicit
  non-claims.
- [x] Add Lean semantic VCS predicates for finite merge/rebase materialization.
- [x] Add Lean-readable merge/rebase JSON payloads for reduced conformance
  checks.
- [ ] Make every `TheoryIr` equation, rewrite, transport, dependent context,
  residual, and obligation addressable by stable refs.
- [ ] Feed runtime theory checks into CQ gates, behavior cases, software
  coverage, migration preview, semantic merge/rebase, and backend projection.
- [ ] Export the canonical compiled-snapshot manifest and validated derived
  `RuntimeIrRef` citations to Lean-readable checker payloads.
- [ ] Centralize finite closure semantics so “complete” means every in-scope
  obligation is checked or explicitly residual.

## Category And Migration Work

- [x] Make canonical `SchemaPresentationIr` the shared category presentation;
  derived runtime reports cite it through `KernelRefV2` instead of duplicating
  category objects or arrows.
- [ ] Represent schema morphisms and context maps as first-class typed runtime
  objects.
- [ ] Implement functorial migration operators as runtime plans first:
  `Delta_F`, `Sigma_F`, and `Pi_F`.
- [ ] Certify narrow migration fragments in Lean once runtime payloads are
  stable.
- [ ] Add natural-transformation-style witnesses for compatible refactorings
  where they materially improve merge/rebase explanations.

## HoTT And Groupoid Work

- [ ] Keep path normalization and path equivalence certificates small and
  replayable.
- [ ] Treat equivalence and transport as explicit witnesses or residual
  obligations.
- [ ] Avoid full univalence claims until a concrete fragment is encoded and
  checked.
- [ ] Use higher-path intuition for reconciliation explanations, but keep
  runtime reports finite and inspectable.

## Rust Verification Work

- [ ] Define local invariants worth checking with Rust verification tools:
  parser determinism, byte-format bounds, stable ref resolution, path endpoint
  preservation, and report validation.
- [ ] Add fuzz/property targets for `.axi`, AxQL, certificate JSON, `.axpd`,
  canonical fact logs, semantic merge/rebase payloads, and backend projection
  artifacts.
- [ ] Evaluate Miri/Kani/Verus selectively for small high-risk modules. These
  harden Rust but do not replace Lean semantic certificates.

## References

- `docs/research/APPLIED_CATEGORY_TYPE_THEORY_FOR_AXIograph.md`
- `docs/reference/RUNTIME_THEORY_CHECKER.md`
- `docs/reference/LEAN_THEORY_EVALUATION.md`
- `docs/explanation/MATHEMATICAL_FOUNDATIONS.md`
- `docs/explanation/HOTT_FOR_KNOWLEDGE_GRAPHS.md`
- `docs/explanation/TYPED_ONTOLOGY_ENGINEERING.md`
