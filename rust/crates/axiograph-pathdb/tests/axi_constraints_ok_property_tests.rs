use std::collections::{HashMap, HashSet, VecDeque};

use axiograph_dsl::schema_v1::{
    ConstraintV1, FieldDeclV1, InstanceAssignmentV1, RelationDeclV1, SchemaV1Instance,
    SchemaV1Module, SchemaV1Schema, SchemaV1Theory, SetItemV1, SetLiteralV1,
};
use axiograph_pathdb::axi_module_constraints::check_axi_constraints_ok_v1;
use proptest::prelude::*;

fn build_single_relation_module(
    relation_fields: &[String],
    tuples: &[Vec<String>],
    constraints: Vec<ConstraintV1>,
) -> SchemaV1Module {
    let schema_name = "S".to_string();
    let relation_name = "R".to_string();

    // Keep the schema minimal: one generic object type.
    let object_ty = "Atom".to_string();
    let schema = SchemaV1Schema {
        name: schema_name.clone(),
        objects: vec![object_ty.clone()],
        subtypes: Vec::new(),
        relations: vec![RelationDeclV1 {
            name: relation_name.clone(),
            fields: relation_fields
                .iter()
                .map(|f| FieldDeclV1 {
                    field: f.clone(),
                    ty: object_ty.clone(),
                })
                .collect(),
        }],
    };

    let theory = SchemaV1Theory {
        name: "Rules".to_string(),
        schema: schema_name.clone(),
        constraints,
        equations: Vec::new(),
        rewrite_rules: Vec::new(),
    };

    // (Optional) declare the used `Atom` values so the module is readable.
    let mut atoms: Vec<String> = tuples.iter().flat_map(|t| t.iter().cloned()).collect();
    atoms.sort();
    atoms.dedup();

    let mut assignments: Vec<InstanceAssignmentV1> = Vec::new();
    if !atoms.is_empty() {
        assignments.push(InstanceAssignmentV1 {
            name: object_ty,
            value: SetLiteralV1 {
                items: atoms
                    .into_iter()
                    .map(|name| SetItemV1::Ident { name })
                    .collect(),
            },
        });
    }

    assignments.push(InstanceAssignmentV1 {
        name: relation_name,
        value: SetLiteralV1 {
            items: tuples
                .iter()
                .map(|vals| {
                    SetItemV1::Tuple {
                        fields: relation_fields
                            .iter()
                            .cloned()
                            .zip(vals.iter().cloned())
                            .collect(),
                    }
                })
                .collect(),
        },
    });

    let inst = SchemaV1Instance {
        name: "Demo".to_string(),
        schema: schema_name,
        assignments,
    };

    SchemaV1Module {
        module_name: "PropTest".to_string(),
        schemas: vec![schema],
        theories: vec![theory],
        instances: vec![inst],
    }
}

fn key_ok(
    ordered_fields: &[String],
    tuples: &[Vec<String>],
    key_fields: &[String],
) -> bool {
    let mut idxs: Vec<usize> = Vec::with_capacity(key_fields.len());
    for f in key_fields {
        let Some(idx) = ordered_fields.iter().position(|x| x == f) else {
            return false;
        };
        idxs.push(idx);
    }

    let mut seen: HashSet<Vec<String>> = HashSet::new();
    for t in tuples {
        let key: Vec<String> = idxs.iter().map(|i| t[*i].clone()).collect();
        if !seen.insert(key) {
            return false;
        }
    }
    true
}

fn functional_ok(
    ordered_fields: &[String],
    tuples: &[Vec<String>],
    src_field: &str,
    dst_field: &str,
) -> bool {
    let Some(src_idx) = ordered_fields.iter().position(|x| x == src_field) else {
        return false;
    };
    let Some(dst_idx) = ordered_fields.iter().position(|x| x == dst_field) else {
        return false;
    };

    let mut map: HashMap<String, String> = HashMap::new();
    for t in tuples {
        let src = t[src_idx].clone();
        let dst = t[dst_idx].clone();
        if let Some(prev) = map.get(&src) {
            if prev != &dst {
                return false;
            }
        } else {
            map.insert(src, dst);
        }
    }
    true
}

fn symmetric_closure(
    relation_fields: &[String],
    tuples_full: &[Vec<String>],
    carrier_left: &str,
    carrier_right: &str,
    params: Option<&[String]>,
    where_field: Option<&str>,
    where_values: &[String],
) -> (Vec<String>, Vec<Vec<String>>) {
    let left_idx = relation_fields
        .iter()
        .position(|x| x == carrier_left)
        .expect("carrier_left exists");
    let right_idx = relation_fields
        .iter()
        .position(|x| x == carrier_right)
        .expect("carrier_right exists");
    let cond_idx = where_field.map(|f| {
        relation_fields
            .iter()
            .position(|x| x == f)
            .expect("where_field exists")
    });

    let (closure_fields, projection_idxs, swap_left_proj, swap_right_proj) = if let Some(p) =
        params
    {
        let allowed: HashSet<&str> = [carrier_left, carrier_right]
            .into_iter()
            .chain(p.iter().map(|s| s.as_str()))
            .collect();
        let mut projection_fields: Vec<String> = Vec::new();
        let mut idxs: Vec<usize> = Vec::new();
        for (i, f) in relation_fields.iter().enumerate() {
            if allowed.contains(f.as_str()) {
                projection_fields.push(f.clone());
                idxs.push(i);
            }
        }
        let swap_left = projection_fields
            .iter()
            .position(|x| x == carrier_left)
            .expect("carrier in projection");
        let swap_right = projection_fields
            .iter()
            .position(|x| x == carrier_right)
            .expect("carrier in projection");
        (projection_fields, idxs, swap_left, swap_right)
    } else {
        (
            relation_fields.to_vec(),
            (0..relation_fields.len()).collect::<Vec<_>>(),
            left_idx,
            right_idx,
        )
    };

    // Deduplicate under the projection.
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    let mut out: Vec<Vec<String>> = Vec::new();

    for full in tuples_full {
        let mut vals: Vec<String> = Vec::with_capacity(projection_idxs.len());
        for idx in projection_idxs.iter() {
            vals.push(full[*idx].clone());
        }
        if seen.insert(vals.clone()) {
            out.push(vals.clone());
        }

        let apply = match cond_idx {
            None => true,
            Some(i) => where_values.iter().any(|v| v == &full[i]),
        };
        if apply {
            let mut swapped = vals;
            swapped.swap(swap_left_proj, swap_right_proj);
            if seen.insert(swapped.clone()) {
                out.push(swapped);
            }
        }
    }

    (closure_fields, out)
}

fn transitive_pairs(adj: &HashMap<String, Vec<String>>) -> HashSet<(String, String)> {
    let mut closure: HashSet<(String, String)> = HashSet::new();
    let sources: Vec<String> = adj.keys().cloned().collect();
    for src in sources {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        if let Some(neigh) = adj.get(&src) {
            for v in neigh {
                queue.push_back(v.clone());
            }
        }
        while let Some(v) = queue.pop_front() {
            if !visited.insert(v.clone()) {
                continue;
            }
            closure.insert((src.clone(), v.clone()));
            if let Some(more) = adj.get(&v) {
                for w in more {
                    queue.push_back(w.clone());
                }
            }
        }
    }
    closure
}

fn transitive_closure(
    relation_fields: &[String],
    tuples_full: &[Vec<String>],
    carrier0: &str,
    carrier1: &str,
    params: Option<&[String]>,
) -> (Vec<String>, Vec<Vec<String>>) {
    let carrier0_idx = relation_fields
        .iter()
        .position(|x| x == carrier0)
        .expect("carrier0 exists");
    let carrier1_idx = relation_fields
        .iter()
        .position(|x| x == carrier1)
        .expect("carrier1 exists");

    if let Some(params) = params {
        let allowed: HashSet<&str> = [carrier0, carrier1]
            .into_iter()
            .chain(params.iter().map(|s| s.as_str()))
            .collect();

        let projection_fields: Vec<String> = relation_fields
            .iter()
            .filter(|f| allowed.contains(f.as_str()))
            .cloned()
            .collect();

        let mut param_idxs: Vec<usize> = Vec::with_capacity(params.len());
        for p in params {
            let idx = relation_fields
                .iter()
                .position(|x| x == p)
                .expect("param exists");
            param_idxs.push(idx);
        }

        let mut param_pos: HashMap<&str, usize> = HashMap::new();
        for (i, p) in params.iter().enumerate() {
            param_pos.insert(p.as_str(), i);
        }

        // Build adjacency per param fiber.
        let mut adj_by_param: HashMap<Vec<String>, HashMap<String, Vec<String>>> = HashMap::new();
        for full in tuples_full {
            let mut pkey: Vec<String> = Vec::with_capacity(param_idxs.len());
            for idx in param_idxs.iter() {
                pkey.push(full[*idx].clone());
            }
            let src = full[carrier0_idx].clone();
            let dst = full[carrier1_idx].clone();
            adj_by_param
                .entry(pkey)
                .or_default()
                .entry(src)
                .or_default()
                .push(dst);
        }

        let mut seen: HashSet<Vec<String>> = HashSet::new();
        let mut out: Vec<Vec<String>> = Vec::new();
        for (pkey, adj) in adj_by_param.into_iter() {
            for (a, b) in transitive_pairs(&adj).into_iter() {
                let mut tup: Vec<String> = Vec::with_capacity(projection_fields.len());
                for f in projection_fields.iter() {
                    if f == carrier0 {
                        tup.push(a.clone());
                    } else if f == carrier1 {
                        tup.push(b.clone());
                    } else if let Some(i) = param_pos.get(f.as_str()) {
                        tup.push(pkey[*i].clone());
                    } else {
                        unreachable!("projection field must be a carrier or param");
                    }
                }
                if seen.insert(tup.clone()) {
                    out.push(tup);
                }
            }
        }

        (projection_fields, out)
    } else {
        // Global closure on carrier pair only.
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for full in tuples_full {
            let src = full[carrier0_idx].clone();
            let dst = full[carrier1_idx].clone();
            adj.entry(src).or_default().push(dst);
        }
        let pairs = transitive_pairs(&adj);
        let mut out: Vec<Vec<String>> = pairs.into_iter().map(|(a, b)| vec![a, b]).collect();
        // Deduplicate + stable-ish order.
        out.sort();
        out.dedup();
        (vec![carrier0.to_string(), carrier1.to_string()], out)
    }
}

fn entity_name(prefix: &str, n: u8) -> String {
    format!("{prefix}{n}")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn symmetric_closure_compat_matches_naive_basic(
        tuples in proptest::collection::vec((0u8..4, 0u8..4), 0..=8),
        key_fields in prop_oneof![
            Just(vec!["a".to_string()]),
            Just(vec!["b".to_string()]),
            Just(vec!["a".to_string(), "b".to_string()]),
        ]
    ) {
        let relation_fields = vec!["a".to_string(), "b".to_string()];
        let tuples_full: Vec<Vec<String>> = tuples
            .into_iter()
            .map(|(a, b)| vec![entity_name("E", a), entity_name("E", b)])
            .collect();

        let constraints = vec![
            ConstraintV1::Key { relation: "R".to_string(), fields: key_fields.clone() },
            ConstraintV1::Functional { relation: "R".to_string(), src_field: "a".to_string(), dst_field: "b".to_string() },
            ConstraintV1::Symmetric { relation: "R".to_string(), carriers: None, params: None },
        ];

        let module = build_single_relation_module(&relation_fields, &tuples_full, constraints);

        let original_ok =
            key_ok(&relation_fields, &tuples_full, &key_fields) &&
            functional_ok(&relation_fields, &tuples_full, "a", "b");

        let (closure_fields, closure_tuples) = symmetric_closure(
            &relation_fields,
            &tuples_full,
            "a",
            "b",
            None,
            None,
            &[],
        );
        let closure_ok =
            key_ok(&closure_fields, &closure_tuples, &key_fields) &&
            functional_ok(&closure_fields, &closure_tuples, "a", "b");

        let expected_ok = original_ok && closure_ok;
        let got_ok = check_axi_constraints_ok_v1(&module).is_ok();
        prop_assert_eq!(got_ok, expected_ok);
    }

    #[test]
    fn symmetric_where_in_matches_naive(
        tuples in proptest::collection::vec((0u8..4, 0u8..4, 0u8..3), 0..=10),
        key_fields in prop_oneof![
            Just(vec!["a".to_string()]),
            Just(vec!["b".to_string()]),
            Just(vec!["a".to_string(), "b".to_string()]),
        ]
    ) {
        // Relation has an extra discriminant `kind` field; only kind=K1 is symmetric.
        let relation_fields = vec!["a".to_string(), "b".to_string(), "kind".to_string()];
        let tuples_full: Vec<Vec<String>> = tuples
            .into_iter()
            .map(|(a, b, k)| vec![entity_name("E", a), entity_name("E", b), entity_name("K", k)])
            .collect();
        let where_values = vec![entity_name("K", 1)];

        let constraints = vec![
            ConstraintV1::Key { relation: "R".to_string(), fields: key_fields.clone() },
            ConstraintV1::Functional { relation: "R".to_string(), src_field: "a".to_string(), dst_field: "b".to_string() },
            ConstraintV1::SymmetricWhereIn {
                relation: "R".to_string(),
                field: "kind".to_string(),
                values: where_values.clone(),
                carriers: None,
                params: None,
            },
        ];

        let module = build_single_relation_module(&relation_fields, &tuples_full, constraints);

        let original_ok =
            key_ok(&relation_fields, &tuples_full, &key_fields) &&
            functional_ok(&relation_fields, &tuples_full, "a", "b");

        let (closure_fields, closure_tuples) = symmetric_closure(
            &relation_fields,
            &tuples_full,
            "a",
            "b",
            None,
            Some("kind"),
            &where_values,
        );
        let closure_ok =
            key_ok(&closure_fields, &closure_tuples, &key_fields) &&
            functional_ok(&closure_fields, &closure_tuples, "a", "b");

        let expected_ok = original_ok && closure_ok;
        let got_ok = check_axi_constraints_ok_v1(&module).is_ok();
        prop_assert_eq!(got_ok, expected_ok);
    }

    #[test]
    fn symmetric_param_projection_matches_naive(
        tuples in proptest::collection::vec((0u8..4, 0u8..4, 0u8..3, 0u8..3, 0u8..4), 0..=12),
        key_idx_set in proptest::collection::hash_set(0usize..4, 1..=4),
    ) {
        // Relation has extra witness field that is intentionally out-of-scope for
        // `axi_constraints_ok_v1` once we add `param (ctx, time)`.
        let relation_fields = vec![
            "a".to_string(),
            "b".to_string(),
            "ctx".to_string(),
            "time".to_string(),
            "witness".to_string(),
        ];
        let tuples_full: Vec<Vec<String>> = tuples
            .into_iter()
            .map(|(a, b, ctx, time, w)| {
                vec![
                    entity_name("E", a),
                    entity_name("E", b),
                    entity_name("C", ctx),
                    entity_name("T", time),
                    entity_name("W", w),
                ]
            })
            .collect();

        // Key fields are chosen from the allowed closure fields: (a,b,ctx,time) (no witness).
        let mut key_idxs: Vec<usize> = key_idx_set.into_iter().collect();
        key_idxs.sort();
        let allowed_fields = vec!["a", "b", "ctx", "time"];
        let key_fields: Vec<String> = key_idxs.into_iter().map(|i| allowed_fields[i].to_string()).collect();

        let constraints = vec![
            ConstraintV1::Key { relation: "R".to_string(), fields: key_fields.clone() },
            ConstraintV1::Functional { relation: "R".to_string(), src_field: "a".to_string(), dst_field: "b".to_string() },
            ConstraintV1::Symmetric { relation: "R".to_string(), carriers: None, params: Some(vec!["ctx".to_string(), "time".to_string()]) },
        ];
        let module = build_single_relation_module(&relation_fields, &tuples_full, constraints);

        let original_ok =
            key_ok(&relation_fields, &tuples_full, &key_fields) &&
            functional_ok(&relation_fields, &tuples_full, "a", "b");

        let (closure_fields, closure_tuples) = symmetric_closure(
            &relation_fields,
            &tuples_full,
            "a",
            "b",
            Some(&["ctx".to_string(), "time".to_string()]),
            None,
            &[],
        );

        let closure_ok =
            key_ok(&closure_fields, &closure_tuples, &key_fields) &&
            functional_ok(&closure_fields, &closure_tuples, "a", "b");

        let expected_ok = original_ok && closure_ok;
        let got_ok = check_axi_constraints_ok_v1(&module).is_ok();
        prop_assert_eq!(got_ok, expected_ok);
    }

    #[test]
    fn transitive_param_matches_naive(
        tuples in proptest::collection::vec((0u8..5, 0u8..5, 0u8..3, 0u8..3, 0u8..4), 0..=14),
        key_kind in prop_oneof![
            Just("from_ctx_time"),
            Just("from_to_ctx_time"),
        ]
    ) {
        let relation_fields = vec![
            "from".to_string(),
            "to".to_string(),
            "ctx".to_string(),
            "time".to_string(),
            "witness".to_string(),
        ];
        let tuples_full: Vec<Vec<String>> = tuples
            .into_iter()
            .map(|(a, b, ctx, time, w)| {
                vec![
                    entity_name("E", a),
                    entity_name("E", b),
                    entity_name("C", ctx),
                    entity_name("T", time),
                    entity_name("W", w),
                ]
            })
            .collect();

        let key_fields: Vec<String> = match key_kind {
            "from_ctx_time" => vec!["from".to_string(), "ctx".to_string(), "time".to_string()],
            _ => vec!["from".to_string(), "to".to_string(), "ctx".to_string(), "time".to_string()],
        };

        let constraints = vec![
            ConstraintV1::Key { relation: "R".to_string(), fields: key_fields.clone() },
            ConstraintV1::Functional { relation: "R".to_string(), src_field: "from".to_string(), dst_field: "to".to_string() },
            ConstraintV1::Transitive { relation: "R".to_string(), carriers: None, params: Some(vec!["ctx".to_string(), "time".to_string()]) },
        ];
        let module = build_single_relation_module(&relation_fields, &tuples_full, constraints);

        let original_ok =
            key_ok(&relation_fields, &tuples_full, &key_fields) &&
            functional_ok(&relation_fields, &tuples_full, "from", "to");

        let (closure_fields, closure_tuples) = transitive_closure(
            &relation_fields,
            &tuples_full,
            "from",
            "to",
            Some(&["ctx".to_string(), "time".to_string()]),
        );
        let closure_ok =
            key_ok(&closure_fields, &closure_tuples, &key_fields) &&
            functional_ok(&closure_fields, &closure_tuples, "from", "to");

        let expected_ok = original_ok && closure_ok;
        let got_ok = check_axi_constraints_ok_v1(&module).is_ok();
        prop_assert_eq!(got_ok, expected_ok);
    }

    #[test]
    fn transitive_closure_compat_matches_naive_without_params(
        tuples in proptest::collection::vec((0u8..6, 0u8..6), 0..=16),
        key_fields in prop_oneof![
            Just(vec!["from".to_string()]),
            Just(vec!["from".to_string(), "to".to_string()]),
        ]
    ) {
        let relation_fields = vec!["from".to_string(), "to".to_string()];
        let tuples_full: Vec<Vec<String>> = tuples
            .into_iter()
            .map(|(a, b)| vec![entity_name("E", a), entity_name("E", b)])
            .collect();

        let constraints = vec![
            ConstraintV1::Key { relation: "R".to_string(), fields: key_fields.clone() },
            ConstraintV1::Functional { relation: "R".to_string(), src_field: "from".to_string(), dst_field: "to".to_string() },
            ConstraintV1::Transitive { relation: "R".to_string(), carriers: None, params: None },
        ];
        let module = build_single_relation_module(&relation_fields, &tuples_full, constraints);

        let original_ok =
            key_ok(&relation_fields, &tuples_full, &key_fields) &&
            functional_ok(&relation_fields, &tuples_full, "from", "to");

        let (closure_fields, closure_tuples) = transitive_closure(
            &relation_fields,
            &tuples_full,
            "from",
            "to",
            None,
        );
        let closure_ok =
            key_ok(&closure_fields, &closure_tuples, &key_fields) &&
            functional_ok(&closure_fields, &closure_tuples, "from", "to");

        let expected_ok = original_ok && closure_ok;
        let got_ok = check_axi_constraints_ok_v1(&module).is_ok();
        prop_assert_eq!(got_ok, expected_ok);
    }
}
