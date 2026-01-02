use axiograph_dsl::schema_v1::{
    format_constraint_v1, parse_constraint_v1, CarrierFieldsV1, ConstraintV1,
};
use proptest::prelude::*;

fn ident() -> impl Strategy<Value = String> {
    // Keep identifiers small and readable (and compatible with the `.axi` parser).
    proptest::string::string_regex("[A-Za-z][A-Za-z0-9_]{0,10}").unwrap()
}

fn ident_list(min: usize, max: usize) -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(ident(), min..=max)
}

fn carriers_opt() -> impl Strategy<Value = Option<CarrierFieldsV1>> {
    proptest::option::of((ident(), ident()).prop_filter("carriers must be distinct", |(a, b)| a != b).prop_map(|(a, b)| {
        CarrierFieldsV1 {
            left_field: a,
            right_field: b,
        }
    }))
}

fn params_opt() -> impl Strategy<Value = Option<Vec<String>>> {
    proptest::option::of(ident_list(1, 4))
}

fn closure_constraint() -> impl Strategy<Value = ConstraintV1> {
    prop_oneof![
        // symmetric Rel [on (...)][param (...)]
        (ident(), carriers_opt(), params_opt()).prop_map(|(relation, carriers, params)| {
            ConstraintV1::Symmetric {
                relation,
                carriers,
                params,
            }
        }),
        // transitive Rel [on (...)][param (...)]
        (ident(), carriers_opt(), params_opt()).prop_map(|(relation, carriers, params)| {
            ConstraintV1::Transitive {
                relation,
                carriers,
                params,
            }
        }),
        // symmetric Rel where Rel.field in {...} [on (...)][param (...)]
        (
            ident(),
            ident(),
            ident_list(1, 5),
            carriers_opt(),
            params_opt(),
        )
            .prop_map(|(relation, field, values, carriers, params)| ConstraintV1::SymmetricWhereIn {
                relation,
                field,
                values,
                carriers,
                params,
            }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn closure_constraints_roundtrip_parse_and_format(c in closure_constraint()) {
        let formatted = format_constraint_v1(&c).expect("format");
        let rest = formatted
            .strip_prefix("constraint ")
            .expect("formatter should prefix with `constraint `");
        let parsed = parse_constraint_v1(rest).expect("parse");
        prop_assert_eq!(parsed, c);
    }
}

