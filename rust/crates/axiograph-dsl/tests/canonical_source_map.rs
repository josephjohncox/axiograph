use std::collections::BTreeSet;

use axiograph_dsl::{
    axi_v1::{
        parse_axi_v1, parse_axi_v1_with_source_map, CanonicalSourceMap,
        CanonicalSyntacticAddressV1 as Address, ConstraintTermV1 as ConstraintTerm,
        RefinementTermV1 as RefinementTerm, RewritePathStepV1 as PathStep,
        RewritePathTermV1 as PathTerm, RewriteSideV1 as Side, RoleTypePathStepV1 as TypeStep,
    },
    schema_v1::{MAX_AXI_LINE_BYTES, MAX_AXI_SOURCE_BYTES},
};

#[test]
fn canonical_source_map_tracks_parser_occurrences_and_nested_carriers() {
    for newline in ["\n", "\r\n"] {
        let text = [
            "module M",
            "# Compny 😀",
            "schema Earlier:",
            "  object Compny",
            "  relation Previous(value: Compny)",
            "schema Actual:",
            "  object Company",
            "  relation Employment(",
            "    first: refined(Company; eq(Compny)), # Compny",
            "    next: indexed(refined(Compny; eq(Compny)); first),",
            "    fact: refined(relation(Employmnt); cardinality(0|1)))",
        ]
        .join(newline);
        let (module, map) = parse_axi_v1_with_source_map(&text).unwrap();
        assert_eq!(
            serde_json::to_vec(&module).unwrap(),
            serde_json::to_vec(&parse_axi_v1(&text).unwrap()).unwrap()
        );
        let spans = &map.role_carriers;
        assert_eq!(spans.len(), 4);
        assert_eq!(
            (
                spans[2].schema_index,
                spans[2].relation_index,
                spans[2].role_index
            ),
            (1, 0, 1)
        );
        assert_eq!(&text[spans[2].bytes.clone()], "Compny");
        assert_eq!(
            spans[2].bytes.start,
            text.find("Compny; eq(Compny)").unwrap()
        );
        assert_eq!(&text[spans[3].bytes.clone()], "Employmnt");
        let wire = serde_json::to_string(&module).unwrap();
        assert!(!wire.contains("role_carriers"));
        assert!(!wire.contains("byte_start"));
    }
}

#[test]
fn canonical_source_map_rejects_noncanonical_subtype_surfaces() {
    for subtype in [
        "subtype Child <: Parent",
        "subtype Child <: Parent as child_to_parent",
        "subtype Child << Parent",
        "subtype Child <= Parent",
        "subtype Child : Parent",
        "subtype Child < Parent as",
        "subtype Child < Parent child_to_parent",
        "subtype Child < Parent as child_to_parent as duplicate",
    ] {
        let source = format!("module M\nschema S\n  object Child\n  object Parent\n  {subtype}\n");
        parse_axi_v1_with_source_map(&source)
            .expect_err("source-map parsing must reject every undeclared subtype surface");
    }
}

#[test]
fn canonical_source_map_large_multiline_relation_scales_with_ordered_segments() {
    use std::fmt::Write;

    // The production cursor has at most S advances and R containment checks for
    // S ordered segments and R carriers. Timings supplement that O(S + R) bound;
    // they are not a flaky wall-clock assertion or a reduction of accepted limits.
    for roles in [25_000, 50_000, 100_000] {
        let mut text = String::from("module Large\nschema S:\n  object A\n  relation R(\n");
        let mut expected = Vec::with_capacity(roles);
        for role in 0..roles {
            write!(text, "    role{role}: ").unwrap();
            expected.push(text.len()..text.len() + 1);
            text.push_str(if role + 1 == roles { "A)\n" } else { "A,\n" });
        }
        assert!(text.len() <= 4 * 1024 * 1024);
        assert!(text.lines().count() <= 200_000);
        let start = std::time::Instant::now();
        let (module, map) = parse_axi_v1_with_source_map(&text).unwrap();
        eprintln!(
            "source-map roles={roles} bytes={} elapsed_us={}",
            text.len(),
            start.elapsed().as_micros()
        );
        assert_eq!(module.schemas[0].relations[0].fields.len(), roles);
        assert_eq!(map.role_carriers.len(), roles);
        // Six module/schema/object occurrences, two relation occurrences, and
        // four occurrences per simple role. This exact structural count proves
        // mapper work grows with parser nodes rather than source spellings.
        assert_eq!(map.occurrences().len(), 8 + 4 * roles);
        for (role, (span, expected)) in map.role_carriers.iter().zip(expected).enumerate() {
            assert_eq!(
                (span.schema_index, span.relation_index, span.role_index),
                (0, 0, role)
            );
            assert_eq!(span.bytes, expected);
            assert_eq!(&text[span.bytes.clone()], "A");
        }
        let start = std::time::Instant::now();
        let ordinary = parse_axi_v1(&text).unwrap();
        eprintln!(
            "ordinary-parse roles={roles} elapsed_us={}",
            start.elapsed().as_micros()
        );
        assert_eq!(
            serde_json::to_vec(&module).unwrap(),
            serde_json::to_vec(&ordinary).unwrap()
        );
    }
}

fn exact<'a>(text: &'a str, map: &CanonicalSourceMap, address: Address) -> &'a str {
    let occurrence = map.occurrence(&address).expect("supported occurrence");
    text.get(occurrence.bytes.clone())
        .expect("UTF-8 boundary in exact input")
}

#[test]
fn canonical_occurrence_map_honors_exact_source_byte_boundary() {
    let mut text = String::from("module Boundary\nschema S:\n  object A\n");
    while text.len() < MAX_AXI_SOURCE_BYTES {
        let remaining = MAX_AXI_SOURCE_BYTES - text.len();
        if remaining == 1 {
            text.push('#');
            break;
        }
        let line_bytes = remaining.min(MAX_AXI_LINE_BYTES + 1);
        text.push('#');
        text.extend(std::iter::repeat_n('x', line_bytes - 2));
        text.push('\n');
    }
    assert_eq!(text.len(), MAX_AXI_SOURCE_BYTES);
    let (_, map) = parse_axi_v1_with_source_map(&text).expect("N-byte source");
    assert_eq!(exact(&text, &map, Address::ModuleName), "Boundary");

    text.push('x');
    let error = parse_axi_v1_with_source_map(&text).expect_err("N+1 source must reject");
    assert!(error.to_string().contains("source exceeds"));
}

#[test]
fn canonical_occurrence_map_exhaustively_addresses_nested_supported_terms() {
    let lines = [
        "module Same",
        "schema Same:",
        "  object Same",
        "  object Other",
        "  relation Same(",
        "    Same: refined(",
        "      indexed(Same; Same|Other);",
        "      eq(Same); in(Same|Other); cardinality(1|2);",
        "      key(Same|Other); enum(Same|Other);",
        "      predicate(Same|Same|Other)) @context)",
        "  function Same: Same -> Same @reversible",
        "theory Same on Same:",
        "  constraint functional Same.Same -> Same.Same",
        "  constraint at_most 12 Same.Same -> Same.Same param (Same, Other)",
        "  constraint typing Same: Same",
        "  constraint symmetric Same where Same.Same in {Same, Other} on (Same, Other) param (Same)",
        "  constraint symmetric Same on (Same, Other) param (Same)",
        "  constraint transitive Same on (Same, Other) param (Same)",
        "  constraint key Same \t( Same\t,\r Other )",
        "  rewrite Same:",
        "    vars: Same: Same, route: Path(Same, Same)",
        "    orientation: backward",
        "    lhs: trans(inv(step(Same,Same,Same)),",
        "      refl(Same)) # Same in a comment is never selected",
        "    rhs: Same",
        "instance Same of Same:",
        "  Same = {Same, Same: (Same=Same, Other=Ünicode)}",
    ];

    for newline in ["\n", "\r\n"] {
        let text = lines.join(newline);
        let (module, map) = parse_axi_v1_with_source_map(&text).expect("supported syntax");
        assert_eq!(module, parse_axi_v1(&text).expect("ordinary parser parity"));
        let has = |address| map.occurrence(&address).is_some();

        for term in [
            RefinementTerm::EqualsValue,
            RefinementTerm::MemberValue { value_index: 0 },
            RefinementTerm::MemberValue { value_index: 1 },
            RefinementTerm::CardinalityMin,
            RefinementTerm::CardinalityMax,
            RefinementTerm::KeyRole { role_index: 0 },
            RefinementTerm::KeyRole { role_index: 1 },
            RefinementTerm::EnumValue { value_index: 0 },
            RefinementTerm::EnumValue { value_index: 1 },
            RefinementTerm::PredicateName,
            RefinementTerm::PredicateArgument { argument_index: 0 },
            RefinementTerm::PredicateArgument { argument_index: 1 },
        ] {
            let predicate_index = match term {
                RefinementTerm::EqualsValue => 0,
                RefinementTerm::MemberValue { .. } => 1,
                RefinementTerm::CardinalityMin | RefinementTerm::CardinalityMax => 2,
                RefinementTerm::KeyRole { .. } => 3,
                RefinementTerm::EnumValue { .. } => 4,
                RefinementTerm::PredicateName | RefinementTerm::PredicateArgument { .. } => 5,
            };
            assert!(has(Address::RoleRefinementTerm {
                schema_index: 0,
                relation_index: 0,
                role_index: 0,
                path: vec![],
                predicate_index,
                term,
            }));
        }
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RoleKind {
                    schema_index: 0,
                    relation_index: 0,
                    role_index: 0,
                },
            ),
            "@context"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::GeneratorKind {
                    schema_index: 0,
                    generator_index: 0,
                },
            ),
            "function"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::GeneratorReversible {
                    schema_index: 0,
                    generator_index: 0,
                },
            ),
            "@reversible"
        );

        let constraint_terms: &[&[ConstraintTerm]] = &[
            &[
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::SourceField,
                ConstraintTerm::Relation {
                    occurrence_index: 1,
                },
                ConstraintTerm::TargetField,
            ],
            &[
                ConstraintTerm::Bound,
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::SourceField,
                ConstraintTerm::Relation {
                    occurrence_index: 1,
                },
                ConstraintTerm::TargetField,
                ConstraintTerm::Parameter { parameter_index: 0 },
                ConstraintTerm::Parameter { parameter_index: 1 },
            ],
            &[
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::Rule,
            ],
            &[
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::Relation {
                    occurrence_index: 1,
                },
                ConstraintTerm::GuardField,
                ConstraintTerm::GuardValue { value_index: 0 },
                ConstraintTerm::GuardValue { value_index: 1 },
                ConstraintTerm::CarrierField { carrier_index: 0 },
                ConstraintTerm::CarrierField { carrier_index: 1 },
                ConstraintTerm::Parameter { parameter_index: 0 },
            ],
            &[
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::CarrierField { carrier_index: 0 },
                ConstraintTerm::CarrierField { carrier_index: 1 },
                ConstraintTerm::Parameter { parameter_index: 0 },
            ],
            &[
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::CarrierField { carrier_index: 0 },
                ConstraintTerm::CarrierField { carrier_index: 1 },
                ConstraintTerm::Parameter { parameter_index: 0 },
            ],
            &[
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                ConstraintTerm::KeyField { field_index: 0 },
                ConstraintTerm::KeyField { field_index: 1 },
            ],
        ];
        for (constraint_index, terms) in constraint_terms.iter().enumerate() {
            for term in *terms {
                assert!(has(Address::ConstraintTerm {
                    theory_index: 0,
                    constraint_index,
                    term: *term,
                }));
            }
        }
        assert_eq!(
            exact(
                &text,
                &map,
                Address::ConstraintTerm {
                    theory_index: 0,
                    constraint_index: 1,
                    term: ConstraintTerm::Bound,
                },
            ),
            "12"
        );
        for (term, expected) in [
            (
                ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
                "Same",
            ),
            (ConstraintTerm::KeyField { field_index: 0 }, "Same"),
            (ConstraintTerm::KeyField { field_index: 1 }, "Other"),
        ] {
            assert_eq!(
                exact(
                    &text,
                    &map,
                    Address::ConstraintTerm {
                        theory_index: 0,
                        constraint_index: 6,
                        term,
                    },
                ),
                expected
            );
        }
        let repeated_constraint_spans = [
            ConstraintTerm::Relation {
                occurrence_index: 0,
            },
            ConstraintTerm::SourceField,
            ConstraintTerm::Relation {
                occurrence_index: 1,
            },
            ConstraintTerm::TargetField,
        ]
        .map(|term| {
            map.occurrence(&Address::ConstraintTerm {
                theory_index: 0,
                constraint_index: 0,
                term,
            })
            .unwrap()
            .bytes
            .clone()
        });
        assert_eq!(
            repeated_constraint_spans
                .iter()
                .map(|span| (span.start, span.end))
                .collect::<BTreeSet<_>>()
                .len(),
            repeated_constraint_spans.len()
        );
        assert!(repeated_constraint_spans
            .iter()
            .all(|span| &text[span.clone()] == "Same"));

        for address in [
            Address::RewriteVariableDeclaration {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 0,
            },
            Address::RewriteVariableName {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 0,
            },
            Address::RewriteVariableType {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 0,
            },
            Address::RewriteVariableObjectType {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 0,
            },
            Address::RewriteVariableType {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 1,
            },
            Address::RewriteVariablePathFrom {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 1,
            },
            Address::RewriteVariablePathTo {
                theory_index: 0,
                rewrite_index: 0,
                variable_index: 1,
            },
            Address::RewriteOrientation {
                theory_index: 0,
                rewrite_index: 0,
                occurrence_index: 0,
            },
        ] {
            assert!(has(address));
        }
        for path in [
            vec![],
            vec![PathStep::TransLeft],
            vec![PathStep::TransLeft, PathStep::InvPath],
            vec![PathStep::TransRight],
        ] {
            assert!(has(Address::RewritePathNode {
                theory_index: 0,
                rewrite_index: 0,
                side: Side::Left,
                path,
            }));
        }
        for (path, term) in [
            (
                vec![PathStep::TransLeft, PathStep::InvPath],
                PathTerm::StepFrom,
            ),
            (
                vec![PathStep::TransLeft, PathStep::InvPath],
                PathTerm::StepRelation,
            ),
            (
                vec![PathStep::TransLeft, PathStep::InvPath],
                PathTerm::StepTo,
            ),
            (vec![PathStep::TransRight], PathTerm::ReflexiveEntity),
        ] {
            assert!(has(Address::RewritePathTerm {
                theory_index: 0,
                rewrite_index: 0,
                side: Side::Left,
                path,
                term,
            }));
        }
        assert!(has(Address::RewritePathTerm {
            theory_index: 0,
            rewrite_index: 0,
            side: Side::Right,
            path: vec![],
            term: PathTerm::Variable,
        }));
        let repeated_step_spans = [PathTerm::StepFrom, PathTerm::StepRelation, PathTerm::StepTo]
            .map(|term| {
                map.occurrence(&Address::RewritePathTerm {
                    theory_index: 0,
                    rewrite_index: 0,
                    side: Side::Left,
                    path: vec![PathStep::TransLeft, PathStep::InvPath],
                    term,
                })
                .unwrap()
                .bytes
                .clone()
            });
        assert_eq!(
            repeated_step_spans
                .iter()
                .map(|span| (span.start, span.end))
                .collect::<BTreeSet<_>>()
                .len(),
            repeated_step_spans.len()
        );

        for address in [
            Address::SetIdentName {
                instance_index: 0,
                assignment_index: 0,
                item_index: 0,
            },
            Address::SetTupleLabel {
                instance_index: 0,
                assignment_index: 0,
                item_index: 1,
            },
            Address::SetTupleFieldRole {
                instance_index: 0,
                assignment_index: 0,
                item_index: 1,
                field_index: 0,
            },
            Address::SetTupleFieldValue {
                instance_index: 0,
                assignment_index: 0,
                item_index: 1,
                field_index: 0,
            },
            Address::SetTupleFieldRole {
                instance_index: 0,
                assignment_index: 0,
                item_index: 1,
                field_index: 1,
            },
            Address::SetTupleFieldValue {
                instance_index: 0,
                assignment_index: 0,
                item_index: 1,
                field_index: 1,
            },
        ] {
            assert!(has(address));
        }
        assert_eq!(
            exact(
                &text,
                &map,
                Address::SetTupleFieldValue {
                    instance_index: 0,
                    assignment_index: 0,
                    item_index: 1,
                    field_index: 1,
                },
            ),
            "Ünicode"
        );

        let addresses = map
            .occurrences()
            .iter()
            .map(|occurrence| occurrence.address.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(addresses.len(), map.occurrences().len());
        assert!(map.occurrences().iter().all(|occurrence| {
            text.is_char_boundary(occurrence.bytes.start)
                && text.is_char_boundary(occurrence.bytes.end)
        }));
    }
}

#[test]
fn canonical_occurrence_map_covers_supported_declarations_and_terms_exactly() {
    let lines = [
        "module Same",
        "import Base",
        "# Same Same 😀 must never be selected",
        "schema Same:",
        "  object Same",
        "  subtype Child < Same as child_as_same",
        "  relation Same(",
        "    Same: refined(",
        "      indexed(Same; Same);",
        "      eq(Same);",
        "      in(Same|Other)), # repeated Same comment",
        "    fact: relation(Same))",
        "  function Same: Same -> Same @reversible",
        "theory Same on Same:",
        "  constraint functional Same.Same -> Same.fact",
        "  constraint dialect Same Same",
        "  constraint Named:",
        "    opaque Same body",
        "  equation Same:",
        "    step(Same,Same,Same)",
        "      = step(Same,Same,Same)",
        "  rewrite Same:",
        "    vars: Same: Same",
        "    lhs: step(Same,Same,Same)",
        "    rhs: refl(Same)",
        "instance Same of Same:",
        "  Same = {",
        "    Same,",
        "    Same: (Same=Same, fact=Same)",
        "  }",
    ];

    for newline in ["\n", "\r\n"] {
        let text = lines.join(newline);
        let (module, map) = parse_axi_v1_with_source_map(&text).expect("canonical parser");
        assert_eq!(module, parse_axi_v1(&text).expect("ordinary parser parity"));

        assert_eq!(exact(&text, &map, Address::ModuleName), "Same");
        assert_eq!(
            exact(&text, &map, Address::ImportName { import_index: 0 }),
            "Base"
        );
        assert_eq!(
            exact(&text, &map, Address::SchemaName { schema_index: 0 }),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::SubtypeInclusion {
                    schema_index: 0,
                    subtype_index: 0,
                },
            ),
            "child_as_same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RelationName {
                    schema_index: 0,
                    relation_index: 0,
                },
            ),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RoleName {
                    schema_index: 0,
                    relation_index: 0,
                    role_index: 0,
                },
            ),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RoleTypeCarrier {
                    schema_index: 0,
                    relation_index: 0,
                    role_index: 0,
                },
            ),
            "Same"
        );
        let complete_type = exact(
            &text,
            &map,
            Address::RoleType {
                schema_index: 0,
                relation_index: 0,
                role_index: 0,
                path: vec![],
            },
        );
        assert!(complete_type.starts_with("refined("));
        assert!(complete_type.ends_with("in(Same|Other))"));
        assert!(complete_type.contains(newline));
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RoleType {
                    schema_index: 0,
                    relation_index: 0,
                    role_index: 0,
                    path: vec![TypeStep::RefinedBase, TypeStep::IndexedBase],
                },
            ),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RoleIndexedOver {
                    schema_index: 0,
                    relation_index: 0,
                    role_index: 0,
                    path: vec![TypeStep::RefinedBase],
                    over_role_index: 0,
                },
            ),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RoleRefinementPredicate {
                    schema_index: 0,
                    relation_index: 0,
                    role_index: 0,
                    path: vec![],
                    predicate_index: 0,
                },
            ),
            "eq(Same)"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::GeneratorSource {
                    schema_index: 0,
                    generator_index: 0,
                },
            ),
            "Same"
        );
        assert_eq!(
            exact(&text, &map, Address::TheorySchema { theory_index: 0 }),
            "Same"
        );
        assert!(exact(
            &text,
            &map,
            Address::ConstraintDeclaration {
                theory_index: 0,
                constraint_index: 0,
            },
        )
        .starts_with("constraint functional"));
        assert!(map
            .occurrence(&Address::ConstraintDeclaration {
                theory_index: 0,
                constraint_index: 1,
            })
            .is_none());
        assert!(map
            .occurrence(&Address::ConstraintTerm {
                theory_index: 0,
                constraint_index: 1,
                term: ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
            })
            .is_none());
        assert_eq!(
            exact(
                &text,
                &map,
                Address::NamedConstraintName {
                    theory_index: 0,
                    constraint_index: 2,
                },
            ),
            "Named"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::NamedConstraintBodyLine {
                    theory_index: 0,
                    constraint_index: 2,
                    body_index: 0,
                },
            ),
            "opaque Same body"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::EquationLeft {
                    theory_index: 0,
                    equation_index: 0,
                },
            ),
            "step(Same,Same,Same)"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RewriteVariables {
                    theory_index: 0,
                    rewrite_index: 0,
                },
            ),
            "Same: Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::RewriteRight {
                    theory_index: 0,
                    rewrite_index: 0,
                },
            ),
            "refl(Same)"
        );
        assert_eq!(
            exact(&text, &map, Address::InstanceSchema { instance_index: 0 }),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::AssignmentName {
                    instance_index: 0,
                    assignment_index: 0,
                },
            ),
            "Same"
        );
        assert_eq!(
            exact(
                &text,
                &map,
                Address::SetItem {
                    instance_index: 0,
                    assignment_index: 0,
                    item_index: 1,
                },
            ),
            "Same: (Same=Same, fact=Same)"
        );

        let mut previous = 0;
        let mut addresses = BTreeSet::new();
        for occurrence in map.occurrences() {
            assert!(occurrence.bytes.start < occurrence.bytes.end);
            assert!(occurrence.bytes.end <= text.len());
            assert!(text.is_char_boundary(occurrence.bytes.start));
            assert!(text.is_char_boundary(occurrence.bytes.end));
            assert!(previous <= occurrence.bytes.start);
            assert!(addresses.insert(occurrence.address.clone()));
            previous = occurrence.bytes.start;
        }
        assert!(!map
            .occurrences()
            .iter()
            .any(|occurrence| text[occurrence.bytes.clone()].contains("😀")));
        assert!(map
            .occurrence(&Address::ObjectName {
                schema_index: 0,
                object_index: 99,
            })
            .is_none());
    }
}

#[test]
fn constraint_source_terms_follow_canonical_ascii_keyword_whitespace() {
    let text = "module M\nschema S\n  object A\n  relation R(left:A,right:A)\ntheory T on S\n  constraint functional\tR.left -> R.right\n  constraint at_most\r1 R.left -> R.right\n  constraint typing\tR: rule_name\n  constraint symmetric\rR\twhere\tR.left\tin\t{A}\ton\t(left,right)\n  constraint transitive\tR\ton\t(left,right)\n  constraint key\rR(left)\n";
    let (_, map) = parse_axi_v1_with_source_map(text)
        .expect("all recognized families accept canonical ASCII keyword boundaries");

    for constraint_index in 0..6 {
        let occurrence = map
            .occurrence(&Address::ConstraintTerm {
                theory_index: 0,
                constraint_index,
                term: ConstraintTerm::Relation {
                    occurrence_index: 0,
                },
            })
            .expect("recognized constraints retain relation source terms");
        assert_eq!(&text[occurrence.bytes.clone()], "R");
    }
    let bound = map
        .occurrence(&Address::ConstraintTerm {
            theory_index: 0,
            constraint_index: 1,
            term: ConstraintTerm::Bound,
        })
        .expect("at_most retains its bound source term after a CR family separator");
    assert_eq!(&text[bound.bytes.clone()], "1");
}
