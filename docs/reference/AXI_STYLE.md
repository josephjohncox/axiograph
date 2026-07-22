# `.axi` Authoring Style

**Diataxis:** Reference  
**Audience:** contributors

This document defines the preferred canonical authoring style for `axi_v1`.

The short version:

- write one clear schema/theory/instance story,
- prefer explicit roles over shorthand,
- keep relation semantics readable at the declaration site,
- and keep ontology files focused on typed meaning rather than runtime essays.

## Principles

1. Treat `axi_v1` as the single canonical authoring surface.
2. Prefer explicit roles over shorthand or positional meaning.
3. Model n-ary facts as named relations with readable role names.
4. Keep schema, theory, and instance sections visually separate.
5. Use comments to explain domain meaning, not to compensate for unclear syntax.
6. Keep backend/runtime/process notes in docs unless they are required to
   understand the ontology itself.

## Canonical Shape

Canonical modules should read like this:

```axi
module Example

schema ExampleSchema:
  object Person
  object Project
  object Context
  object Time

  relation Assigned(person: Person, project: Project, ctx: Context, time: Time)

theory ExampleRules on ExampleSchema:
  constraint key Assigned(person, project, ctx, time)

instance ExampleData of ExampleSchema:
  Person = {Alice}
  Project = {Migration}
  Context = {Plan}
  Time = {T0}

  Assigned = {
    (person=Alice, project=Migration, ctx=Plan, time=T0)
  }
```

## Roles, Not Mystery Fields

Choose role names that explain the semantic job of each participant:

- `employee`, `manager`
- `from`, `to`
- `quantity`, `unit`
- `ctx`, `time`

Avoid generic names unless the domain really demands them:

- avoid `x`, `y`, `z` in schemas
- avoid `value` when a better role exists such as `speed`, `amount`, `quality`

## Explicit Scoping

Preferred canonical style is explicit scoping roles:

```axi
relation Parent(child: Person, parent: Person, ctx: Context, time: Time)
```

The parser still accepts shorthand such as:

```axi
relation Parent(child: Person, parent: Person) @context Context @temporal Time
```

but that is an accepted shorthand, not the preferred authoring form. Use
explicit `ctx` / `time` roles in new examples and cleaned modules.

## Constraints

Keep constraints close to the relation family they govern and write them in the
canonical structured form whenever possible:

```axi
constraint functional ReportsTo.employee -> ReportsTo.manager
constraint key MeasurementObs(run, quantity, ctx, time)
constraint symmetric Relationship where Relationship.kind in {Friend}
constraint transitive Accessible on (from, to)
```

Guidelines:

- use explicit relation-qualified fields for `functional`
- use composite keys rather than prose comments for tuple identity
- use `on (role0, role1)` when the carrier pair is not visually obvious
- use `param (...)` when closure semantics are fibered by context/time/etc.

If a theory note is not yet executable/certifiable, keep it obviously
non-semantic or move it to explanation docs. Do not make opaque prose look like
runtime-trusted theory.

## Rewrite Rules

Rewrite rules should stay small, typed, and reviewable:

```axi
rewrite manager_inverse_reports_to:
  orientation: bidirectional
  vars: e: Person, m: Person
  lhs: step(m, ManagerOf, e)
  rhs: inv(step(e, ReportsTo, m))
```

Guidelines:

- keep one semantic lesson per rule
- prefer stable domain names over algebraic placeholders in examples
- use `bidirectional` as the canonical spelling
- use `refl(x)` rather than older aliases

## Examples

The example corpus should bias toward:

- small examples that teach one modeling idea clearly
- medium examples that show one realistic workflow end to end
- large domain examples only when they are internally coherent and queryable

Avoid files that are half ontology and half roadmap essay. If a concept needs a
long explanation, put the explanation in `docs/` and keep the `.axi` module
tight.

## Naming

Prefer one of these patterns consistently within a module:

- `UpperCamel` for object types and relation names in conceptual examples
- domain-stable imported naming when the source system already has a canonical
  vocabulary, such as `proto_service_has_rpc`

Within one module, do not mix multiple naming conventions without a reason.

Avoid collisions where an object type and a relation have the same name unless
the distinction is intentional and well-documented.

## Instance Data

Instance data should be:

- small in accepted examples,
- readable without external tooling,
- and typed enough to show the intended ontology shape.

If the real workload is large or literal-heavy:

- keep a small accepted seed instance,
- keep bulk evidence in typed overlays or backend projections outside accepted state,
- and preserve typed queryability through identifiers or typed bins where
  needed.

## Non-Goals

This style guide does not say:

- that every accepted shorthand must be removed immediately,
- that every module must use the same domain naming convention,
- or that all operational notes belong nowhere near ontology files.

It does say:

- new and cleaned authoring should converge on one explicit, role-aware surface,
- and examples should teach the ontology model we actually want users to learn.
