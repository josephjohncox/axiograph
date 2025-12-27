use anyhow::Result;
use axiograph_dsl::digest::axi_digest_v1;
use axiograph_pathdb::axi_export::export_pathdb_to_axi_v1;
use axiograph_pathdb::{
    AxiAnchorV1, CertificateV2, FixedPointProbability, PathDB, ReachabilityProofV2,
};

fn main() -> Result<()> {
    // Build a tiny PathDB snapshot and emit:
    // - a `PathDBExportV1` `.axi` snapshot (written to a provided file path), and
    // - an anchored reachability certificate that references snapshot fact IDs.
    //
    // Usage:
    //   cargo run -p axiograph-pathdb --example emit_reachability_cert_v2_anchored -- <out.axi>
    //
    // The certificate is printed to stdout.
    let mut db = PathDB::new();

    let a = db.add_entity("Thing", vec![("name", "a")]);
    let b = db.add_entity("Thing", vec![("name", "b")]);
    let c = db.add_entity("Thing", vec![("name", "c")]);

    let rel1_id = db.add_relation("r1", a, b, 0.9, vec![]);
    let rel2_id = db.add_relation("r2", b, c, 0.8, vec![]);
    db.build_indexes();

    let axi = export_pathdb_to_axi_v1(&db)?;
    let digest = axi_digest_v1(&axi);

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: emit_reachability_cert_v2_anchored <out.axi>");
        std::process::exit(2);
    }
    std::fs::write(&args[1], &axi)?;

    let proof = ReachabilityProofV2::Step {
        from: a,
        rel_type: db.interner.intern("r1").raw(),
        to: b,
        rel_confidence_fp: FixedPointProbability::try_new(900_000)
            .expect("valid fixed-point probability"),
        relation_id: Some(rel1_id),
        rest: Box::new(ReachabilityProofV2::Step {
            from: b,
            rel_type: db.interner.intern("r2").raw(),
            to: c,
            rel_confidence_fp: FixedPointProbability::try_new(800_000)
                .expect("valid fixed-point probability"),
            relation_id: Some(rel2_id),
            rest: Box::new(ReachabilityProofV2::Reflexive { entity: c }),
        }),
    };

    let cert = CertificateV2::reachability(proof).with_anchor(AxiAnchorV1 {
        axi_digest_v1: digest,
    });

    println!("{}", serde_json::to_string_pretty(&cert)?);
    Ok(())
}
