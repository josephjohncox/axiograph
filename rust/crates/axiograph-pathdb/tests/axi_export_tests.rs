//! PathDB ↔ `.axi` export/import tests.

use axiograph_pathdb::axi_export::{export_pathdb_to_axi_v1, import_pathdb_from_axi_v1};
use axiograph_pathdb::PathDB;

#[test]
fn pathdb_export_v1_roundtrip_is_deterministic_and_reversible() {
    let mut db = PathDB::new();

    let alice = db.add_entity(
        "Person",
        vec![
            ("name", "Alice"),
            ("bio", "likes cats & coffee"),
            ("unicode", "Δ-schema"),
        ],
    );
    let bob = db.add_entity("Person", vec![("name", "Bob"), ("note", "works on v2")]);
    let acme = db.add_entity("Company", vec![("name", "ACME, Inc.")]);

    db.add_relation(
        "works_at",
        alice,
        acme,
        0.9,
        vec![("role", "Senior Engineer"), ("since", "2024-01-01")],
    );
    db.add_relation("knows", alice, bob, 0.625, vec![("met_at", "NYC (2023)")]);

    db.add_equivalence(alice, bob, "PossibleSamePerson?");
    db.build_indexes();

    let axi_1 = export_pathdb_to_axi_v1(&db).expect("export to axi");
    let imported = import_pathdb_from_axi_v1(&axi_1).expect("import from axi");
    let axi_2 = export_pathdb_to_axi_v1(&imported).expect("re-export to axi");

    assert_eq!(
        axi_1, axi_2,
        "expected `.axi` export to be stable across a round-trip"
    );
}
