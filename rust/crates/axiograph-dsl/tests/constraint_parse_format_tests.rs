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
    let c = parse_constraint_v1("transitive Accessible on (from, to) param (ctx)").expect("parse");
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
fn rejects_noncanonical_param_before_on_order() {
    let err = parse_constraint_v1("symmetric R param (ctx) on (a, b)")
        .expect_err("noncanonical closure-clause order must reject");
    assert!(
        err.contains("canonical `on (...) param (...)` order"),
        "err={err}"
    );
}

#[test]
fn rejects_param_clause_on_key_constraints() {
    let err = parse_constraint_v1("key R(a) param (ctx)").expect_err("should error");
    assert!(
        err.contains("only supported for symmetric/transitive/at_most"),
        "err={err}"
    );
}

#[test]
fn rejects_duplicate_param_clause() {
    let err =
        parse_constraint_v1("symmetric R param (ctx) param (time)").expect_err("should error");
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
    assert!(
        err.contains("param fields must not contain empty"),
        "err={err}"
    );
}

#[test]
fn rejects_bare_symmetric_guard_field_shorthand() {
    let err = parse_constraint_v1("symmetric Relationship where relType in {Friend, Sibling}")
        .expect_err("bare guard fields are not canonical");
    assert!(
        err.contains("canonical qualified `Relation.field`"),
        "err={err}"
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
fn preserves_canonical_suffix_clause_order() {
    let c = parse_constraint_v1("symmetric R on (a, b) param (ctx)").expect("parse");
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

#[test]
fn parses_at_most_with_param_clause() {
    let c = parse_constraint_v1("at_most 2 Parent.child -> Parent.parent param (ctx, time)")
        .expect("parse");
    assert_eq!(
        c,
        ConstraintV1::AtMost {
            relation: "Parent".to_string(),
            src_field: "child".to_string(),
            dst_field: "parent".to_string(),
            max: 2,
            params: Some(vec!["ctx".to_string(), "time".to_string()]),
        }
    );

    let formatted = axiograph_dsl::schema_v1::format_constraint_v1(&c).expect("format");
    assert_eq!(
        formatted,
        "constraint at_most 2 Parent.child -> Parent.parent param (ctx, time)"
    );
}
