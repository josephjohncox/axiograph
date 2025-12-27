//! Semantics-driven tests for “untrusted engine, trusted checker”.
//!
//! These are not just coverage: they encode small, meaningful laws from `docs/explanation/BOOK.md`,
//! and act as regression tests for certificate emitters and migration operators.

use axiograph_pathdb::certificate::{PathRewriteRuleV2, PathRewriteStepV2};
use axiograph_pathdb::*;

fn strip_instance_name(mut instance: InstanceV1) -> InstanceV1 {
    instance.name = "<ignored>".to_string();
    instance
}

// =============================================================================
// Groupoid normalization semantics (v2)
// =============================================================================

#[test]
fn normalize_path_v2_cancels_adjacent_inverse_pair() {
    let step = PathExprV2::Step {
        from: 1,
        rel_type: 10,
        to: 2,
    };
    let inv_step = PathExprV2::Inv {
        path: Box::new(step.clone()),
    };

    // p · p⁻¹ = id
    let expr = PathExprV2::Trans {
        left: Box::new(step),
        right: Box::new(inv_step),
    };

    assert_eq!(
        expr.normalize(),
        PathExprV2::Reflexive { entity: 1 },
        "expected cancellation to normalize to reflexive"
    );

    // The derived rewrite proof should be a single `cancel_head` at the root.
    let (normalized, derivation) = expr.normalize_with_derivation();
    assert_eq!(normalized, PathExprV2::Reflexive { entity: 1 });
    let steps = derivation.expect("expected a derivation for a simple cancellation");
    assert_eq!(
        steps,
        vec![PathRewriteStepV2 {
            pos: vec![],
            rule: PathRewriteRuleV2::CancelHead
        }]
    );
}

#[test]
fn normalize_path_v2_double_inverse_is_noop() {
    let base = PathExprV2::Trans {
        left: Box::new(PathExprV2::Step {
            from: 1,
            rel_type: 10,
            to: 2,
        }),
        right: Box::new(PathExprV2::Step {
            from: 2,
            rel_type: 20,
            to: 3,
        }),
    };

    // (p⁻¹)⁻¹ = p
    let expr = PathExprV2::Inv {
        path: Box::new(PathExprV2::Inv {
            path: Box::new(base.clone()),
        }),
    };

    assert_eq!(expr.normalize(), base.normalize());

    let (normalized, derivation) = expr.normalize_with_derivation();
    assert_eq!(normalized, base.normalize());
    let steps = derivation.expect("expected a derivation for inv(inv(p))");
    assert!(
        steps
            .first()
            .is_some_and(|s| s.rule == PathRewriteRuleV2::InvInv),
        "expected the first rewrite step to be inv_inv"
    );
}

// =============================================================================
// Proof-mode execution traces
// =============================================================================

#[test]
fn execute_with_mode_records_query_shape_when_enabled() {
    let mut db = PathDB::new();

    let a = db.add_entity("Thing", vec![("name", "a")]);
    let b = db.add_entity("Thing", vec![("name", "b")]);
    db.add_relation("r", a, b, 1.0, vec![]);
    db.build_indexes();

    let query = PathQuery::Join(
        Box::new(PathQuery::SelectByType("Thing".to_string())),
        Box::new(PathQuery::SelectRelated(a, "r".to_string())),
    );

    let proved = db.execute_with_mode::<WithProof>(&query);
    assert!(proved.value.contains(b));
    assert_eq!(
        proved.proof,
        vec![
            QueryExecutionEvent::Join,
            QueryExecutionEvent::SelectByType {
                type_name: "Thing".to_string()
            },
            QueryExecutionEvent::SelectRelated {
                source: a,
                rel_type: "r".to_string()
            }
        ]
    );

    let proved_none = db.execute_with_mode::<NoProof>(&query);
    let _: () = proved_none.proof;
}

#[test]
fn with_confidence_filters_low_conf_edges() {
    let mut db = PathDB::new();

    let a = db.add_entity("Thing", vec![("name", "a")]);
    let b = db.add_entity("Thing", vec![("name", "b")]);
    let c = db.add_entity("Thing", vec![("name", "c")]);

    db.add_relation("r", a, b, 0.3, vec![]);
    db.add_relation("r", a, c, 0.8, vec![]);
    db.build_indexes();

    let base = PathQuery::SelectRelated(a, "r".to_string());
    let query = PathQuery::WithConfidence {
        base: Box::new(base),
        min_confidence: 0.5,
    };

    let result = db.execute(&query);
    assert!(!result.contains(b));
    assert!(result.contains(c));
}

// =============================================================================
// Δ_F semantics: functoriality (composition)
// =============================================================================

#[test]
fn delta_f_is_functorial_on_objects_and_arrows() {
    let optimizer = ProofProducingOptimizer::default();

    // S0: A --f--> B
    let schema_s0 = SchemaV1 {
        name: "S0".to_string(),
        objects: vec!["A".to_string(), "B".to_string()],
        arrows: vec![ArrowDeclV1 {
            name: "f".to_string(),
            src: "A".to_string(),
            dst: "B".to_string(),
        }],
        subtypes: vec![],
    };

    // S1: X --g1--> Y --g2--> Z
    let schema_s1 = SchemaV1 {
        name: "S1".to_string(),
        objects: vec!["X".to_string(), "Y".to_string(), "Z".to_string()],
        arrows: vec![
            ArrowDeclV1 {
                name: "g1".to_string(),
                src: "X".to_string(),
                dst: "Y".to_string(),
            },
            ArrowDeclV1 {
                name: "g2".to_string(),
                src: "Y".to_string(),
                dst: "Z".to_string(),
            },
        ],
        subtypes: vec![],
    };

    // S2: U --p--> V --q--> W
    let instance_s2 = InstanceV1 {
        name: "I2".to_string(),
        schema: "S2".to_string(),
        objects: vec![
            ObjectElementsV1 {
                obj: "U".to_string(),
                elems: vec!["u1".to_string(), "u2".to_string()],
            },
            ObjectElementsV1 {
                obj: "V".to_string(),
                elems: vec!["v1".to_string(), "v2".to_string()],
            },
            ObjectElementsV1 {
                obj: "W".to_string(),
                elems: vec!["w1".to_string(), "w2".to_string()],
            },
        ],
        arrows: vec![
            ArrowMapV1 {
                arrow: "p".to_string(),
                pairs: vec![
                    ("u1".to_string(), "v1".to_string()),
                    ("u2".to_string(), "v2".to_string()),
                ],
            },
            ArrowMapV1 {
                arrow: "q".to_string(),
                pairs: vec![
                    ("v1".to_string(), "w1".to_string()),
                    ("v2".to_string(), "w2".to_string()),
                ],
            },
        ],
    };

    // F : S1 → S2  (X↦U, Y↦V, Z↦W), with arrow images g1↦p, g2↦q
    let morphism_f = SchemaMorphismV1 {
        source_schema: "S1".to_string(),
        target_schema: "S2".to_string(),
        objects: vec![
            ObjectMappingV1 {
                source_object: "X".to_string(),
                target_object: "U".to_string(),
            },
            ObjectMappingV1 {
                source_object: "Y".to_string(),
                target_object: "V".to_string(),
            },
            ObjectMappingV1 {
                source_object: "Z".to_string(),
                target_object: "W".to_string(),
            },
        ],
        arrows: vec![
            ArrowMappingV1 {
                source_arrow: "g1".to_string(),
                target_path: vec!["p".to_string()],
            },
            ArrowMappingV1 {
                source_arrow: "g2".to_string(),
                target_path: vec!["q".to_string()],
            },
        ],
    };

    // G : S0 → S1  (A↦X, B↦Z), with f↦(g1 ; g2)
    let morphism_g = SchemaMorphismV1 {
        source_schema: "S0".to_string(),
        target_schema: "S1".to_string(),
        objects: vec![
            ObjectMappingV1 {
                source_object: "A".to_string(),
                target_object: "X".to_string(),
            },
            ObjectMappingV1 {
                source_object: "B".to_string(),
                target_object: "Z".to_string(),
            },
        ],
        arrows: vec![ArrowMappingV1 {
            source_arrow: "f".to_string(),
            target_path: vec!["g1".to_string(), "g2".to_string()],
        }],
    };

    // Δ_G(Δ_F(I))
    let delta_f = optimizer
        .delta_f_v1::<NoProof>(morphism_f.clone(), schema_s1, instance_s2.clone())
        .expect("delta_f should succeed")
        .value;
    let nested = optimizer
        .delta_f_v1::<NoProof>(morphism_g.clone(), schema_s0.clone(), delta_f)
        .expect("delta_f (nested) should succeed")
        .value;

    // Δ_(F∘G)(I)
    let composed = morphism_g
        .then(&morphism_f)
        .expect("morphism composition should succeed");
    let direct = optimizer
        .delta_f_v1::<NoProof>(composed, schema_s0, instance_s2)
        .expect("delta_f (composed) should succeed")
        .value;

    assert_eq!(strip_instance_name(nested), strip_instance_name(direct));
}
