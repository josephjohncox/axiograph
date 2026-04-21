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

That backend point should be read narrowly. "Stable across backends" does not
mean every graph store is an equally good semantic home. It means advanced graph
engines can host **typed projections** of accepted semantic state when they have
enough structure, while semantic VCS, authoring, ologs, trust contracts, and
CQ-gated lifecycle remain above them in Axiograph.

At the moment that means:

- `TypeDB` is the primary target when we want the backend itself to preserve
  typed relation/role/n-ary structure well.
- `TerminusDB` is the strongest RDF/VCS-shaped secondary target.
- property-graph backends remain experimental rather than first-class support
  targets. `Apache AGE` is the main one still worth evaluating, but it is not
  yet on the same compatibility tier.

That priority is not just a ranking; it is a pushdown split:

- push the richest type/constraint/query-validation surface into `TypeDB`,
- push branch/history/diff workspace mirroring into `TerminusDB`,
- keep property-graph execution as a later optimization layer rather than an
  active backend commitment,
- and keep semantic authority in Axiograph above all of them.

That does not mean projected backends should be opaque. They should still be
usable through their own native query interfaces so outside tools and graph
users can inspect a meaningful subset of the projected ontology. The contract
should be:

- native backend querying is allowed,
- native backend querying exposes only a reduced projection of Axiograph
  semantics,
- and backend mutation is not authoritative: semantic mutation, review,
  promotion, and lifecycle transitions still go through Axiograph.

The current Rust implementation now reflects that split directly. The CLI layer
contains a backend pushdown planner that consumes:

- `CompiledSchemaIr`,
- `BackendCapabilityProfileV1`,
- `ProjectionCapabilityProfileV1`,

and emits typed backend-specific plans for the first-class backends:

- `TypeDbPushdownPlanV1`
- `TerminusDbPushdownPlanV1`

These plans make the degradation boundary explicit instead of burying it in
adapter code. They say which tuple/role/context structure is preserved natively,
which native read-only query surface is exposed, and which semantics still
require Axiograph-side trust contracts and anchor checks.

The important refinement is that projected backends should not be described as
"mere exports". They should preserve readable lower-tier interfaces all the way
through the projection:

- native query interfaces remain usable as lower-tier read surfaces,
- RDF-style datasets remain readable where the backend genuinely hosts them,
- SHACL-style validation remains usable as a lower-tier closed-world interface
  over the projected graph,
- and Axiograph lifts those lower-tier surfaces back into higher typed semantic
  objects, anchors, lifecycle state, CQ gates, and trust contracts.

So the right model is:

- **preserve lower-tier interfaces**,
- **lift them into higher typed/meta semantics**,
- and **never let the lower tier replace the semantic kernel**.

In other words, we should not ask one backend to do all jobs equally well.
`TypeDB` is the best host for typed runtime elaboration fragments.
`TerminusDB` is the best host for native graph-history collaboration.
Property-graph engines may still become useful execution targets later, but
they are not part of the current first-class support contract.

## TypeDB-inspired typing we should actually import

TypeDB is useful to study because it gets several schema ideas right:

- relation types are first-class, not hidden edge metadata,
- roles are first-class typed interfaces,
- subtypes inherit interface implementations,
- and schema edits are treated as explicit operations rather than free-form
  mutation.

The official TypeDB docs are especially clear on:

- entity/relation/attribute typing and interface polymorphism,
- `owns` / `plays` / `relates` as interface declarations and implementations,
- subtype inheritance of those interfaces,
- cardinality / key constraints,
- and `redefine` as an explicit schema-change operation.

For Axiograph, the right import is semantic, not syntactic.

We should keep:

- first-class relation objects,
- scoped role interfaces,
- subtype-inherited admissible player sets,
- explicit constrained schema evolution,
- and typed constraint objects rather than prose-only rules.

We should not copy directly:

- backend-owned schema authority,
- backend-native history as the ontology VCS,
- or a backend surface becoming the canonical authoring language.

So the Axiograph reading is:

- a relation role defines a typed interface over the canonical IR,
- subtype closure determines the admissible players of that interface,
- CQ-gated evolution and semantic VCS own the mutation lifecycle,
- and backend pushdown plans may preserve these interfaces natively without
  becoming authoritative.

This is where TypeDB-like discipline helps Axiograph most:

- typed authoring can suggest valid role binders from admissible players,
- query elaboration can surface why a subtype is accepted at a role boundary,
- backend adapters can say exactly which role/player semantics survive pushdown,
- and evolution previews can explain subtype generalization/specialization as
  first-class semantic moves instead of opaque diffs.

The important caveat is that backend-native history is still not the same thing
as Axiograph semantic VCS. For example, TerminusDB's git-for-data layer is
genuinely useful for projected collaboration branches, but its own docs say that
branch pull/push operations transport instance information while schema
operations "need to be manually maintained" or "manually applied". That is
already enough to disqualify backend-native history from becoming the ontology
kernel for reviewable schema/theory evolution.

For ontology authors, typing is valuable only if it catches meaning errors early,
keeps proposal deltas reviewable, and surfaces CQ breakage before they accept
new commitments. For tool builders, typing is valuable when it gives deterministic,
contracted outputs (typed handles, anchors, and trust classes) that do not require
trusting opaque runtime heuristics.

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

### Runtime-typed usefulness bar

Typing becomes operational only when it emits reviewable artifacts rather than
only booleans, warnings, or prose.

Near-term Axiograph should converge on a small runtime-typed artifact family:

| Artifact family | Why it exists | Minimum typed content |
| --- | --- | --- |
| Evolution preview | review ontology change before mutation | typed `schema` / `theory` / `instance` / `context` deltas, CQ status, trust contract, residual obligations |
| Business-rule applicability report | answer which obligations apply under explicit anchors | matched rule/theory/CQ ids, snapshot/world scope, trust strength, checked surfaces, next actions |
| Semantic coverage / drift report | show what the ontology actually covers in implementation and interop | ontology-object/rule/CQ coverage, uncovered areas, drift reasons, anchors, caveats |
| Agent-facing semantic report | let coding agents make disciplined engineering claims | proposition/task, matched ontology objects, trust, evidence/checks used, residual unknowns, suggested repairs |

Lean will certify the strongest fragment of some of these artifacts, but the
existence of the artifact family itself is a runtime design requirement rather
than a theorem-proving requirement.

### Runtime surfaces now worth building around

The runtime checker becomes materially useful only when it exposes concrete
typed services rather than keeping all of its semantics hidden inside import and
query code paths.

The immediate high-value services are:

- typed olog fragment checking over the compiled schema/category IR,
- business-rule applicability over explicit relation/theory scopes,
- semantic coverage over implementation surfaces and rule ids,
- and evolution previews that reuse the same typed deltas instead of inventing
  workflow-specific JSON envelopes.

The important operational detail is that these are not generic "analysis"
reports. They are intended to become the everyday ontology-engineering currency
 for:

- authoring relation boxes and projections,
- checking whether an endpoint/workflow/job is semantically governed,
- showing which accepted rules are only weakly mapped into implementation,
- and telling an agent what ontology/code/test work is still missing.

That is the practical meaning of "runtime-usable dependent typing" in this
project: a typed service layer over the canonical IR that is good enough to
drive authoring, review, and engineering workflows before optional Lean
certification.

### Rich evolution primitives for directed exploration

Directed exploration should not stop at "this preview added 2 schema objects"
or "a subtype link appeared somewhere in the diff". If the ontology is going
to be a serious authoring and discovery tool, the runtime checker needs a
vocabulary of reviewable structural moves.

The useful minimum is:

- `reify_relation_object`
- `introduce_dependent_relation_family`
- `introduce_subtype`
- `generalize_to_supertype`
- `specialize_to_subtype`
- `push_relation_role_to_subtype`
- `pull_relation_role_to_supertype`
- `factor_common_structure_to_supertype`
- `split_type_into_subtypes`
- `merge_types_under_supertype`
- `lift_relation_to_carrier`
- `add_path_equation`
- `add_rewrite_rule`

These are not UI sugar. They are the typed operational language for evolving
ologs and ontologies as new structure is discovered.

From an ontology-engineering perspective, they matter because:

- relation-object reification and indexed/dependent-family primitives expose
  when a domain relation is really a family over context, time, or other
  parameters rather than a plain binary edge;
- subtype/supertype moves change the lattice that query elaboration, rule
  applicability, and code mapping depend on;
- relation-role push/pull changes whether a role is valid at a general type or
  only at a refined subtype;
- path-equation and rewrite-rule primitives let theory evolution be reviewed as
  first-class semantic law rather than being buried in prose;
- factor/split/merge moves are often the real semantic change even when the
  visible vocabulary barely changes;
- lifting a relation into an explicit carrier object is how the model matures
  from "binary edge sketch" to a relation-object that can carry time, context,
  provenance, parameters, and business constraints.

From a user perspective, this makes exploration materially more useful. The
system should be able to say:

- "introduce `Pump <: RotatingEquipment`"
- "generalize this maintenance rule from `BoilerPump` to `Pump`"
- "push the `requires_certification` role down from `Equipment` to
  `PressureVessel`"
- "lift `delivers_to` into `Delivery(material, plant, carrier, time)` because
  time/provenance is now first-class"

That kind of preview is what lets ontology authoring become a disciplined
co-evolution workflow rather than a bag of schema notes.

## 0. What Would Make Axiograph A Dependently Typed Ontology Engine?

The phrase matters because it is easy to use it loosely and end up saying
almost nothing.

The useful reading is not:

- every implementation detail lives in Lean,
- every query is theorem-backed,
- or Rust disappears as an execution layer.

The useful reading is:

> the semantics-bearing seams of the product are indexed by the right semantic
> parameters, and the highest-value soundness claims reduce to a small checked
> kernel.

In this repo, that stronger claim should be treated as earned only when all of
the following are true.

1. Lean owns the semantic acceptance boundary for the certifiable fragment.
   Accepted canonical `.axi` modules, checked rewrite rules, and the relevant
   witness-bearing artifacts for queries, migrations, and constraints are all
   reduced to explicit checked obligations.

2. The compiled schema/category IR is the canonical semantic spine.
   The same accepted module lowers deterministically to the same objects,
   relations, roles, arrows, equations, and stable ids. PathDB, RDF, property
   graph, and visualization layers become projections from that compiled meaning.

3. Rust APIs carry the indices that make ontology objects meaningful.
   Public handles are indexed by lifecycle state, accepted snapshot anchor,
   schema/theory/context identity, and prepared semantic form rather than by
   raw strings and storage-local integers alone.

4. Authoring, querying, migration, and certification use one family of typed
   artifacts.
   The system stops having one language for editor changes, another for query
   elaboration, another for migration preview, and another for certificates.
   They become different views of one typed operational story.

5. Olog authoring lowers into the same core.
   Boxes, aspects, relation objects, and commuting diagrams compile to the same
   stable ids and same canonical deltas that the rest of the toolchain uses.

6. Semantic VCS tracks typed ontology change rather than only storage mutation.
   Review, merge, promotion, supersession, and retraction are expressed as
   anchored semantic deltas over schema/theory/instance/context objects.

There is a subtle but important clarification here:

- becoming a dependently typed ontology engine does **not** mean “the whole
  runtime is trusted”;
- it means “the semantically meaningful boundaries are typed, anchored, and
  fail-closed, and the checked claims are explicit about soundness scope”.

Memorable takeaway:

> In Axiograph, “dependently typed ontology engine” should mean a small checked
> semantic kernel plus one typed operational currency for ontology work.

### 0.1 What sound type theory should mean here

For Axiograph, a sound type theory is not "full-spectrum Martin-Lof type theory
everywhere in the runtime". It is a disciplined split between:

- a small checked fragment where the strongest semantic claims are reduced to
  explicit judgments, and
- a runtime elaborator/checker that preserves those same indices and refuses to
  erase them into storage-local or stringly surfaces.

The right internal judgment forms are practical rather than ornamental:

- schema/category formation,
- object and relation-role typing,
- context/world-indexed truth,
- endpoint-safe path typing,
- rewrite admissibility,
- transport along schema/theory maps,
- and lifecycle legality across evidence/review/accepted/certified states.

That is already enough to be recognizably dependent in the useful sense:

- a fact depends on its schema, relation, anchor, and sometimes context;
- a path depends on its endpoints and anchor;
- a query depends on the schema/context/result-shape it elaborates against;
- a migration witness depends on a transport basis and reindexing links;
- and a certified answer depends on the accepted anchor under which it was
  checked.

What about higher-order structure?

- In the trusted kernel, higher-order structure should be treated narrowly:
  rules, path equations, CQ obligations, and transport/rewrite operators can be
  first-class semantic objects that are checked and referenced.
- In the runtime layer, higher-order usefulness means users and agents can ask
  for structure over structure: "what rewrite laws apply here?", "what CQs are
  induced by this authoring fragment?", "what transport obligations does this
  schema map generate?", "what refinements preserve certifiability?"
- It does **not** currently mean arbitrary higher-order dependent programming in
  the theorem-prover sense, and the docs should stay honest about that.

So the soundness bar is:

- accepted meaning is formed over the canonical IR,
- the strongest claims are checked against explicit anchors,
- Rust preserves the semantic indices in its public/runtime tooling surface,
- and no layer silently upgrades weak/exploratory/evidence-plane structure into
  accepted truth.

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

The practical rule across all layers is that anything exposed to operators should
be typed by its trust boundary and its provenance, not only by its structure.

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

### 2.1 Trust contract language

For anything that drives authoring, browsing, migration, or query result use,
the minimum contract should make these fields explicit.

For ontology authors, this is the minimum information needed to decide whether a
result is safe to rely on during review or promotion. For system builders, these
fields should be treated as required response data rather than optional
documentation prose.

The contract should include:

- `trust_class`: `certified` / `mixed` / `execution-only`
- `soundness`: what semantic fragment was checked (e.g. certificate
  kernel slice, CQ subset, migration transport slice)
- `coverage`: where coverage can be trusted (schemas, relations,
  contexts, snapshot windows)
- `scope`: where the claim is valid (snapshot/context slice and artifact family)
- `anchors`: stable IDs for accepted snapshot, source snapshot, proposal run, and
  context/world
- `caveats`: short explicit boundary clauses such as open-world behavior,
  heuristic fallback, or unsupported transport obligations

Only this contract-based split keeps ontology meaning and runtime heuristics
from becoming conflated in operator workflows.

Just as important is what the contract does **not** say:

- it does not say that all valid answers were found,
- it does not say the ontology is closed under all intended consequences,
- it does not say every affected context or query family was analyzed,
- and it does not say an evolution step preserved meaning outside the stated
  `coverage` and `scope`.

So when a trust contract says `certified`, the right reading is:

> within the stated soundness fragment and anchors, this artifact carries a
> checked justification.

The wrong reading is:

> the system has established full completeness, closure, or global semantic
> safety.

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
  and whether context or world refinements were interpreted as restriction,
  reification, or projection.

A useful migration preview therefore includes:

- a full diff of preserved/generated/dropped shape by layer (`schema`,
  `theory`, `instance`, `context`),
- CQ regression signals for configured suites,
- trust-contract deltas (what remains certified vs execution-only),
- and a provenance chain from source accepted snapshot to proposal set and target
  anchor.

The product value is practical only when migration outputs include both the
resulting model and an explicit rationale artifact:

- typed migrated artifacts,
- migration witness (including failed obligations),
- and a migration impact summary (`schema/theory/instance/context` deltas + CQ
  regression signal).

Until this is complete for all transport forms, migration should be presented as a
typed operator with explicit soundness/coverage scope rather than as automatic
truth preservation.

#### 4.8a Reindexing and semantic comparability

Transport is only half of useful evolution. The other half is explicit
reindexing: which source semantic ids and target semantic ids remain comparable
across anchors after a change.

That is what lets the runtime say:

- this source object is preserved as that target object,
- this source arrow is now reviewed as a target path equation,
- this subtype collapsed into its supertype image,
- or this distinction was merged and now survives only as a residual
  obligation.

Without reindexing, migration reports degrade into counts and prose. With
reindexing, previews, CQs, implementation mappings, and certificates can all
point at the same semantic objects when they explain what changed.

#### 4.8b Reconciliation as typed evolution over conflicts and decisions

Reconciliation should be framed in the same typed language as authoring and
migration, not as a separate file-merge story.

Operationally, reconciliation is typed evolution over competing deltas rooted
at a common anchor:

- identify conflicts over schema/theory/instance/context artifacts,
- record explicit decisions,
- attach CQ/trust/coverage consequences,
- and carry unresolved conflicts forward as residual obligations.

The current runtime slice is deliberately conservative. It can summarize typed
reconciliation previews from explicit conflict/decision records, but it does
not yet claim merge optimality, merge completeness, or ontology closure. That
is still the right operational move because it makes semantic merge reviewable
through the same preview contract as every other mutation seam.

### 4.9 CQ-gated evolution

Competency questions should not be treated as a nice-to-have reporting layer.
They are the most useful concrete acceptance obligations available to ontology
authors.

For a proposed change set `Δ`, the system should be able to answer:

- which CQs stayed satisfied,
- which newly became satisfiable,
- which regressed,
- which remain unsupported because typing/schema/theory/context is missing,
- and which trust contracts changed from `certified` to `mixed` or
  `execution-only`.

That makes CQ gating operational rather than rhetorical. A useful evolution
preview should therefore bundle:

- the source accepted snapshot anchor,
- the candidate target anchor or review delta,
- the typed change summary (`schema` / `theory` / `instance` / `context`),
- explicit structural primitives describing the intended ontology move:
  - `reify_relation_object`
  - `introduce_dependent_relation_family`
  - `introduce_subtype`
  - `generalize_to_supertype`
  - `specialize_to_subtype`
  - `push_relation_role_to_subtype`
  - `pull_relation_role_to_supertype`
  - `factor_common_structure_to_supertype`
  - `split_type_into_subtypes`
  - `merge_types_under_supertype`
  - `lift_relation_to_carrier`
  - `add_path_equation`
  - `add_rewrite_rule`
- per-CQ before/after status with expected answer-shape references,
- trust-contract deltas,
- and residual obligations or migration failures that still need author review.

For ontology authors, this turns a vague question like "is this schema cleanup
safe?" into a reviewable statement about which domain questions still work and
which commitments changed. For system builders, it gives a precise fail-closed
interface for `fail_on_regression`, `fail_on_unsatisfied_after`, and later
merge/promotion policies.

Those structural primitives are not cosmetic labels. They are the bridge between
typed ontology semantics and useful engineering review:

- `reify_relation_object` and `introduce_dependent_relation_family` explain
  when the authoring move is really about relation-objects, indexed families,
  and dependent structure rather than new nouns alone;
- `introduce_subtype` / `generalize_to_supertype` / `specialize_to_subtype`
  explain how the subtype lattice is evolving and whether rules, constraints,
  and implementation mappings should move up or down;
- `push_relation_role_to_subtype` / `pull_relation_role_to_supertype` explain whether a role is being
  re-homed between an object and a relation carrier, which is often the actual
  semantic change hidden inside "field cleanup";
- `factor_common_structure_to_supertype`, `split_type_into_subtypes`, and `merge_types_under_supertype` make
  refactoring intent explicit enough for migration preview and CQ review to be
  meaningful rather than textual;
- `lift_relation_to_carrier` tells the system that an edge-like pattern now
  requires first-class tuple identity, provenance, context axes, or additional
  dependent parameters;
- `add_path_equation` and `add_rewrite_rule` make theory evolution visible as a
  typed change rather than mixing semantic laws into prose or opaque notes.
  roles.

These same primitives also improve directed exploration. A useful runtime
checker should be able to suggest:

- sibling subtypes or supertypes that now deserve review,
- relations that become candidates for shared factoring,
- roles likely to move with the current refactor,
- CQs whose answer shape may widen or narrow,
- and implementation/doc/test surfaces mapped to the affected semantic ids.

That is the difference between a preview that merely reports "schema changed"
and one that helps an ontology engineer steer the next edit safely.

Current limitation: Axiograph has a first useful CQ-gated preview slice for
proposal validation today, but migration preview, semantic merge, and accepted-
plane promotion are not yet all mediated by one uniform evolution-preview
object.

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

For each artifact outside the trusted kernel, the minimum response language is:

- `trust_class` (what can be claimed),
- `soundness` (what was proven),
- `coverage` (what is not covered),
- `scope` (snapshot/context/wf boundary of the claim),
- `anchors` (accepted/pending snapshot + input context),
- and a short caveat list (e.g. open-world, approximation, or heuristic fallback).

Avoid conflating these tiers. A key product-level rule is:

- soundness is explicit and scoped by claim,
- completeness is never implied unless documented by a known completeness theorem
  for the form of computation.

For this reason, every user-facing verification narrative should include both a
positive assertion (what is checked) and a boundary clause (what remains
unchecked).

Trust contracts are therefore about soundness class and scope, not about
completeness or semantic closure. Typical examples:

- a `certified` query row means the returned row has a checked witness under the
  stated anchors; it does **not** mean every valid row was found;
- a CQ report over a configured suite means those specific questions were
  evaluated; it does **not** mean all important domain questions are known;
- a migration preview with typed obligations means the reported transport slice
  was analyzed; it does **not** mean the target ontology is semantically closed
  or fully equivalent to the source.

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

### 6.1 Rust as the runtime-usable checker

The next important step is to treat the Rust layer not only as a collection of
guardrails, but as the **runtime-usable checker** for ordinary ontology work.

That is the layer that should be fast and practical enough to drive:

- typed authoring validation,
- typed query elaboration and result-shape reporting,
- business-rule lookup and applicability checking,
- CQ execution over accepted/review baselines,
- migration and semantic-diff preview,
- semantic coverage and drift reporting,
- and agent-facing answers over code, docs, data, and accepted ontology state.

This is a stronger and more useful role than "runtime plumbing", but it is still
not the role of trusted semantic authority.

The practical value is that ontology authors and coding agents need answers
before every question is promotable to Lean-certified form. A useful system
must be able to say:

- which accepted or review-state ontology objects apply,
- which contexts/worlds are in scope,
- what the typed result shape is,
- what is covered by the modeled semantics,
- and where the answer remains weak, partial, or unsupported.

That is why Rust is the right place for the operational checker surface. It is
where authoring, query, preview, and agent workflows become usable enough to
drive everyday engineering.

The practical implication is that Rust should expose typed operations over
semantic structure, not only typed wrappers over stored values. A useful
runtime surface needs first-class APIs for:

- elaborating partial queries into typed result-shape judgments,
- surfacing typed holes and machine-applicable refinements,
- checking typed olog fragments against the compiled IR,
- reporting business-rule applicability under explicit anchors and contexts,
- previewing transport/reindexing/reconciliation obligations,
- and returning agent-facing semantic reports that preserve trust class and
  lifecycle scope.

That is the runtime analogue of higher-order/dependent usefulness: the checker
works over rules, holes, transport maps, CQ obligations, and previews as typed
objects, not only over entity rows.

### 6.2 What the runtime checker may and may not claim

The right trust split is:

- Rust provides the useful checker for the broader runtime slice,
- Lean remains authoritative for the certified semantic slice,
- and retrieval/evidence layers remain weaker than both.

So a runtime-checked result can honestly claim things like:

- this rule applies under accepted snapshot `A` and context `W`,
- this authoring fragment is well-typed relative to the compiled schema IR,
- this query elaborates to result shape `R`,
- this migration preview preserves the reported obligations,
- this SHACL report found `Invalid` or `Unknown` findings against the mapped
  ontology objects,
- or this implementation surface has no accepted ontology mapping yet.

But it should not claim:

- semantic completeness,
- ontology closure,
- universal validity outside the stated anchors and contexts,
- or trusted semantic certification unless the result has actually crossed the
  Lean-checked boundary.

That distinction matters especially for coding agents. The point is not to make
all useful engineering claims certified. The point is to make them **typed,
anchored, scoped, and explicit about their strength**.

### 6.3 Why SHACL, RDF, and ologs must meet in the same place

Interop and authoring only become operationally coherent when SHACL, RDF, and
ologs meet at one canonical semantic layer.

They play different roles:

- ologs are the human-facing authoring surface,
- RDF is a boundary-layer interchange and query surface,
- SHACL is a boundary-layer validation surface.

But if they lower into different semantic stories, ontology engineering becomes
fragmented:

- the authoring view says one thing,
- the interop adapter says another,
- and the validation report names objects the author never sees.

The better pattern is:

- olog edits lower to canonical object/arrow/relation-role deltas,
- RDF import/export/query flows lower to the same relation-object and projection
  arrow story,
- SHACL path and count constraints are interpreted against the same canonical
  objects where the mapping is defined,
- and the runtime checker reports all three using one trust-language family.

This is not only a documentation convenience. It is what makes typed ontology
authoring and querying sound enough to use operationally:

- authors can see which canonical objects they are editing,
- interop users can see which mappings are accepted versus provisional,
- agents can tell whether a rule/check is ontology-backed, runtime-checked, or
  only retrieval-supported,
- and Lean can remain reserved for the narrower certified fragment rather than
  being forced to explain every interop detail.

## 7. Ologs As The Human-Facing Surface

Ologs matter because ontology engineers and domain experts need a surface that
is cognitively legible.

In a useful loop, olog editing is not passive drawing:

- every box/arrow change is immediately mapped to stable kernel identifiers,
- every proposed addition is serialized as an `.axi` draft delta with roles,
  provenance, and trust-class metadata,
- and every olog fragment can be routed into the same CQ/migration preview
  gates as other proposal sources.

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

They are also not the only exploration surface. The same compiled IR should
power query refinement, migration preview, reconciliation review, backend
projection inspection, and implementation-surface mapping, all over the same
stable semantic ids.

### 7.1 What typed olog authoring should emit

If an olog editor is genuinely part of ontology engineering, each meaningful
edit should emit a machine-reviewable artifact rather than canvas-only state.

At minimum, an edit transaction should produce:

- a canonical delta or draft over a known base anchor,
- stable identifiers for boxes, arrows, relation objects, and roles,
- provenance fields tying the edit back to author, source evidence, context, and
  proposal run when relevant,
- typing status plus repair diagnostics when the fragment is incomplete,
- CQ attachments or CQ impact references,
- and a trust contract stating whether the fragment is only drafted, validated,
  or otherwise backed by a checked slice.

That is what lets ontology authors review meaning changes directly, and what
lets tool builders connect editor, compiler, migration preview, and review UI to
the same underlying objects.

### 7.2 Current slice and gap

The current runtime has the beginning of this story, not the finished surface:

- proposal flows can already emit canonical `.axi` drafts,
- `draft_axi_from_proposals` can distinguish plain rendered drafts from Rust-side
  validated drafts,
- and proposal preview validation can already attach CQ/trust metadata.

What is not yet shipped is the full olog workbench that preserves those same
artifacts through edit, semantic diff, review, merge, and promotion. That gap
should be described plainly whenever the authoring surface is discussed.

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

The immediate value is dual: ontology authors get better drafting signals, and
tool builders get deterministic typed APIs for editor, CQ, and migration tooling.

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

### 9.3a Type inference and hole-driven exploration

Type inference here should be read conservatively. The job is not to infer an
ontology we failed to model. The job is to infer the strongest indices already
licensed by the accepted/reviewed schema/category IR: variable types, relation
roles, schema qualification, context/world carriers, result shape, and
certifiability class. When multiple readings remain possible, the runtime
should keep that ambiguity explicit and return repair choices rather than
silently guessing.

Typed holes are the dual surface of that same service. A hole is not just
missing syntax; it is a request for admissible next moves under explicit
anchors. In queries, holes can stand for unresolved relation objects, role
fillers, projection targets, or context restrictions. In authoring, they can
stand for missing roles, projection arrows, subtype witnesses, or CQ
expectations. The useful response is a bounded set of schema-valid completions
plus an explanation of how each completion changes trust and certifiability.

The current runtime slice now does this in two places:

- queries return structured typed holes plus refinement candidates over
  prepared/query IR;
- typed olog authoring returns structured holes for missing relation-role
  bindings, role/type mismatches, projection gaps, and path endpoint failures.

Those two surfaces now also share a first common runtime refinement protocol:

- one handle/candidate envelope over compiled IR,
- query-local refinement operations for query elaboration,
- authoring-local refinement operations for typed olog repair where the edit is
  deterministic,
- and stable machine-applicable ids that let agents and editors move from
  “what fills this hole?” to “apply this refinement and re-check”.

That is already useful for type-directed exploration, but it is still a runtime
checker service, not a theorem-backed completeness result and not yet a uniform
machine-applicable repair protocol across every authoring surface.

That is why query elaboration should be treated as an exploration service, not
only as a compiler pass. A partial or ambiguous query should be able to return:

- currently inferred variable and result types,
- the candidate schemas/relations/contexts still in play,
- the typed holes or unresolved obligations that remain,
- the admissible next refinements,
- and how each refinement changes the path from `execution-only` to `mixed` or
  `certifiable`.

This is a real dependent-type effect in the runtime layer: partial artifacts
remain indexed by the schema/context/anchor relative to which they make sense,
and users can ask "what can go here?" before committing to a full term. But it
does not replace the semantic kernel. Inference, completion, and hole filling
are operational services over the compiled IR; accepted `.axi` anchors and
Lean-checked certificates remain the authority for the strongest semantic
claims.

A useful formal reading is:

- `Sigma; Gamma; A |- q => (q', H, E, kappa)`

where:

- `Sigma` is the compiled schema/category IR,
- `Gamma` is the current variable typing environment,
- `A` is the accepted/review-state anchor plus world/context scope,
- `q` is a partial query or authoring fragment,
- `q'` is the best elaborated form currently justified,
- `H` is the finite family of typed holes that remain,
- `E` is the finite family of admissible next refinements,
- and `kappa` is the current runtime trust/certifiability classification.

Denotationally, a partial typed query is not yet a proposition with one fixed
meaning. It denotes a bounded family of anchor-scoped completions licensed by
the current IR:

- `[[q]]_(Sigma,A) = { q_complete | q_complete is a schema-valid completion of q under A }`

The elaborator is therefore a semantics-preserving restriction operator: it
shrinks that completion family, records why, and keeps the remaining ambiguity
explicit instead of silently guessing. That is the runtime analogue of
Lean/Haskell-style hole-driven development. The next implementation step is to
make those refinements machine-applicable over `query_ir_v1`, not merely
diagnostic strings.

### 9.3b Compiled-IR exploration beyond ologs

Typed exploration should not stop at queries or hand-authored olog fragments.
The same compiled IR should be able to surface:

- relation-object reification opportunities,
- indexed/dependent-family opportunities,
- carrier/fiber lifting opportunities,
- candidate path equations or rewrite rules suggested by witness view,
- migration transport obligations,
- and reconciliation follow-on work.

Denotationally these are not separate products. They are restricted views of
one anchor- and schema-scoped semantic program. A partial query, a partial
olog, a migration preview, and a reconciliation preview should all be able to
ask the same questions:

- what is currently well-typed,
- what semantic ids are in play,
- what obligations remain,
- and what admissible next refinements preserve the current trust boundary.

What is still missing if we want this to feel materially closer to
Lean/Haskell-style typed exploration rather than "better query errors":

- typed holes should extend beyond the currently implemented query + typed-olog
  slice into CQ authoring, migration authoring, reconciliation authoring, and
  implementation-surface mapping;
- refinement candidates should be machine-applicable objects, not only
  explanation strings;
- the checker should expose expected result shape and expected repair/proof
  obligations as first-class IDE/API data;
- agents and editors should be able to ask "what fills this hole while
  preserving certifiability / accepted-world meaning / lifecycle legality?";
- and the runtime should treat partially known authoring states as legitimate
  typed states instead of forcing early collapse into either free text or hard
  failure.

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

Throughout this document, "LLM-assisted" should be read narrowly: MCP/skill/API
plugin surfaces, typed tool-loop integrations, and evidence-plane proposal
generators that consume or emit structured IR and trust metadata, not
free-form semantic authority or rewrite-only behavior.

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

Current runtime slice:

- proposal preview validation now carries a uniform `trust` contract plus
  optional CQ before/after coverage deltas,
- proposal authoring tools can gate previews on `fail_on_regression` and
  `fail_on_unsatisfied_after`,
- and `draft_axi_from_proposals` distinguishes a plain rendered draft from a
  Rust-side `validated` canonical draft carrying an `axi_well_typed_proof_v1`
  summary.

This is still not the full typed-authoring target. It is the first useful
operational step: the authoring loop can now show whether a proposed delta is
well-typed, which competency questions it helps or leaves unsatisfied, and what
trust class applies before review/promotion.

The next useful step is to make the authoring response contract uniform across
editor, CLI, and server surfaces. When a user asks "what happens if I add or
rename this?", the response should include:

- a canonical draft or delta artifact,
- repair-oriented typing diagnostics,
- CQ before/after status,
- `schema` / `theory` / `instance` / `context` impact summary,
- trust-contract fields and anchors,
- and links to the supporting evidence that motivated the draft.

The same principle should hold for exploration. Suggestions should not stop at
prose like "you might add a type guard". The runtime should emit
machine-applicable refinement candidates that an editor, agent, or MCP/skill
surface can turn into concrete patches over query fragments, olog fragments,
migration drafts, or reconciliation choices.

That is concrete value for ontology authors because it makes changes reviewable
before they mutate accepted meaning. It is concrete value for system builders
because it gives one stable contract to implement across authoring surfaces.

### 9.7 One typed operational currency

The deepest operational bug in ontology tooling is often not "bad logic". It is
that the same semantic change appears in five disconnected forms:

- as a canvas edit in an editor,
- as a text diff in `.axi`,
- as an elaborated query plan,
- as a migration preview,
- and as a certificate payload.

When those forms do not share one typed core, meaning drifts even if each local
tool looks reasonable.

The target here is therefore not merely "more types". The target is one family
of typed operational artifacts over the same anchors and IR ids. The exact Rust
names may evolve, but the conceptual handles should look like:

- a canonical draft or semantic delta over a base anchor,
- a prepared query handle over compiled schema/theory ids,
- a migration preview handle over a source and target anchor plus a schema
  morphism,
- and certificate-bearing answer handles tied to the same semantic objects.

In other words:

- authoring should not invent ids that query or migration tooling cannot see,
- query elaboration should not depend on runtime-only string heuristics that
  certificates cannot name,
- migration should not be explained only as a storage transform,
- and certificate emission should not be the first place where semantics become
  explicit.

Once those surfaces share one typed currency, several things become possible at
once:

- semantic diff can talk about the same objects the editor talks about,
- CQ preview can attach directly to the same authored delta,
- migration obligations can be named against the same schema/category objects,
- and "certified answer" becomes a typed refinement of an ordinary answer rather
  than a separate reporting universe.

That is the operational heart of "making it one" in this repo.

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

For evidence-plane ontology work, grounding should be treated as an admission
contract for review tooling. A minimally admissible proposal bundle should carry:

- a proposal-set identity or digest,
- the accepted snapshot or exported source snapshot it was derived from,
- source document/chunk/record references,
- context/world anchors when the proposal is scoped,
- proposer/run identity (LLM run, world-model run, import job, or human draft),
- and the current ontology objects it claims to extend, contradict, or refine.

Bundles that lack those fields may still be useful exploration artifacts, but
they are not yet review-ready ontology proposals.

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

For user usefulness, "explicit and typed" means more than "rendered into text".
The proposed axiom, schema extension, or olog fragment should be packaged with:

- the candidate canonical delta,
- the evidence that motivated it,
- the typing result against the current kernel IR,
- the CQ and migration impact preview,
- and the trust contract that explains what is soundly known versus still
  heuristic.

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
7. Attach a trust contract to each proposal set (`trust_class`, `coverage`,
   `anchor provenance`) so tooling can refuse acceptance when CQ regressions or
   unknown-coverage rules are triggered.
8. Promote only reviewed canonical changes into accepted `.axi`.

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

In practice, a useful CQ asset should carry:

- a stable CQ identifier,
- the accepted snapshot or review baseline it was authored against,
- an expected answer shape or admissibility condition,
- the contexts/worlds where it is meant to hold,
- and the reason for failure when it does not currently hold
  (missing typing, conflicting theory, missing data, unsupported query form,
  heuristic-only answer path, etc.).

This lets ontology authors distinguish "the ontology is missing something" from
"the system found an answer but cannot soundly certify it". It also lets system
builders treat CQ results as typed policy inputs instead of human-only notes.

### 10.6 Grounded axiomatization contract

The production path from evidence to acceptance should remain explicit:

- AI extraction proposes typed candidates;
- grounded proposer metadata and anchors are attached;
- compiler and trusted slices classify each proposal as `certified`, `execution-only`,
  or `mixed` with explicit coverage/soundness notes;
- proposal bundles that fail typing or trust-class checks are retained as
  evidence-plane assets, not silently discarded;
- only reviewed and promoted typed proposals enter the accepted plane.

A good operational rule is:

> anything without explicit grounding, typed status, and trust scope is evidence
> for ontology work, not ontology state.

That rule is what keeps AI-assisted axiomatization useful. It lets the system be
aggressive in discovery while staying conservative in meaning changes.

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
- evidence-plane proposal protocols for ingest, LLMs, and world models,
- a first CQ-gated proposal-preview slice with trust metadata,
- first runtime trust-contract fields on query/certificate/proposal-preview paths,
- and the first `sem/` lifecycle slice for persisted world-model run records.

### 13.2 What is not yet real

- a fully internalized categorical ontology kernel,
- a complete olog authoring frontend,
- theorem-backed `Delta/Sigma/Pi` migration semantics end to end,
- complete type-driven query semantics for all query forms,
- fully first-class context/sheaf semantics in the runtime kernel,
- or a complete semantic VCS implementation.

More concretely:

- semantic VCS is still only a first slice: `sem/` exists, but mostly as
  scaffolding plus persisted `world_model_runs`, not as the full semantic branch
  / review / merge / promotion history;
- semantic commits are still evolving toward explicit state-plus-delta objects
  with ancestry, policy metadata, and semantic diff payloads, rather than being
  the current universal unit of ontology mutation;
- trust contracts are intentionally about soundness class, coverage, anchors, and
  scope, not about completeness of answer sets or full ontology closure;
- CQ-gated evolution is real for proposal preview, but not yet uniformly the
  gate for migration preview, semantic merge, and accepted-plane promotion;
- and typed olog authoring is partial: the draft/validation path exists, but the
  full author-review-merge workbench does not.

### 13.3 What should come next

If the goal is "typed ontology engineering that is genuinely useful", the next
technical priorities are:

1. Canonicalize compiled semantics.
   Make the kernel IR explicit and deterministic across more of the system, and
   use its stable ids as the common references for authoring, query,
   migration, diff, and certification.

2. Push public Rust APIs onto typed anchors and lifecycle states.
   The useful endpoint is not "some typestate wrappers exist", but that normal
   operational handles are prepared queries, semantic deltas, migration previews,
   and certified answers indexed by accepted anchors and schema ids.

3. Make typed query/tooling surfaces first-class for users.
   Query elaboration, result-shape reporting, certifiability classification,
   trust contracts, and verification hooks should be ordinary API data across
   CLI, server, and LLM/tool-loop surfaces.

4. Add an olog authoring and review surface that lowers to the same IR and
   preserves proposal provenance.
   This is where the categorical story becomes a practical modeling surface
   rather than explanation-only theory.

5. Tighten the Lean boundary around the semantics that matter most.
   Extend from the current path/rewrite/constraint slice toward checked rewrite
   admissibility, certifiable IR well-formedness, and narrow query/migration
   witness checking, while keeping soundness/completeness language honest.

6. Make semantic VCS history, semantic diff, and reconciliation first-class,
   with semantic commits that carry both state anchors and explicit deltas.
   Review/merge/promotion should become typed ontology operations, not only file
   operations with side metadata.

7. Keep AI-generated ontology structure in the evidence plane until grounded,
   typed, reviewed, and promoted.
   The more aggressive discovery becomes, the more important the typed review
   boundary becomes.

8. Require CQ-driven migration previews before schema changes are accepted, and
   make the same CQ gate apply at review/merge/promotion boundaries.
   This is where useful dependent typing becomes visible to ontology authors:
   the system can say exactly which domain questions stay valid, which regress,
   and what remains only heuristic.

The stronger product claim becomes honest only when these items compose into one
operational story rather than shipping as isolated features.

### 13.4 Axiograph as an engine for coding agents

One of the most important practical uses for typed ontology engineering is not
only ontology editing. It is **agentic engineering**: using the ontology layer
to help coding agents understand business systems, make defensible
implementation claims, find missing rules, and surface where the current model
is too weak for automation.

The useful question is not:

> can an agent mention ontology terms while editing code?

The useful question is:

> can an agent make anchored, typed, scoped claims about what a business system
> means, what rules apply, what is implemented correctly, what remains unknown,
> and what ontology or code changes are needed next?

That is the point where Axiograph stops being "a knowledge graph with better
docs" and starts becoming a serious semantic engine for engineering work.

#### 13.4.1 What coding agents actually need

For engineering workflows, the high-value questions are usually of these forms:

- "what business rules apply to this endpoint, workflow, report, or job?"
- "which of those rules are actually checked, and which are only implied by
  comments, docs, or historical behavior?"
- "does this implementation preserve the required business meaning under the
  current ontology version and context?"
- "what semantic areas are covered by the ontology, and what parts of the code
  are only text-grounded or still out-of-schema?"
- "what changed between ontology version A and version B, and which claims got
  stronger or weaker?"
- "what type-directed code, tests, queries, or migrations should be generated
  from the current business semantics?"

These are not ordinary vector-search questions. They are questions about:

- scoped truth,
- typed obligations,
- lifecycle state,
- and semantic change under versioned worlds.

That is why Axiograph's value for coding agents should be described as
**semantic engineering support**, not merely retrieval augmentation.

#### 13.4.2 Business rules as explicit semantic obligations

In business systems, many implementation failures are not low-level type
mistakes. They are failures to preserve business meaning:

- a refund is issued without the required approval path,
- a contract state transition skips a legal prerequisite,
- an authorization rule is implemented more weakly than policy text states,
- a reporting transformation changes the meaning of "active customer",
- a migration preserves rows but breaks semantic comparability across versions.

For Axiograph to be useful here, business rules should not remain prose-only.
They should be represented as typed engineering obligations with explicit trust
class:

- certifiable rules in a conservative checked subset,
- execution-only typed rules that are still structurally meaningful,
- and weaker evidence-backed rules that remain explicitly heuristic.

Operationally, the surface that makes this useful day to day is the Rust-side
runtime checker. That is where an agent should be able to ask:

- which accepted or review-state rule objects match this code path,
- which checks are available now without waiting for a certified witness path,
- which contexts/worlds qualify the rule,
- and whether the current answer is runtime-checked, Lean-certifiable, already
  Lean-certified, or only retrieval-supported.

The engineering advantage is that an agent can then produce a report such as:

- which rule it thinks applies,
- which ontology object/theory/context grounded that rule,
- whether the rule is strongly or weakly supported,
- which code/config/data evidence was checked,
- and what residual unknowns remain.

That is dramatically more useful than "the docs suggest this should happen".

#### 13.4.3 Implementation correctness as a semantic claim problem

For coding agents, "correctness" should not mean only unit tests pass.
Implementation correctness should be stated relative to business semantics.

The right pattern is:

- ontology expresses the intended semantic obligations,
- code/config/data provide the current implementation evidence,
- queries/CQs/rule checks expose the observable behavior,
- and the final agent claim is tagged by trust strength and scope.

That yields a more precise correctness vocabulary:

- `checked_against_certified_obligation`
  - the claim is backed by a checked semantic slice under explicit anchors.
- `checked_against_execution_only_obligation`
  - the obligation is typed and checked by runtime logic, but not in the trusted
    kernel.
- `retrieval_supported_but_unchecked`
  - the claim is grounded in docs/code evidence, but not semantically checked.
- `speculative`
  - the agent is proposing a hypothesis or repair direction.

This is the correct reading of "strong vs weak claims" for engineering. Strong
claims are not universally true claims. They are claims that are **strong under
stated anchors, contexts, and checked obligations**. Weak claims are the ones
that remain evidence-backed or heuristic.

#### 13.4.4 Strong vs weak claims across ontology versions and worlds

Business systems rarely have only one ontology state. They have:

- accepted snapshots,
- review branches,
- migrations in flight,
- context/world variants,
- and temporal or regulatory scopes.

So an engineering claim should almost never be read as unqualified truth. It
should be read as:

> under ontology version `A`, context/world `W`, and trust contract `T`, this
> proposition holds with strength `S`.

This matters operationally because coding agents often need to answer questions
like:

- "was this rule true before the migration, or only after?"
- "is this invariant global or only true in the regulatory world?"
- "did this change weaken a previously certified claim into an execution-only
  one?"
- "is the current patch valid for `main`, for `review/billing`, or only for a
  future target ontology?"

A useful agent-facing claim object therefore needs:

- the proposition,
- the ontology/snapshot anchor,
- the context/world scope,
- the trust class and soundness clause,
- the coverage clause,
- and the weakening caveats.

Without that structure, "strong vs weak claim" becomes vague confidence theater.
With that structure, it becomes a disciplined engineering report.

#### 13.4.5 Semantic coverage is not retrieval coverage

For agentic engineering, semantic coverage should not be reduced to "did search
return a relevant paragraph".

Coverage should mean at least:

- rule coverage
  - which business obligations have explicit typed representation?
- CQ coverage
  - which operational domain questions are executable and satisfied?
- schema/theory coverage
  - which concepts and relations are actually modeled?
- context/world coverage
  - which scopes are represented explicitly, and which are flattened away?
- implementation-surface coverage
  - which APIs, workflows, reports, and migrations are tied to ontology objects?

This is where ontology-backed RAG becomes valuable. A mixed agent answer should
be able to say:

- here is the retrieved documentary evidence,
- here is the typed ontology/query result,
- here is what is certified versus execution-only,
- and here is what is not currently modeled and therefore remains weak.

That is a much better engineering interface than merging all evidence into one
opaque relevance score.

#### 13.4.6 Type-directed programming over business systems

If the ontology layer is genuinely useful to coding agents, it should do more
than explain existing systems. It should help **shape** new code and change
plans.

The important type-directed effects are:

- schema-scoped handles for business objects and relation roles,
- prepared/typechecked query and rule handles,
- context/world-scoped references to rules and facts,
- lifecycle-aware references to accepted/reviewed/proposed artifacts,
- and legal-next-step guidance for workflows and state transitions.

That means the ontology should help agents synthesize:

- code skeletons,
- migration obligations,
- semantic tests and CQ suites,
- validation rules,
- and change plans that already know which invariants matter.

This is especially useful for business software because so much of the logic is
really "typed workflow over domain commitments" rather than algorithmic novelty.

#### 13.4.7 Where HoTT, categories, and dependent-type ideas help

The rich mathematics in the repo should be used where it clarifies engineering
structure, not where it merely increases theoretical ambition.

For coding-agent workflows, the productive uses are:

- relation-as-object plus projection arrows
  - good for workflows, approvals, obligations, and attributed business facts.
- path/rewrite/groupoid structure
  - good for equivalent rule formulations, refactor equivalence, and
    normalization of business logic representations.
- commuting diagrams
  - good for showing that two implementation or migration paths preserve the
    same business effect.
- context/world semantics
  - good for feature flags, regulatory worlds, tenant-specific interpretations,
    temporal validity, and "unknown is not false".
- dependent-type effects in Rust/API surfaces
  - good for preventing agents from mixing snapshot, schema, lifecycle, or
    context identities.
- typed discovery and axiomatization
  - good for turning repeated code/doc/data patterns into candidate constraints,
    rewrite rules, path equations, and ontology extensions.

This is the right product framing:

- use HoTT/categorical structure to make semantic equivalence and transport
  explicit,
- use dependent-type effects to make unsafe agent actions harder to express,
- and use the ontology layer to turn weak engineering intuitions into typed,
  reviewable candidate axioms.

#### 13.4.8 A useful agent workflow

The most defensible medium-term workflow looks like this:

1. agent retrieves documents/code/config/data evidence,
2. agent asks ontology-backed queries over the current accepted snapshot and
   stated contexts/worlds,
3. agent obtains business rules, CQ assets, and current trust contracts,
4. agent checks implementation claims against typed obligations,
5. agent reports strong claims, weak claims, unsupported areas, and required
   follow-up tests or ontology extensions,
6. agent proposes code changes and, when needed, candidate ontology/rule deltas,
7. those deltas go through CQ-gated preview, semantic diff, and review before
   accepted meaning changes.

The point is not "fully autonomous semantic programming". The point is a
reviewable loop where the semantic layer makes agent outputs more legible,
better grounded, and harder to overclaim.

#### 13.4.9 Current slice and honest limitation

The current repo already contains the beginnings of this story:

- typed anchors and lifecycle wrappers,
- trust-contract language,
- CQ-driven preview slices,
- typed query/certifiability classification,
- evidence-plane proposal flows,
- and migration/category scaffolding.

What it does **not** yet have is the full integrated agentic-engineering engine:

- one first-class preview object across proposal review, migration preview,
  merge, and promotion,
- a canonical claim object for strong/weak engineering claims across versions
  and worlds,
- a complete business-rule checking service over code/config/data evidence,
- or a fully productized type-directed programming interface over business
  systems.

So the right present-tense claim is still conservative:

> Axiograph already has the architectural pieces to become a strong semantic
> engine for coding agents, but the fully integrated rule/correctness/coverage
> workflow remains roadmap work.

#### 13.4.10 Canonical co-evolution example: chemical plant + business system

To see what is still missing, it helps to force a hard example rather than a
toy ontology.

Take a chemical plant that has to co-evolve all of the following:

- physical process models:
  equipment, streams, units, recipes, thermodynamics, control loops, alarms,
  and simulation/digital-twin artifacts;
- operational/business process:
  material delivery, vendor certification, lot genealogy, maintenance windows,
  shift operations, release approvals, regulatory reporting, and CAPA flows;
- enterprise systems:
  ERP/MRP, pricing, contracts, inventories, orders, delivery promises,
  financial postings, and forecast models;
- software and automation:
  Go/Python/TypeScript/C/C++ services, PLC logic, optimizers, simulators,
  containers, batch jobs, APIs, HMI/UX screens, and reporting pipelines;
- documentary evidence:
  wikis, SOPs, incident reports, P&IDs, instrument specs, QA certificates,
  procurement records, and model cards.

In that setting, ontology usefulness is not "can we label entities cleanly?" It
is:

- can the ontology tell us whether the simulator, ERP workflow, PLC logic, and
  compliance reports still refer to the same business/physical commitments?
- can we detect when a plant recipe or control invariant changed physically but
  the pricing, certification, or delivery logic did not change with it?
- can an agent tell which code/tests/docs/process assets are semantically
  uncovered after a model change?
- can we distinguish accepted plant semantics from review-state hypotheses and
  evidence-plane suggestions discovered from documents or code?

For that class of system, the right typed ontology architecture has to model at
least these implementation surfaces:

- APIs and jobs,
- simulators and optimization models,
- PLC routines and control modes,
- HMI screens and operator workflows,
- reports and regulatory extracts,
- config/state machines,
- and document sections or wiki procedures.

And it has to model these ontology dimensions explicitly:

- context/world:
  regulatory region, plant, tenant, operating mode, what-if simulation world,
  historical effective date;
- temporal validity:
  when an invariant or business rule was active;
- provenance/evidence:
  which source justified a modeled claim;
- uncertainty:
  what is accepted, what is runtime-checked, what is review-only, and what is
  only evidence-backed.

What is still missing for this to become truly useful in that setting:

- typed implementation-surface modeling that reaches beyond endpoints/jobs into
  simulators, PLC assets, HMI screens, reports, and planning models;
- first-class units/dimensions and physically meaningful quantity typing in the
  runtime checker and semantic IR;
- typed temporal/state-machine constraints for operational modes, approvals,
  process phases, and release gates;
- stronger transport/evolution support so ontology changes can explain what
  breaks in simulation, ERP, control logic, or reporting;
- richer code/document/process ingest that can emit typed candidate mappings
  from real artifacts into implementation surfaces and rule scopes;
- and one agent-facing engineering report that unifies rule applicability,
  semantic coverage, drift, and next actions over those surfaces.

This is exactly why the project should be read as ontology-driven development
and co-evolution, not as static ontology curation.

#### 13.4.11 What "LLM-assisted" should mean here

In this repo, "LLM-assisted" should not mean "the model rewrote some text that
looked ontology-shaped".

It should mean the model interacts with typed semantic services through
explicit interfaces:

- tool-loop endpoints,
- stable APIs,
- plugin or skill contracts,
- future MCP-style adapters,
- and typed request/response objects carrying anchors, lifecycle state, trust,
  and residual unknowns.

That matters because free-form assistant prose is not enough for disciplined
ontology engineering. Agents need structured access to:

- typed query elaboration and hole filling,
- olog fragment checking,
- rule applicability,
- semantic coverage and drift,
- CQ assets and preview gates,
- and semantic VCS lineage.

The product goal is therefore not "AI writes axioms". It is:

> AI uses typed semantic services to discover, ground, propose, compare, and
> repair ontology/code/business deltas under explicit anchors and trust
> contracts.

## 14. A Practical Product Claim

The most accurate product-level claim today is something like this:

> Axiograph is a typed ontology workbench with a Lean-checked semantic kernel for
> conservative gates and typed witness checking, a Rust runtime that makes
> anchors/lifecycle/schema discipline first-class, and an architecture designed
> to support AI-assisted ontology discovery and grounding without surrendering
> truth to heuristics.

That is already a strong claim. It is also one we can defend.

The stronger future claim should remain conditional:

> Axiograph becomes a dependently typed ontology engine when its Lean-checked
> kernel, compiled schema/category IR, Rust typed handles, typed authoring/query/
> migration/certification surfaces, and semantic VCS lifecycle all line up as
> one operational semantics story.

That is not present tense yet. It is the direction the current architecture is
already organized to support.

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
