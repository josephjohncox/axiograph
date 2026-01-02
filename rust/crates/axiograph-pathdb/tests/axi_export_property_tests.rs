//! Property tests for `PathDBExportV1` (reversible `.axi` snapshot export).
//!
//! This is an engineering interchange format, but it is critical infrastructure:
//! it underpins snapshot portability, anchoring for certificates, and offline auditability.

use axiograph_pathdb::axi_export::{export_pathdb_to_axi_v1, import_pathdb_from_axi_v1};
use axiograph_pathdb::certificate::FIXED_POINT_DENOMINATOR;
use axiograph_pathdb::PathDB;
use proptest::prelude::*;

#[derive(Debug, Clone)]
struct ExportCase {
    entity_types: Vec<String>,
    rel_types: Vec<String>,
    entities: Vec<(usize, String, Vec<(String, String)>)>, // (type_idx, name, attrs)
    edges: Vec<(usize, usize, usize, u32, Vec<(String, String)>)>, // (rel_idx, src, dst, conf_fp, attrs)
    equivalences: Vec<(usize, usize, String)>, // (a, b, label)
}

fn small_string() -> impl Strategy<Value = String> {
    // Keep strings short to avoid huge hex dumps in the exported `.axi`.
    prop::collection::vec(any::<char>(), 0..=12).prop_map(|chars| chars.into_iter().collect())
}

fn kv_pairs(max: usize) -> impl Strategy<Value = Vec<(String, String)>> {
    prop::collection::vec((small_string(), small_string()), 0..=max)
}

fn export_case_strategy() -> impl Strategy<Value = ExportCase> {
    (1usize..=10, 1usize..=4, 1usize..=4).prop_flat_map(|(n_entities, n_types, n_rels)| {
        let entity_types: Vec<String> = (0..n_types).map(|i| format!("Type{i}")).collect();
        let rel_types: Vec<String> = (0..n_rels).map(|i| format!("rel_{i}")).collect();

        let entities = prop::collection::vec(
            (
                0usize..n_types,
                small_string(),
                kv_pairs(3),
            ),
            n_entities..=n_entities,
        );

        let edges = prop::collection::vec(
            (
                0usize..n_rels,
                0usize..n_entities,
                0usize..n_entities,
                0u32..=FIXED_POINT_DENOMINATOR,
                kv_pairs(2),
            ),
            0..=25,
        );

        let equivalences = if n_entities < 2 {
            Just(Vec::new()).boxed()
        } else {
            // Generate distinct (a,b) without rejection.
            prop::collection::vec(
                (
                    0usize..n_entities,
                    0usize..(n_entities - 1),
                    small_string(),
                )
                    .prop_map(|(a, b_off, label)| {
                        let b = if b_off >= a { b_off + 1 } else { b_off };
                        (a, b, label)
                    }),
                0..=10,
            )
            .boxed()
        };

        (Just(entity_types), Just(rel_types), entities, edges, equivalences).prop_map(
            |(entity_types, rel_types, entities, edges, equivalences)| ExportCase {
                entity_types,
                rel_types,
                entities,
                edges,
                equivalences,
            },
        )
    })
}

fn build_db(case: &ExportCase) -> PathDB {
    let mut db = PathDB::new();

    let mut ids: Vec<u32> = Vec::with_capacity(case.entities.len());
    for (type_idx, name, attrs) in &case.entities {
        let mut kv: Vec<(String, String)> = Vec::new();
        kv.push(("name".to_string(), name.clone()));
        kv.extend(attrs.iter().cloned());

        let kv_refs: Vec<(&str, &str)> = kv.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let type_name = &case.entity_types[*type_idx];
        let id = db.add_entity(type_name, kv_refs);
        ids.push(id);
    }

    let denom = FIXED_POINT_DENOMINATOR as f32;
    for (rel_idx, src_idx, dst_idx, conf_fp, attrs) in &case.edges {
        let src = ids[*src_idx];
        let dst = ids[*dst_idx];
        let rel = &case.rel_types[*rel_idx];
        let conf = (*conf_fp as f32) / denom;
        let attr_refs: Vec<(&str, &str)> =
            attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        db.add_relation(rel, src, dst, conf, attr_refs);
    }

    for (a, b, label) in &case.equivalences {
        db.add_equivalence(ids[*a], ids[*b], label);
    }

    db.build_indexes();
    db
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn pathdb_export_v1_roundtrip_is_stable(case in export_case_strategy()) {
        let db = build_db(&case);
        let axi_1 = export_pathdb_to_axi_v1(&db).expect("export");
        let imported = import_pathdb_from_axi_v1(&axi_1).expect("import");
        let axi_2 = export_pathdb_to_axi_v1(&imported).expect("re-export");
        prop_assert_eq!(axi_1, axi_2);
    }
}
