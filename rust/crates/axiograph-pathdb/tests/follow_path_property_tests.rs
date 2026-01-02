use axiograph_pathdb::PathDB;
use proptest::prelude::*;
use std::collections::HashSet;

const MAX_ENTITIES: usize = 14;
const MAX_REL_TYPES: usize = 5;
const MAX_EDGES: usize = 80;
const MAX_PATH_LEN: usize = 7;

#[derive(Debug, Clone)]
struct GraphCase {
    entity_count: usize,
    rel_names: Vec<String>,
    edges: Vec<(usize, usize, usize, u32)>, // (rel_idx, src_idx, dst_idx, confidence_fp)
    start_idx: usize,
    path: Vec<usize>,
    min_conf_a_fp: u32,
    min_conf_b_fp: u32,
}

fn graph_case_strategy() -> impl Strategy<Value = GraphCase> {
    (1usize..=MAX_ENTITIES, 1usize..=MAX_REL_TYPES).prop_flat_map(|(entity_count, rel_count)| {
        let rel_names = (0..rel_count).map(|i| format!("r{i}")).collect::<Vec<_>>();
        (
            Just(entity_count),
            Just(rel_names),
            prop::collection::vec(
                (
                    0usize..rel_count,
                    0usize..entity_count,
                    0usize..entity_count,
                    0u32..=axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR,
                ),
                0..=MAX_EDGES,
            ),
            0usize..entity_count,
            prop::collection::vec(0usize..rel_count, 0..=MAX_PATH_LEN),
            0u32..=axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR,
            0u32..=axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR,
        )
    })
    .prop_map(
        |(
            entity_count,
            rel_names,
            edges,
            start_idx,
            path,
            min_conf_a_fp,
            min_conf_b_fp,
        )| GraphCase {
            entity_count,
            rel_names,
            edges,
            start_idx,
            path,
            min_conf_a_fp,
            min_conf_b_fp,
        },
    )
}

fn build_db(case: &GraphCase) -> (PathDB, Vec<u32>) {
    let mut db = PathDB::new();
    let mut ids: Vec<u32> = Vec::with_capacity(case.entity_count);
    for i in 0..case.entity_count {
        let id = db.add_entity("Node", vec![("name", &format!("n{i}"))]);
        ids.push(id);
    }

    let denom = axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR as f32;
    for (rel_idx, src_idx, dst_idx, conf_fp) in &case.edges {
        let rel = &case.rel_names[*rel_idx];
        let src = ids[*src_idx];
        let dst = ids[*dst_idx];
        let conf = (*conf_fp as f32) / denom;
        db.add_relation(rel, src, dst, conf, Vec::new());
    }

    (db, ids)
}

fn follow_naive(
    start: u32,
    path: &[usize],
    case: &GraphCase,
    ids: &[u32],
    min_conf_fp: Option<u32>,
) -> Vec<u32> {
    let denom = axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR as f32;
    let min_conf = min_conf_fp.map(|n| (n as f32) / denom).unwrap_or(0.0);

    let mut current: HashSet<u32> = HashSet::new();
    current.insert(start);

    for rel_idx in path {
        let mut next: HashSet<u32> = HashSet::new();
        for (edge_rel, src_idx, dst_idx, conf_fp) in &case.edges {
            if edge_rel != rel_idx {
                continue;
            }
            let conf = (*conf_fp as f32) / denom;
            if conf < min_conf {
                continue;
            }
            let src = ids[*src_idx];
            let dst = ids[*dst_idx];
            if current.contains(&src) {
                next.insert(dst);
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }

    let mut out: Vec<u32> = current.into_iter().collect();
    out.sort();
    out
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 192,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn follow_path_matches_naive(case in graph_case_strategy()) {
        let (db, ids) = build_db(&case);
        let start = ids[case.start_idx];

        let path_strs: Vec<&str> = case.path.iter().map(|i| case.rel_names[*i].as_str()).collect();
        let actual: Vec<u32> = db.follow_path(start, &path_strs).iter().collect();
        let expected = follow_naive(start, &case.path, &case, &ids, None);

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn follow_path_with_min_confidence_matches_naive(case in graph_case_strategy()) {
        let (db, ids) = build_db(&case);
        let start = ids[case.start_idx];
        let min_conf = (case.min_conf_a_fp as f32) / (axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR as f32);

        let path_strs: Vec<&str> = case.path.iter().map(|i| case.rel_names[*i].as_str()).collect();
        let actual: Vec<u32> = db
            .follow_path_with_min_confidence(start, &path_strs, min_conf)
            .iter()
            .collect();
        let expected = follow_naive(start, &case.path, &case, &ids, Some(case.min_conf_a_fp));

        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn follow_path_with_min_confidence_is_monotone(case in graph_case_strategy()) {
        let (db, ids) = build_db(&case);
        let start = ids[case.start_idx];
        let denom = axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR as f32;

        let a = case.min_conf_a_fp.min(case.min_conf_b_fp);
        let b = case.min_conf_a_fp.max(case.min_conf_b_fp);

        let path_strs: Vec<&str> = case.path.iter().map(|i| case.rel_names[*i].as_str()).collect();
        let ra = db.follow_path_with_min_confidence(start, &path_strs, (a as f32) / denom);
        let rb = db.follow_path_with_min_confidence(start, &path_strs, (b as f32) / denom);

        // Higher threshold => fewer (or equal) reachable targets.
        prop_assert!(rb.is_subset(&ra));
    }
}
