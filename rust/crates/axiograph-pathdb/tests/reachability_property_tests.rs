use axiograph_pathdb::certificate::{FixedPointProbability, ReachabilityProofV2, FIXED_POINT_DENOMINATOR};
use proptest::prelude::*;

const MAX_PATH_LEN: usize = 12;
const MAX_ENTITY_ID: u32 = 200;
const MAX_REL_TYPE_ID: u32 = 50;

fn reachability_proof_v2_strategy(
) -> impl Strategy<Value = (ReachabilityProofV2, Vec<FixedPointProbability>, Vec<u32>)> {
    // Return:
    // - a well-formed reachability proof (a chain of steps ending in Reflexive),
    // - the per-edge confidences (in order),
    // - the visited entity ids (nodes[0]..nodes[n]) for easy checking.
    (0usize..=MAX_PATH_LEN).prop_flat_map(|len| {
        (
            prop::collection::vec(0u32..=MAX_ENTITY_ID, len + 1),
            prop::collection::vec(0u32..=MAX_REL_TYPE_ID, len),
            prop::collection::vec(0u32..=FIXED_POINT_DENOMINATOR, len),
        )
    })
    .prop_map(|(nodes, rel_types, conf_nums)| {
        let confs: Vec<FixedPointProbability> = conf_nums
            .into_iter()
            .map(|n| FixedPointProbability::try_new(n).expect("n is within bounds"))
            .collect();

        let mut proof = ReachabilityProofV2::Reflexive {
            entity: *nodes.last().unwrap_or(&0),
        };

        for i in (0..rel_types.len()).rev() {
            let from = nodes[i];
            let to = nodes[i + 1];
            let rel_type = rel_types[i];
            let rel_confidence_fp = confs[i];
            proof = ReachabilityProofV2::Step {
                from,
                rel_type,
                to,
                rel_confidence_fp,
                relation_id: None,
                rest: Box::new(proof),
            };
        }

        (proof, confs, nodes)
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn reachability_proof_v2_start_end_and_len_are_consistent((p, _confs, nodes) in reachability_proof_v2_strategy()) {
        let expected_len = nodes.len().saturating_sub(1);
        prop_assert_eq!(p.start(), nodes[0]);
        prop_assert_eq!(p.end(), *nodes.last().unwrap());
        prop_assert_eq!(p.path_len(), expected_len);
    }

    #[test]
    fn reachability_proof_v2_confidence_is_fold_of_edge_confidences((p, confs, _nodes) in reachability_proof_v2_strategy()) {
        // `ReachabilityProofV2::path_confidence` is defined as a right-associated
        // multiplication chain: c0 * (c1 * (... * 1)).
        //
        // Fixed-point multiplication rounds down, so associativity does not
        // strictly hold; we must fold in the same order to match semantics.
        let mut expected = FixedPointProbability::try_new(FIXED_POINT_DENOMINATOR).unwrap();
        for c in confs.into_iter().rev() {
            expected = c.mul(expected);
        }
        prop_assert_eq!(p.path_confidence(), expected);
    }
}
