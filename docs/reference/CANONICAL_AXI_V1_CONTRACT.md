# Canonical `.axi` V1 Contract

**Diataxis:** Reference  
**Audience:** parser, compiler, and verifier contributors

This page defines the current `axi_v1` source and AST contract. The machine-readable contract is:

```text
fixtures/canonical/contract/axi_v1_contract.json
```

The contract covers the current finite language. It does not claim parser completeness, general dependent type theory, or unrestricted HoTT semantics.

## Authority

Exact accepted UTF-8 `.axi` bytes are the source authority. The parser does not normalize Unicode, comments, whitespace, newlines, imports, or declarations before revision hashing.

The revision identity uses the `revision` AXIOGRAPH-ID domain. It hashes one length-framed field that contains the exact source bytes.

A normalized AST is only a test view. It is not accepted source, compiled IR, a certificate, or a digest input.

Rust and Lean each read the same source image. Each implementation constructs its own AST. Lean does not read a Rust AST or Rust kernel IR as source meaning.

The Rust parser source map is also outside this contract. Source spans help diagnostics, but they do not change source identity or semantics.

## Ordered Data

The parser preserves each array in source order. This rule applies to imports, declarations, roles, predicates, constraints, equations, rewrites, assignments, set items, and tuple fields.

The module AST stores schemas, theories, and instances in separate arrays. Exact source bytes retain the full textual order across these declaration families.

A consumer must not sort an AST array before contract comparison. The compiler can derive sorted indexes only when a separate IR contract requires them.

## AST Constructors

The machine-readable inventory names 23 declarations. A checker compares this inventory with both source files:

- `rust/crates/axiograph-dsl/src/schema_v1.rs`
- `lean/Axiograph/Axi/SchemaV1.lean`

The following table lists all recursive or sum-type constructors.

| Type | Constructors |
| --- | --- |
| `TypeExprV1` | object, relation object, indexed, refined |
| `RefinementPredicateV1` | equals, member of, cardinality, key, enum, predicate |
| `RoleKindV1` | data, context, world, temporal, parameter, evidence |
| `GeneratorKindV1` | aspect, function |
| `ConstraintV1` | functional, at most, typing, symmetric where in, symmetric, transitive, key, named block, unknown |
| `RewriteOrientationV1` | forward, backward, bidirectional |
| `RewriteVarTypeV1` | object, path |
| `PathExprV3` | variable, reflexive, step, transitive composition, inverse |
| `SetItemV1` | identifier, tuple |

The contract JSON also lists every structure, field, constructor payload, and payload type. The gate extracts each payload type from both source declarations and compares it with this inventory by declaration, field, constructor, and payload position. It does not rely on a list of selected type spellings. Drift-negative tests change `GeneratorDeclV1.reversible`, `EquationV1.lhs`, and a `PathExprV3.step` payload in temporary source copies and require the inventory check to reject each change.

The contract uses `u32` for each cardinality and `at_most` bound. A token is a nonempty sequence of ASCII digits. A token cannot contain a sign, interior whitespace, or Unicode digits. The Rust AST uses `u32`, and the Lean AST uses `UInt32`. The accepted decimal range is 0 through 4294967295. Both parsers reject invalid tokens and larger values at the parse stage.

Runtime parser state, source-map addresses, spans, and parse errors are not semantic AST declarations.

## Surface Forms

An `axi_v1` module has one module header. The module header is the first non-comment, non-whitespace canonical header. A schema, theory, instance, import, or declaration cannot precede it. Ordered imports can follow the module header. Imports cannot occur after a schema, theory, or instance section starts. Both `#` and `--` start a line comment. If a line contains both markers, the first marker from left to right starts the comment.

The lexical whitespace set is ASCII HT, LF, CR, and space. VT, FF, and all non-ASCII whitespace are not syntax whitespace. A physical-line declaration introducer (`module `, `schema `, `constraint `, and the other section or declaration keywords) includes one literal space. The explicit syntax-whitespace set applies at grammar positions that own a whitespace token, including recognized constraint-family separators and recursive-call interiors. The parsers do not use Unicode trimming at syntax boundaries. They preserve supported Unicode in value atoms and opaque equation or constraint text. They do not normalize these bytes.

The grammar token `identifier` uses ASCII letters and underscore. Digits are permitted after the first character. Object and assignment names also use this identifier grammar. A set value atom is a nonempty sequence that excludes syntax whitespace and `,(){}=:`. It can start with a digit. Parsers preserve the atom text exactly; they do not parse or canonicalize it as a number. Thus `001` remains `001`, and `1abc` remains `1abc`.

Schema, theory, instance, equation, rewrite, and named-constraint headers consume the complete header. Each accepts zero or one trailing colon. Both parsers reject a repeated colon or other trailing header text. A module header accepts no colon. For an unrecognized constraint family, text with a colon or no syntax whitespace is a named-header candidate and must satisfy the complete named-header grammar. Unknown constraint text must contain syntax whitespace and no colon. This rule rejects both `constraint Review: trailing` and `constraint Review extra:` instead of reclassifying either as unknown.

A schema can declare these forms:

- object types
- subtypes with an optional named inclusion, using only `Sub < Sup`
- relation objects with one or more ordered roles
- aspects
- functions with an optional reversible marker

An object declaration is exactly `object Name`. The introducer contains one ASCII space. Additional space or HT before the identifier is not canonical and is rejected.

A role type can contain an object, a relation object, an index over earlier roles, or refinements. Parenthesized type and refinement calls permit the declared ASCII syntax whitespace immediately inside either parenthesis and around interior `;` and `|` separators. This rule applies to `relation`, `indexed`, `refined`, `eq`, `in`, `enum`, `key`, `cardinality`, and `predicate`. VT, FF, and non-ASCII whitespace remain invalid syntax separators.

A role-kind annotation must be separated from the role type by whitespace. The annotation is optional, and the default role kind is `data`. Attached forms such as `A@data` are not canonical. A generator's optional `@reversible` marker must also have one or more characters from the declared ASCII syntax-whitespace set before it. Thus space, HT, and an in-line CR separator are canonical. VT, FF, non-ASCII whitespace, and attached markers are rejected.

A theory can contain constraints, equations, and rewrite rules. Both standalone typecheckers check constraint relation and field references against the selected schema. Equations retain their left and right text. Equation names must be unique within a theory.

A theory-level key has the form `constraint key Relation(field, ...)`. The grammar permits zero or more ASCII HT, LF, CR, or space characters between the relation and `(`, after `(`, on either side of each comma, and before `)`. The field list is nonempty and cannot contain an empty element. This grammar is separate from the recursive role refinement `key(earlier_role|...)`, which uses pipe separators and has a different semantic stage.

When both sides use `PathExprV3`, the typecheckers infer one value type for each endpoint variable. A value type is a declared object type or a declared relation object. Both typecheckers apply this rule during initial checks and runtime IR checks. Object subtyping does not add subtyping between relation objects.

The equation check also validates relation existence, carrier roles, composition, and equal left and right endpoints. Carrier selection uses named-role conventions and the declared-order fallback for exactly two data roles. If either side is outside `PathExprV3`, both typecheckers keep it as opaque review material. They still require nonempty sides and a unique equation name.

Rewrite rules use typed object or path variables and `PathExprV3`. `Path` selects the path-type grammar only when the complete `Path(x,y)` or `Path x y` form parses. Each alternative owns the whitespace after `Path`: the parenthesized form permits zero or more canonical syntax-whitespace characters before `(`, and the word form requires one or more before its source name. Otherwise, `Path` and longer identifiers such as `Pathology` remain object-type identifiers. Rewrite object variables must name declared object types, not relation objects. Every variable name is unique across the object and path namespaces, independent of declaration order. Both standalone typecheckers inspect variables, endpoints, relation references, carrier roles, composition, and equal left and right endpoints. A rewrite block has one `vars`, `lhs`, and `rhs` field. It has zero or one valued `orientation` field. Both parsers reject a missing, empty, or repeated field. They do not merge repeated `vars` sections or overwrite an orientation.

The constraint and rewrite surfaces have no compatibility aliases. A qualified field is exactly `Relation.field`; syntax whitespace cannot occur immediately before or after `.`. This adjacency rule applies to `functional`, `at_most`, and qualified symmetric guards. A symmetric guard must use this qualified form; bare `field` is rejected. Closure clauses use `on (...) param (...)` order; the reversed order is rejected. Rewrite authors must use `bidirectional`, not `both`, and `refl(x)`, not `id(x)`. Both parsers reject these forms. The opaque equation text surface separately retains existing category-path notation such as `id(A)`; that notation is not a `PathExprV3` rewrite constructor.

An instance contains ordered assignments. A set item is an identifier or a tuple. A tuple can have a local fact label. Generator tuples reject a repeated field before Rust or Lean constructs a lookup map. This includes repeated `source` fields, repeated `target` fields, and mixed field order.

Finite list and set parsers do not discard empty elements. They reject leading, interior, and trailing elements around `,` or `|`. This rule covers relation roles, indexed roles, refinement arguments, constraint parameters, carriers, guard sets, keys, rewrite variables, set items, and tuple fields. The explicit `{}` set literal remains valid.

## Unsupported Forms

The parser rejects a missing, repeated, or non-first module header. It also rejects a late import.

The parser rejects relation-level role-axis shorthand. Authors must put each role kind on its role. It also rejects an attached role-kind annotation without separating whitespace.

The parser rejects an unknown role annotation and malformed or non-all-consuming schema headers. It also rejects malformed relation, rewrite, set, and delimiter syntax. Before line parsing, both implementations run the same bounded global scan for unmatched or mismatched `()[]{}`, unmatched single or double quotes, quoted escapes, comments, and delimiter nesting above 64. Thus opaque constraint text such as `constraint review_only [` rejects in both parsers rather than bypassing delimiter checks.

An unrecognized nonempty constraint family parses as `ConstraintV1.unknown`. Both parsers preserve the complete trimmed text, including a trailing `on (...)` or `param (...)` phrase; closure-clause parsing runs only after a recognized constraint family is selected. Recognition and parser dispatch use the same syntax-keyword boundary predicate for `functional`, `at_most`, `typing`, `symmetric`, `transitive`, and `key`. A recognized family followed by space, HT, or CR cannot downgrade to opaque text. A named block parses as `ConstraintV1.namedBlock`. These constructors preserve review material, but they do not add executable semantics. A malformed recognized form is different: both parsers reject its invalid shape with `parse.constraint_shape`.

Both standalone typecheckers reject syntactically valid modules with invalid names or types. Examples include duplicate declarations, object/relation name collisions, repeated subtype edges, self or multi-edge subtype cycles, unknown targets, empty or non-earlier dependent-role indexes, empty or inverted refinements, invalid refinement keys, and predicates other than `non_empty`. They also reject supported theory constraints that name unknown relations or fields and typed rewrites that contain unknown or ill-typed references. Parsing these references is distinct from checking them, and canonical formation remains a later stage.

The canonical compiler performs a separate formation step. For example, a standalone parser and typechecker can accept an import name when the supplied closure omits that module. The canonical compiler rejects that closure. Both standalone typecheckers also accept a syntactically and type-valid role refinement `key(earlier_role)`, while formation rejects it because no finite witness is implemented. Authors must use a theory-level `constraint key Relation(...)` for that supported executable shape.

The trusted checker supports only its declared certificate fragments. Parser acceptance does not make an equation, rewrite, constraint, instance, or transport certifiable.

## Acceptance Stages

Keep these stages separate:

1. **Boundary:** Read one bounded byte image and decode UTF-8.
2. **Parse:** Construct the AST without semantic name resolution.
3. **Typecheck:** Validate the conservative module and type rules.
4. **Formation:** Validate the import closure and construct supported canonical IR.
5. **Certificate check:** Reparse the exact source and check one supported claim family.

`lean/Axiograph/VerifyMain.lean` implements the trusted anchor loader. `readFileChecked` reads one bounded byte image, rejects invalid UTF-8, and passes that same decoded text to revision derivation and independent Lean parsing. `loadAxiV1Anchor` derives the revision before it invokes `parseAxiV1`; no Rust AST or compiled IR enters this path.

Success at one stage does not imply success at a later stage. A Rust formation decision is not a Lean theorem.

## Exact-Byte Identity

Comments and newline forms can produce the same AST. They still produce different revision identities.

The contract fixtures include LF, CRLF, changed-comment, and mixed-marker comment sources. The mixed-marker fixture puts `--` before `#` on the module-header line. All four sources have equal normalized AST output and distinct revision digests.

Invalid UTF-8 fails before parsing and revision construction. There is no replacement-character fallback.

## Operational Bounds

The Rust direct parser applies these current maxima:

| Limit | Maximum |
| --- | ---: |
| Source bytes | 4 MiB |
| Source lines | 200,000 |
| Bytes per line | 1 MiB |
| Delimiter depth | 64 |

Both direct parsers apply the delimiter/quote scan and the delimiter-depth maximum of 64. The trusted Lean file and stdio loaders limit one `.axi` module to 4 MiB. Other parser, closure, certificate, and process limits remain in [Security Boundaries](SECURITY_BOUNDARIES.md).

These values are operational limits. They are not theorems about termination, soundness, completeness, or memory use.

EQ16-U03 owns expanded limit parity. This contract records the current limits without claiming that all direct parser entry points enforce identical limits.

## Normalized Comparison

Both parser executables accept this test-only option:

```bash
axiograph_parse_axi_v1 --contract-ast-v1 module.axi
```

Rust serializes its parsed AST. Lean constructs the same JSON shape from its independently parsed AST. The test compares JSON values and preserves all array order.

The test never sends Rust JSON to Lean. The normalized JSON cannot enter `CanonicalCompiler` or `VerifyMain` as source meaning.

## Differential Envelope

The parser and standalone typechecker binaries also accept this test-only option:

```bash
axiograph_parse_axi_v1 --contract-envelope-v1 module.axi
axiograph_typecheck_axi --contract-envelope-v1 module.axi
```

Each command emits one closed JSON envelope. The envelope identifies the implementation, requested stage, observed stage, and decision. It includes the normalized AST only after parsing succeeds. The typechecker also includes its finite summary after typechecking succeeds. The runner validates the complete recursive normalized-AST shape and the exact typecheck or formation summary fields. It rejects Boolean versions, missing or extra nested fields, wrong nested scalar types, and malformed matching output from both implementations.

The parser and typechecker do not assign a rejection class. The differential runner derives that class from the exact source, observed stage, and implementation-specific diagnostic. This rule prevents a parser from approving its own incorrect classification.

The Rust formation helper emits the same closed envelope for one bounded module. Its formation class is a Rust decision-procedure result. It is not a Lean result or a proof.

Run the deterministic differential gate:

```bash
make verify-axi-contract-differential
```

The gate reads `fixtures/canonical/contract/corpus.json`. That corpus binds the generator version, seed, case count, source bounds, process bounds, stable ordering, generated-case digest, and each replayable grammar choice. SplitMix64 selects a fresh deterministic permutation of all grammar families in each generation block. It also selects comment, schema-header, and declaration-order alternatives.

On a generated differential failure, the runner applies bounded deterministic delta debugging to lines and then Unicode code points. Each candidate must reproduce the original failure. The runner replays the minimized candidate and includes its exact source, digest, attempt bound, and size reduction in the fail-closed diagnostic.

The gate runs twice and compares its reports byte-for-byte. Measured duration is excluded from the report. A timeout, output overflow, malformed envelope, zero cases, duplicate case ID, or unknown class fails the gate.

Hand-authored cases bind exact source hashes and revision digests. Selected positive cases also bind hand-written AST values. An unsupported opaque constraint has a separate expected outcome, even when parse, typecheck, and formation all succeed. Closed semantic coverage probes replace free-form section labels. Each probe checks the source, golden AST, expected stage transition, or bound witness before execution. The report records the hand case and passed runtime observation for every required contract section. The operational-bound probe runs the exact-N fixture through both binaries and checks that the runner rejects its N+1 mutation before execution.

The report is local test evidence. It does not become accepted source, compiled IR, or a certificate.

## Rejection Classes

The contract JSON defines a closed cross-language rejection taxonomy for the boundary, parse, and standalone typecheck stages that this gate executes in both languages. For each rejected fixture and matrix cell, the Python checker derives a class independently from the Rust and Lean exit status, diagnostic family, and exact source stimulus. For source-prioritized parse classes, it also requires each diagnostic to match the source-specific reason family. An unrelated parse diagnostic cannot acquire a class from the stimulus alone. The two observed stages, classes, and applicable reason families must agree with the declaration. Every class in `rejection_classes` must have at least one runtime observation.

Formation and unsupported-fragment values are listed separately in `non_cross_language_outcome_classes`. This gate does not claim cross-language diagnostic taxonomy for them. The Make target runs separate Rust compiler tests for the current formation cases, but the Python checker does not interpret fixture `formation` fields.

The mechanized taxonomy asserts stage, class, and source-specific reason-family parity. Rust and Lean diagnostic strings can differ within an allowed reason family. The classes do not replace typed Rust errors or the Lean checker result.

## Checks

Run the focused contract gate:

```bash
make verify-axi-v1-contract
```

The gate performs these checks:

1. It compares all Rust and Lean AST declaration fields and constructors with the contract inventory and mechanically checks every structure-field and constructor payload type. Separate drift-negative tests require representative non-`u32` source type changes to fail. The same check binds the shared `u32`/`UInt32` numeric domain.
2. It verifies each fixture hash.
3. It compares independent Rust and Lean normalized AST values.
4. It checks the hashed fixture suite for parse and standalone typecheck outcomes.
5. It runs the hashed 1,107-case adversarial matrix against both languages.
6. For each accepted matrix parse, it compares independently normalized Rust and Lean ASTs and runs both standalone typecheckers.
7. The matrix stores 73 focused cases and 1,034 mechanically generated Cartesian boundary cells. The runner reconstructs the required products and rejects an omitted, duplicated, relabeled, or hand-edited cell or axis. The products contain three object-declaration separator cells, 144 recognized-family separator cells, 120 theory-key delimiter/list cells, 352 analogous constraint-delimiter cells, 80 qualified-field adjacency cells, 25 subtype operator/inclusion cells, 24 `u32` cells, 14 rewrite `Path` cells, and 272 recursive-constructor cells. Object coverage accepts exactly one ASCII space and compares the normalized Rust/Lean AST. It rejects an additional space or HT before the identifier through both the ordinary parse and normalized-AST entry points. Theory-key coverage includes valid, malformed-list, and invalid-reference outcomes at every interior delimiter position. Recursive coverage addresses every applicable internal delimiter position, not only the opening parenthesis. Rejected whitespace classes include VT, FF, NBSP, and EM SPACE. The subtype product accepts only `<`, with an optional complete `as Inclusion`, and rejects every other declared combination. The hashed physical suite contains 72 fixtures.
8. For every cross-language rejection, it derives the Rust and Lean stage/class from actual process exits and diagnostics, requires parity and the expected class, and proves that every declared rejection class was observed. Source-prioritized lexical-domain, noncanonical-alias, numeric-domain, and list-syntax classifications require an allowed source-specific diagnostic reason in each language. Mutation tests reject changed labels, swapped stages, unrelated same-stage diagnostics, and different Rust/Lean reasons for each of those four classes.
9. It checks exact-byte Rust and Lean revision parity.
10. It checks invalid UTF-8 rejection.
11. The Make target runs the Rust `axi_v1_contract` compiler tests for current formation cases.

The required aggregate checks remain:

```bash
make verify-axi-parse-e2e
make verify-identity-parity
python3 scripts/check_book.py
```
