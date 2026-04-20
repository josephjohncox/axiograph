# Typed Ontology Engineering in Axiograph

**Diataxis:** Explanation  
**Audience:** contributors

This document is the focused theory note for Axiograph's type story. It is
meant to answer a practical question:

> What does "typed ontology engineering" buy us here, mathematically and
> operationally, especially for ontology creation, discovery, grounding, and
> AI-assisted axiomatization?

The short answer is:

- typing should make ontology mistakes harder to express,
- denotational semantics should make meaning stable across backends,
- ologs should make the authoring surface intelligible,
- and AI should help propose, ground, and test ontology changes without ever
  silently becoming the source of truth.
- migration should be type-annotated and reviewable, because meaning changes are the
  highest-risk ontology events.

This document is narrower and more practical than
`docs/explanation/MATHEMATICAL_FOUNDATIONS.md`. It is also explicit about the
current trust boundary and what each layer is or is not licensed to claim.

## Executive Summary

- Axiograph is not yet a full dependently typed ontology engine.
- Axiograph does already have a meaningful typed core:
  - canonical `.axi` modules as the reviewable meaning plane,
  - a Lean-checked kernel for conservative gates and typed witness checking,
  - Rust lifecycle/anchor/schema wrappers that encode useful dependent-typing
    effects,
  - and an emerging kernel IR whose canonical semantic form is
    relation-as-object plus projection arrows.
- The right denotational picture is:
  - schema -> small category,
  - instance -> functor into finite sets,
  - contexts/worlds -> context-indexed family of such instances, naturally read
    in presheaf/sheaf terms,
  - theory -> path equations, constraints, and rewrite rules restricting
    admissible models,
  - queries -> typed graph patterns interpreted over those models,
  - certificates -> evidence that a runtime result is sound with respect to a
    narrow checked fragment of that semantics.
- Ologs fit this architecture naturally:
  - boxes become object types,
  - aspects become arrows or relation-object projections,
  - commutative diagrams become path equations,
  - n-ary facts become tuple objects with roles.
- Competency questions are the practical lens for usefulness:
  - a CQ can be seen as a typed demand on the ontology,
  - CQ coverage becomes the acceptance-quality signal,
  - and under-specified CQ outcomes become concrete extension targets.
- AI-assisted discovery belongs in the evidence plane:
  - it should discover candidates,
  - ground them to sources, contexts, and accepted anchors,
  - synthesize reviewable candidate axioms,
  - and feed a typed review/promotion pipeline.
- The goal is not "make Rust look like Idris".
- The goal is to make ontology engineering safer, more legible, more auditable,
  and more useful by putting the right invariants in the right layers.

## 1. Why Typing Matters For Ontology Engineering

Typing matters here because ontology engineering fails in ways that ordinary
graph tooling usually hides:

- a class is confused with an instance,
- a binary edge is used where an attributed relation-object is required,
- source and target roles are swapped,
- a context-scoped claim is mistaken for global truth,
- a proposal from an LLM or world model is treated as accepted semantics,
- a migration silently changes meaning because its schema mapping was only
  implied by storage shape,
- a query answer is treated as "the truth" rather than "a sound row under a
  particular snapshot and context".

In other words, ontology errors are often type errors in disguise.

The point of typing in Axiograph is not to decorate a graph with academic
vocabulary. The point is to make the semantic state machine explicit:

- what kind of thing is this,
- under which schema/theory/context does it make sense,
- in what lifecycle state does it live,
- what anchor does it belong to,
- and what semantics are we actually licensed to claim about it.

That is the notion of soundness that matters for a practical ontology engine.

## 2. The Layers Of Typing In Axiograph

Typing in Axiograph is layered. Different layers carry different kinds of
invariants and different trust classes.

| Layer | Representative artifacts | Main invariant | Trust class |
| --- | --- | --- | --- |
| Meaning plane | canonical `.axi` | reviewable ontology source of truth | accepted human/system artifact |
| Lean kernel | `axi_well_typed_v1`, `axi_constraints_ok_v1`, path/rewrite certificates | narrow checked semantic soundness | trusted |
| Rust workflow types | `Module<Validated>`, `AcceptedSnapshotId`, `AxiTypedFact` | lifecycle, anchor, schema, arity, endpoint discipline | untrusted but fail-closed guardrail |
| Kernel IR | relation objects, roles, carrier specs, path equations | stable compiled semantic form | emerging semantic spine |
| Derived execution | PathDB, AxQL elaboration, RDF/PG projections | performance and interoperability | untrusted runtime |
| Evidence plane | `proposals.json`, `chunks.json`, world-model runs, LLM suggestions | grounded candidate changes | explicitly untrusted |

The crucial architectural rule is:

> Accepted `.axi` defines what we mean; derived stores and projections define how
> we compute.

That rule is already explicit in the repo's reference docs and should remain
non-negotiable.

## 3. What "Dependent Typing" Should Mean Here

Axiograph should not use "dependent typing" to mean "every runtime detail is
encoded in the type system". That is neither necessary nor especially useful for
this product.

The useful dependent-typing effects are these:

1. Snapshot-scoped identity  
   A witness or result should say which accepted snapshot or PathDB snapshot it
   belongs to.

2. Schema-scoped typing  
   An entity/fact should be typed relative to a declared schema, not only by a
   bare string like `"Person"`.

3. Arity and field completeness  
   An n-ary fact should either have exactly the declared roles or fail.

4. Endpoint alignment  
   Paths and rewrites should be unconstructible if endpoints do not line up.

5. Context/world scoping  
   A fact or answer should say whether it is global, world-scoped, or
   context-restricted.

6. Lifecycle state  
   Proposed, validated, reviewed, accepted, certified, superseded, and
   retracted are semantically different states and should not collapse into one
   undifferentiated blob.

This is why Rust typestate and anchor newtypes matter even though Lean remains
the trusted checker. They encode the workflow discipline that ontology systems
usually lack.

## 4. Denotational Semantics: The Intended Meaning

This section gives the semantic picture the system should converge on.

### 4.1 Accepted modules and compiled semantics

Let `M` be an accepted canonical `.axi` module.

The intended compiled semantic form is a kernel IR:

`K(M) = (C_M, T_M, I_M, W_M, ...)`

where:

- `C_M` is the schema/category presentation,
- `T_M` is the theory layer (constraints, path equations, rewrite rules),
- `I_M` is the instance layer,
- `W_M` is the context/world indexing structure when present.

This is the right place to put semantic identity. PathDB, RDF, and property
graph outputs should be lowerings from `K(M)`, not rival semantic centers.

### 4.2 Schemas as categories

The clearest categorical story already present in the repo is:

- object types are objects of a small category `C`,
- subtype declarations are inclusion arrows,
- relations are not privileged binary edges; they are relation objects with
  projection arrows to their role targets,
- path equations live in the theory layer.

So a relation such as:

```text
Parent(child: Person, parent: Person)
```

should be read semantically as a tuple object with projections:

```text
ParentFact -> Person   (child)
ParentFact -> Person   (parent)
```

This is the olog-shaped reading and it matches the fact-node runtime shape much
better than a primitive binary-edge semantics.

### 4.3 Instances as functors into finite sets

An instance should be read as a functor:

`I_M : C_M -> FintypeCat`

Operationally this means:

- each object type denotes a finite set of inhabitants,
- each arrow denotes a function between those sets,
- each relation fact is an inhabitant of a relation-object carrier,
- each projection role is total on that fact.

This is not just category-theory decoration. It gives one unifying story for:

- schema-directed typechecking,
- schema-directed query elaboration,
- migration along schema morphisms,
- and type-aware ontology exploration.

### 4.4 Theory as restriction of admissible models

The theory part of a module restricts the admissible models over `C_M`.

The important classes are:

- typing/formation constraints,
- certifiable core constraints such as keys, functional dependencies,
  symmetry/transitivity subsets,
- path equations,
- accepted rewrite rules.

So the denotation of a theory is not "more data". It is a restriction on which
instances count as semantically admissible.

### 4.5 Contexts and worlds

When contexts are present, the right reading is not "attach a label to a fact
and hope downstream code remembers". The right reading is:

- there is a category or preorder `W` of worlds/contexts,
- knowledge is indexed by those worlds,
- weakening/refinement between worlds has semantics.

The conceptual form is:

`K_M : W_M^op -> [C_M, FintypeCat]`

That is: each world/context gives an instance, and moving along a refinement
map tells us how information restricts or weakens.

This explains why Axiograph should preserve:

- `@context`,
- `axi_fact_in_context`,
- world filters,
- provenance scopes,
- temporal axes,
- and context-aware querying

as semantic features, not only as runtime filters.

It also explains why "unknown" should not collapse into "false". The natural
logic here is intuitionistic/open-world by default.

### 4.6 Queries as typed pattern denotations

For the certifiable core, a conjunctive query is best read as a typed graph
pattern or homomorphism problem.

Given:

- an instance `I`,
- optionally a context/world `w`,
- and a typed query pattern `q`,

the denotation `[[q]]_(I,w)` is the set of assignments to the query variables
that make every typed atom true in `I` at `w`.

For Axiograph this yields a clean split:

- query planning and search may be heuristic,
- certified querying only promises row soundness,
- not completeness,
- and context scoping becomes part of the typed denotation rather than an
  afterthought.

### 4.7 Certificates as semantic witnesses

In the long run, the most important typed values in the system are not generic
database rows but certified semantic witnesses:

- a path witness,
- a rewrite derivation,
- a verified query row,
- a migration witness,
- a constraint satisfaction certificate,
- a typed accepted module anchor.

Denotationally, a certificate says:

> the runtime produced some candidate result `r`; here is a structured witness
> that `r` is in the interpretation of the narrow checked semantics under anchor
> `a`.

That is the right meaning of "proof-carrying ontology backend".

### 4.8 Migration as typed semantic transport

Schema evolution is where ontology engineering becomes risky if it is not typed.

For a schema map `F : C_src -> C_tgt`, useful migration must surface:

- what structure is preserved and what is derived (`Δ_F`-style transport),
- what constraints are rechecked, weakened, or become unsupported,
- what query/CQ families change answer behavior,
- and whether context or world refinements were interpreted as restriction,
  reification, or projection.

The product value is practical only when migration outputs include both the
resulting model and an explicit rationale artifact:

- typed migrated artifacts,
- migration witness (including failed obligations),
- and a migration impact summary (`schema/theory/instance/context` deltas + CQ
  regression signal).

Until this is complete for all transport forms, migration should be presented as a
typed operator with explicit soundness/coverage scope rather than as automatic
truth preservation.

## 5. Trust Boundary: What Is Actually Trusted Today

The doc above is the intended meaning story. The current trusted kernel is
narrower.

Today the strongest Lean-backed slice is:

- conservative `.axi` well-typedness gates,
- conservative certifiable constraint checking,
- anchored certificate checking,
- fixed-point confidence/probability algebra,
- typed path expressions denoted into mathlib's free groupoid.

The current kernel is not:

- a full HoTT foundation for the entire ontology engine,
- a full topos/sheaf semantics implementation,
- a complete theorem-backed categorical migration engine,
- or a proof that the storage engine itself defines semantics.

This narrower framing is a strength, not a weakness. It keeps the trusted kernel
small enough to matter.

The operational trust boundary is explicit in three tiers:

- trusted kernel slice: accepted `.axi` checks that are part of the verify path
  (well-typedness, conservative constraints, and checked witnesses);
- semantically mediated outputs: query plans, certificates, query-result witness
  artifacts, and migration previews that are emitted through trusted entrypoints
  and state what contract they satisfy;
- untrusted execution and suggestion layers: PathDB layout order, heuristic
  ranking, planner choices, adapter inference, LLM proposals, and world-model
  rollouts.

Avoid conflating these tiers. A key product-level rule is:

- soundness is explicit and scoped by claim,
- completeness is never implied unless documented by a known completeness theorem
  for the form of computation.

For this reason, every user-facing verification narrative should include both a
positive assertion (what is checked) and a boundary clause (what remains
unchecked).

## 6. Rust As A Carrier Of Useful Dependent-Typing Effects

Rust is not the trusted semantic authority, but it is where ontology workflows
become usable.

The most important existing patterns are:

- lifecycle typestate:
  - `Parsed`
  - `Validated`
  - `Reviewed`
  - `Accepted`
  - `Certified`
- stable anchor ids:
  - `AcceptedSnapshotId`
  - `PathdbSnapshotId`
  - `AxiDigest`
  - `SchemaId`
  - `TheoryId`
  - `ContextId`
  - `WorldModelRunId`
- well-typed module wrappers:
  - `Module<Validated>`
  - `Module<Reviewed>`
- schema-scoped fact/entity wrappers:
  - `AxiTypedEntity`
  - `AxiTypedFact`
- checked builders:
  - construct typed fact tuples fail-closed rather than by ad hoc mutation.

These should be read as practical dependent pairs:

- data,
- plus the index relative to which the data is meaningful,
- plus the evidence that the index discipline has been enforced.

This is what makes typed APIs useful rather than ceremonial.

## 7. Ologs As The Human-Facing Surface

Ologs matter because ontology engineers and domain experts need a surface that
is cognitively legible.

The right olog reading here is:

- boxes -> object types,
- aspects -> arrows,
- relation boxes -> relation objects with roles,
- commutative diagrams -> path equations,
- ontology refinements -> explicit new arrows, constraints, or rewrites.

That gives Axiograph a powerful authoring story:

- the mathematical core stays categorical,
- the storage layer can remain graph-friendly,
- and the human-facing authoring surface stays diagrammatic and understandable.

Ologs are therefore not an ornament. They are the best human interface to the
same typed semantic core.

## 8. Typing As User Usefulness

The theory only matters if it makes ontology work better.

From a user perspective, typing should enable:

- better ontology authoring
  - the system can say "this proposed relation is missing a role" rather than
    "invalid input".
- better discovery
  - users can ask for "all relations from `Person` to `Organization` in context
    `Regulatory2026`" instead of searching raw graph shapes.
- better explanations
  - the system can say which schema/theory/context made a result well-formed.
- better change review
  - users can see whether a proposal changes schema, theory, instance, or only
    evidence.
- better migration safety
  - migration previews can be reviewed before acceptance,
  - users can see which CQ suites remain stable across source and target models,
  - and data not carried by the migration can be surfaced as explicit residual.
- better browsing
  - exploration can pivot on object types, subtyping, relation roles, path
    equations, contexts, and competency questions rather than raw IDs.

From a system perspective, typing should enable:

- safer compilation from `.axi` into a kernel IR,
- safer lowering into PathDB, RDF, and property graphs,
- stronger query elaboration and plan selection,
- stable semantic diffs and migrations,
- typed world-model and LLM protocols,
- and future proof artifacts that are easier to certify.

The same contract should hold across authoring surfaces:

- typed `.axi` editing,
- typed query editing,
- typed migration proposals,
- and typed AI-assisted proposal previews.

## 9. The Compiler And Typechecker As Ontology Services

If Axiograph is going to be genuinely useful for ontology work, the compiler and
typechecker cannot be treated as narrow developer-only components. They should
be exposed as ontology services.

That means the compiler/typechecker should do more than reject malformed input.
It should support the user's actual modeling work.

### 9.1 Structural services

These services answer "what kind of thing is this?" and "how should it lower?".

- canonical parsing of `.axi`,
- conservative well-typedness checking,
- classification of canonical modules versus derived snapshot exports,
- lowering into a stable kernel IR,
- explicit derivation of relation-object roles, carrier structure, context axes,
  and temporal axes,
- deterministic semantic ids for schema/theory/relation/fact handles.

### 9.2 Repair and completion services

These services answer "what is missing?" or "what are the valid next moves?".

- typed holes for missing roles or fields,
- schema-qualified completion,
- ambiguity explanation and repair,
- role-name and target-type suggestions,
- constraint normalization suggestions,
- separation of accepted-plane issues from evidence-plane issues.

### 9.3 Elaboration services

These services answer "what did the system think I meant?".

- AxQL elaboration from surface syntax into typed query IR,
- explanation of context lowering,
- explanation of schema qualification,
- explanation of rewrite and normalization choices,
- explicit statement of whether a given query form is certifiable, execution-only,
  or mixed.

### 9.4 Review services

These services answer "what semantic effect would this change have?".

- semantic diff over schema/theory/instance/context,
- migration preview,
- competency-question regression checks,
- certificate availability and trust-class reporting,
- reconciliation support for branch/merge workflows.

### 9.5 Discovery services

These services answer "what ontology might we want next?".

- cluster grounded evidence into candidate object types and relation objects,
- synthesize candidate olog fragments,
- propose path equations and rewrite rules from repeated patterns,
- surface extensional regularities as candidate constraints,
- identify which competency questions are unsupported by the current ontology,
- propose schema extensions rather than silently coercing out-of-schema data.

In other words, the compiler/typechecker should become the main type-driven
assistant for ontology exploration and creation.

### 9.6 Typed authoring services (olog + CQ + migration)

Ontology usefulness is best improved when the same typed checks and anchors are
present in all editing surfaces:

- olog and relation-object authoring with schema-aware completion,
- role-aware fact and tuple construction that cannot be malformed,
- CQ construction from typed templates so expectations are explicit, and
- migration proposal previews that report query/CQ behavioral impact.

These services should always preserve provenance fields (`ctx`, `time`, accepted
anchor, proposal source, and supporting evidence) even when the surface artifact
is still in the evidence plane.

## 10. AI-Assisted Ontology Discovery

AI assistance should help with discovery, not replace semantics.

The key distinction is:

- discovery asks "what structure might be here?",
- ontology semantics asks "what structure are we prepared to accept and depend
  on?".

In Axiograph, discovery should remain evidence-plane by default.

### 10.1 Discovery

Discovery includes:

- candidate classes and object types,
- candidate relations and role names,
- candidate subtype links,
- candidate contexts/world scopes,
- candidate alignments across ontologies,
- candidate competency questions,
- candidate rewrite rules,
- candidate constraints and path equations.

These can be mined from:

- documents,
- SQL schemas,
- JSON payloads,
- RDF/OWL sources,
- logs and event streams,
- world-model rollouts,
- LLM-assisted extraction and repair.

### 10.2 Grounding

Grounding means that every proposal should be tied back to:

- a source,
- a snapshot,
- a context,
- relevant evidence chunks or records,
- and ideally the current ontology objects it claims to refine or extend.

Without grounding, "AI ontology discovery" is just ontology hallucination.

The current architecture is already pointed the right way:

- structured ingest emits `proposals.json`,
- evidence/provenance stays attached,
- named graphs and contexts are preserved,
- world models and LLMs emit reviewable overlays,
- accepted semantics only changes through promotion.

### 10.3 Axiomatization

By "axiomatization" we should mean:

> the controlled process of turning grounded regularities and modeling decisions
> into explicit reviewable ontology commitments.

Those commitments can take several forms:

- schema additions
  - new object types, relations, roles, subtypes
- theory additions
  - keys, functionals, symmetric/transitive closures, path equations, rewrite
    rules
- instance additions
  - promoted accepted facts
- regression assets
  - competency questions and expected answer shapes

The important idea is that AI should help propose these artifacts, but each one
must become explicit and typed before it becomes accepted meaning.

### 10.4 A type-driven discovery loop

The right discovery loop looks like this:

1. Ingest raw evidence into the evidence plane.
2. Emit grounded proposals with provenance and context.
3. Type proposals against the current schema if possible.
4. Where typing fails, synthesize candidate schema or theory extensions rather
   than silently coercing data.
5. Group proposals into reviewable candidate olog fragments or `.axi` deltas.
6. Preview their semantic impact:
   - typecheck,
   - constraint check,
   - CQ coverage,
   - migration/query impact,
   - conflict/reconciliation analysis.
7. Promote only reviewed canonical changes into accepted `.axi`.

This is how AI becomes useful without becoming authoritative.

### 10.5 CQ-driven usefulness loop

For every ontology session, CQs are most useful when they are used as progress
objects:

- answered CQs increase confidence in current modeling commitments,
- underdetermined CQs surface expected typing/context gaps,
- contradictory CQs force explicit merge or scope decisions,
- and unsupported CQs provide concrete candidate ontology extensions.

That loop is more operationally useful than generic LLM proposals because each CQ
result maps to typed structures already in the kernel IR.

### 10.6 Grounded axiomatization contract

The production path from evidence to acceptance should remain explicit:

- AI extraction proposes typed candidates;
- grounded proposer metadata and anchors are attached;
- compiler and trusted slices classify each proposal as accepted, execution-only,
  or mixed;
- proposal bundles that fail typing or trust-class checks are retained as
  evidence-plane assets, not silently discarded;
- only reviewed and promoted typed proposals enter the accepted plane.

## 11. Olog Discovery And Axiomatization

Ologs give a particularly good shape for AI-assisted ontology work because they
turn vague extraction into diagrammatic proposals.

An AI assistant should be able to propose things like:

- "There appears to be an object type `Shipment`."
- "There appears to be a relation-object `Delivery` with roles
  `(shipment, carrier, destination, time)`."
- "There appears to be a commuting path from `Employee` to `Department`:
  `employee -> team -> department` agrees with `employee -> manager -> department`."
- "This recurring pattern looks like a key or functional dependency."
- "This translation pattern looks like a rewrite rule."

Those are not just text suggestions. They are candidate typed structures that
can be shown to the user in the same semantic language the system uses
internally.

That is the deeper usefulness of a typed ontology engine: discovery and
authoring use the same objects.

This is why olog-style authoring should be treated as a first-class workflow,
not a visualization garnish. It is the practical bridge from subject expertise to
typed compiler-ready ontology fragments.

## 12. Type-Driven Tools We Should Build

If we want the theory to become productively useful, the next tools should be
explicitly type-driven.

### 12.1 User-facing tools

- Schema-aware editor with typed holes
  - "a relation from `Person` to `Company` is missing one role"
- Olog workbench
  - boxes, aspects, relation objects, path equations, CQ attachments
- Typed ontology explorer
  - browse by object type, subtype, relation role, context, and theory object
- CQ runner and CQ synthesis
  - ask what the ontology can answer and where it is under-specified
- Axiom proposal workbench
  - show proposed constraints, rewrites, path equations, and their grounding
- Migration preview
  - show how a candidate schema morphism changes instances and queries
- Semantic diff and merge UI
  - review ontology changes as schema/theory/instance/context deltas

### 12.2 System-facing tools

- kernel IR compiler as a first-class crate/module pair,
- type-directed query IR and prepared handles,
- semantic VCS with refs/commits/reconciliation,
- typed world-model protocols and provenance ids,
- certificate-aware API surfaces,
- typed adapter layers for RDF/OWL/SHACL and property graphs.

## 13. Current Reality Versus Roadmap

The current system is already serious, but it is important to describe it
accurately.

### 13.1 What is already real

- canonical `.axi` as accepted meaning plane,
- Lean-checked conservative `.axi` gates,
- Lean-checked anchored certificate replay,
- typed path expressions with free-groupoid denotation,
- fixed-point confidence algebra,
- Rust lifecycle and anchor types,
- schema-scoped typed wrappers for facts and entities,
- an initial compiled schema IR for relation-role semantics,
- evidence-plane proposal protocols for ingest, LLMs, and world models.

### 13.2 What is not yet real

- a fully internalized categorical ontology kernel,
- a complete olog authoring frontend,
- theorem-backed `Delta/Sigma/Pi` migration semantics end to end,
- complete type-driven query semantics for all query forms,
- fully first-class context/sheaf semantics in the runtime kernel,
- or a complete semantic VCS implementation.

### 13.3 What should come next

If the goal is "typed ontology engineering that is genuinely useful", the next
technical priorities are:

1. make the kernel IR explicit and canonical across more of the system,
2. push more public Rust APIs onto typed anchors and lifecycle states,
3. make typed query/tooling surfaces first-class for users,
4. add an olog authoring and review surface that lowers to the same IR and
   preserves proposal provenance,
5. make semantic VCS history, semantic diff, and reconciliation first-class,
6. keep AI-generated ontology structure in the evidence plane until grounded,
   typed, reviewed, and promoted,
7. require CQ-driven migration previews before schema changes are accepted.

## 14. A Practical Product Claim

The most accurate product-level claim today is something like this:

> Axiograph is a typed ontology workbench with a Lean-checked semantic kernel for
> conservative gates and typed witness checking, a Rust runtime that makes
> anchors/lifecycle/schema discipline first-class, and an architecture designed
> to support AI-assisted ontology discovery and grounding without surrendering
> truth to heuristics.

That is already a strong claim. It is also one we can defend.

## 15. Reading Map

This document is best read alongside:

- `docs/reference/TRUSTED_KERNEL.md`
- `docs/reference/KERNEL_IR.md`
- `docs/reference/SEMANTIC_VCS.md`
- `docs/explanation/TOPOS_THEORY.md`
- `docs/explanation/RUST_DEPENDENT_TYPES.md`
- `docs/explanation/MATHEMATICAL_FOUNDATIONS.md`
- `docs/explanation/OBJECTIVE_DRIVEN_AI.md`
- `docs/reference/WORLD_MODEL_PLUGIN.md`
- `docs/roadmaps/ROADMAP_ONTOLOGY_ENGINEERING.md`

## 16. Bottom Line

Typing is not a side concern in Axiograph. It is the mechanism by which:

- ontology semantics becomes stable,
- workflows become safer,
- provenance becomes harder to fake,
- AI assistance becomes reviewable,
- and ontology discovery becomes a disciplined source of candidate structure
  rather than a source of silent semantic drift.

The right endpoint is not "a graph database with some type annotations".

The right endpoint is:

- a canonical accepted meaning plane,
- a compiled semantic IR,
- a small trusted checker,
- fast untrusted execution,
- type-driven tooling for exploration and creation,
- and AI systems that help discover ontology structure while remaining grounded,
  typed, and subordinate to review.
