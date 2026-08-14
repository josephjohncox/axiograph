use axiograph_pathdb::{CertificateV2, CertificateV3};

#[test]
fn canonical_axi_seed_is_accepted() {
    let seed = include_str!("../corpus/axi_parser/minimal.axi");
    axiograph_dsl::axi_v1::parse_axi_v1(seed).expect("canonical .axi seed must remain accepted");
}

#[test]
fn certificate_seeds_cover_accepted_v2_and_v3_shapes() {
    let v2 = include_bytes!("../corpus/certificate_json/typecheck_v2.json");
    let parsed_v2: CertificateV2 =
        axiograph_security::parse_json_bounded(v2, 8 * 1024 * 1024, "V2 seed")
            .expect("CertificateV2 seed must remain accepted");
    assert_eq!(parsed_v2.version, 2);
    assert!(parsed_v2.anchor.is_some());

    let v3 = include_bytes!("../corpus/certificate_json/query_result_v4_exact.json");
    let _: CertificateV3 =
        axiograph_security::parse_json_bounded(v3, 8 * 1024 * 1024, "V3 seed")
            .expect("CertificateV3 seed must remain accepted");
}

#[test]
fn repl_seed_reaches_the_axql_preserving_route() {
    let seed = include_str!("../corpus/repl_command/axql.txt").trim();
    let tokens = axiograph_cli::repl_command::tokenize_repl_line(seed);
    assert_eq!(tokens[0], "q");
    assert_eq!(tokens[1], "--apply-refinement");
    assert_eq!(tokens[2], "repair/parent");
    assert_eq!(
        tokens[3],
        "select ?x where name(\"Åsa\") -Parent-> ?x limit 3"
    );
}

#[test]
fn axpd_adversarial_seed_reaches_the_sqlite_header_route() {
    let seed = include_bytes!("../corpus/axpd_bytes/sqlite-prefix.bin");
    assert!(seed.starts_with(b"SQLite format 3\0"));
    assert!(seed.len() <= 262_144);
}
