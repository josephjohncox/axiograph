use axiograph_dsl::schema_v1::{parse_constraint_v1, CarrierFieldsV1, ConstraintV1};

#[test]
fn parses_symmetric_with_param_clause() {
    let c = parse_constraint_v1("symmetric Spouse param (ctx, time)").expect("parse");
    assert_eq!(
        c,
        ConstraintV1::Symmetric {
            relation: "Spouse".to_string(),
            carriers: None,
            params: Some(vec!["ctx".to_string(), "time".to_string()]),
        }
    );
}

#[test]
fn parses_transitive_with_on_and_param_clause() {
    let c = parse_constraint_v1("transitive Accessible on (from, to) param (ctx)")
        .expect("parse");
    assert_eq!(
        c,
        ConstraintV1::Transitive {
            relation: "Accessible".to_string(),
            carriers: Some(CarrierFieldsV1 {
                left_field: "from".to_string(),
                right_field: "to".to_string(),
            }),
            params: Some(vec!["ctx".to_string()]),
        }
    );
}

#[test]
fn parses_param_before_on_even_if_noncanonical() {
    // Parser should accept either suffix order; formatter will canonicalize.
    let c = parse_constraint_v1("symmetric R param (ctx) on (a, b)").expect("parse");
    assert_eq!(
        c,
        ConstraintV1::Symmetric {
            relation: "R".to_string(),
            carriers: Some(CarrierFieldsV1 {
                left_field: "a".to_string(),
                right_field: "b".to_string(),
            }),
            params: Some(vec!["ctx".to_string()]),
        }
    );
}

#[test]
fn rejects_param_clause_on_key_constraints() {
    let err = parse_constraint_v1("key R(a) param (ctx)").expect_err("should error");
    assert!(err.contains("only supported for symmetric/transitive"), "err={err}");
}

#[test]
fn rejects_duplicate_param_clause() {
    let err = parse_constraint_v1("symmetric R param (ctx) param (time)").expect_err("should error");
    assert!(err.contains("duplicate `param"), "err={err}");
}

#[test]
fn rejects_duplicate_on_clause() {
    let err = parse_constraint_v1("transitive R on (a, b) on (c, d)").expect_err("should error");
    assert!(err.contains("duplicate `on"), "err={err}");
}

#[test]
fn rejects_empty_param_list() {
    let err = parse_constraint_v1("symmetric R param ()").expect_err("should error");
    assert!(err.contains("param fields clause expects"), "err={err}");
}

#[test]
fn parses_where_shorthand_and_formats_to_canonical() {
    let c = parse_constraint_v1("symmetric Relationship where relType in {Friend, Sibling}")
        .expect("parse");
    match &c {
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            values,
            carriers,
            params,
        } => {
            assert_eq!(relation, "Relationship");
            assert_eq!(field, "relType");
            assert_eq!(values, &vec!["Friend".to_string(), "Sibling".to_string()]);
            assert!(carriers.is_none());
            assert!(params.is_none());
        }
        other => panic!("unexpected constraint: {other:?}"),
    }

    let formatted = axiograph_dsl::schema_v1::format_constraint_v1(&c).expect("format");
    assert!(
        formatted.contains("where Relationship.relType in {Friend, Sibling}"),
        "formatted={formatted}"
    );
}

#[test]
fn rejects_on_clause_wrong_arity() {
    let err = parse_constraint_v1("symmetric R on (a)").expect_err("should error");
    assert!(err.contains("carrier fields clause expects"), "err={err}");

    let err = parse_constraint_v1("transitive R on (a, b, c)").expect_err("should error");
    assert!(err.contains("carrier fields clause expects"), "err={err}");
}

#[test]
fn formats_suffix_clauses_in_canonical_order() {
    // Parser should accept either order; formatter should emit `on` then `param`.
    let c = parse_constraint_v1("symmetric R param (ctx) on (a, b)").expect("parse");
    let formatted = axiograph_dsl::schema_v1::format_constraint_v1(&c).expect("format");
    assert_eq!(formatted, "constraint symmetric R on (a, b) param (ctx)");
}

#[test]
fn parses_transitive_with_param_only() {
    let c = parse_constraint_v1("transitive Accessible param (ctx)").expect("parse");
    assert_eq!(
        c,
        ConstraintV1::Transitive {
            relation: "Accessible".to_string(),
            carriers: None,
            params: Some(vec!["ctx".to_string()]),
        }
    );
}
