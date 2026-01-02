use axiograph_pathdb::certificate::PathExprV2;
use axiograph_pathdb::optimizer::ProofProducingOptimizer;
use axiograph_pathdb::proof_mode::{NoProof, WithProof};
use proptest::prelude::*;

const MAX_PATH_LEN: usize = 10;
const MAX_ENTITY_ID: u32 = 50;
const MAX_REL_TYPE_ID: u32 = 20;

fn path_expr_v2_strategy() -> impl Strategy<Value = PathExprV2> {
    // Generate a composable chain of atoms (steps or inverse steps), then add
    // some "noise" that should normalize away (identity whiskers, inv-inv).
    (0usize..=MAX_PATH_LEN).prop_flat_map(|len| {
        (
            prop::collection::vec(0u32..=MAX_ENTITY_ID, len + 1),
            prop::collection::vec(0u32..=MAX_REL_TYPE_ID, len),
            prop::collection::vec(any::<bool>(), len), // invert each atom?
            any::<bool>(),                             // right-assoc?
            any::<bool>(),                             // add id-left?
            any::<bool>(),                             // add id-right?
            any::<bool>(),                             // wrap in inv(inv(..))?
            any::<bool>(),                             // wrap in inv(..)?
        )
    })
    .prop_map(
        |(
            nodes,
            rel_types,
            invert_atoms,
            right_assoc,
            add_id_left,
            add_id_right,
            wrap_inv_inv,
            wrap_inv,
        )| {
            let len = rel_types.len();
            let start = *nodes.first().unwrap_or(&0);
            let end = *nodes.last().unwrap_or(&start);

            let mut atoms: Vec<PathExprV2> = Vec::with_capacity(len);
            for i in 0..len {
                let from = nodes[i];
                let to = nodes[i + 1];
                let rel_type = rel_types[i];
                let atom = if invert_atoms[i] {
                    // `inv(step(next -> cur))` is an atom with start=cur, end=next.
                    PathExprV2::Inv {
                        path: Box::new(PathExprV2::Step {
                            from: to,
                            rel_type,
                            to: from,
                        }),
                    }
                } else {
                    PathExprV2::Step { from, rel_type, to }
                };
                atoms.push(atom);
            }

            fn build_right_assoc(atoms: &[PathExprV2], start: u32) -> PathExprV2 {
                match atoms.split_first() {
                    None => PathExprV2::Reflexive { entity: start },
                    Some((first, rest)) => {
                        if rest.is_empty() {
                            first.clone()
                        } else {
                            PathExprV2::Trans {
                                left: Box::new(first.clone()),
                                right: Box::new(build_right_assoc(rest, start)),
                            }
                        }
                    }
                }
            }

            fn build_left_assoc(atoms: &[PathExprV2], start: u32) -> PathExprV2 {
                match atoms.split_first() {
                    None => PathExprV2::Reflexive { entity: start },
                    Some((first, rest)) => {
                        let mut acc = first.clone();
                        for atom in rest {
                            acc = PathExprV2::Trans {
                                left: Box::new(acc),
                                right: Box::new(atom.clone()),
                            };
                        }
                        acc
                    }
                }
            }

            let mut expr = if right_assoc {
                build_right_assoc(&atoms, start)
            } else {
                build_left_assoc(&atoms, start)
            };

            if add_id_left {
                expr = PathExprV2::Trans {
                    left: Box::new(PathExprV2::Reflexive { entity: start }),
                    right: Box::new(expr),
                };
            }
            if add_id_right {
                expr = PathExprV2::Trans {
                    left: Box::new(expr),
                    right: Box::new(PathExprV2::Reflexive { entity: end }),
                };
            }
            if wrap_inv_inv {
                expr = PathExprV2::Inv {
                    path: Box::new(PathExprV2::Inv { path: Box::new(expr) }),
                };
            }
            if wrap_inv {
                expr = PathExprV2::Inv {
                    path: Box::new(expr),
                };
            }
            expr
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn normalize_is_idempotent(p in path_expr_v2_strategy()) {
        let n1 = p.normalize();
        let n2 = n1.normalize();
        prop_assert_eq!(n1, n2);
    }

    #[test]
    fn normalize_preserves_endpoints_for_well_typed_constructions(p in path_expr_v2_strategy()) {
        let n = p.normalize();
        prop_assert_eq!(p.start(), n.start());
        prop_assert_eq!(p.end(), n.end());
    }

    #[test]
    fn p_then_inv_p_normalizes_to_reflexive(p in path_expr_v2_strategy()) {
        let composed = PathExprV2::Trans {
            left: Box::new(p.clone()),
            right: Box::new(PathExprV2::Inv { path: Box::new(p) }),
        };
        let norm = composed.normalize();
        prop_assert_eq!(norm, PathExprV2::Reflexive { entity: composed.start() });
    }

    #[test]
    fn normalize_with_proof_has_consistent_payload(p in path_expr_v2_strategy()) {
        let opt = ProofProducingOptimizer::default();
        let proved = opt.normalize_path_v2::<WithProof>(p.clone());
        prop_assert_eq!(&proved.value, &proved.proof.normalized);
        prop_assert_eq!(&proved.value, &p.normalize());
        if let Some(steps) = proved.proof.derivation.as_ref() {
            let replayed = proved.proof.input.apply_derivation_v2(steps).expect("derivation replay must apply");
            prop_assert_eq!(&replayed, &proved.proof.normalized);
        }
    }

    #[test]
    fn path_equiv_by_normalization_accepts_normal_form(p in path_expr_v2_strategy()) {
        let opt = ProofProducingOptimizer::default();
        let norm = p.normalize();
        let proved = opt.path_equiv_v2::<NoProof>(p, norm.clone()).expect("paths must be equivalent by normalization");
        prop_assert_eq!(proved.value, norm);
    }
}
