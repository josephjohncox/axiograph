use axiograph_pathdb::{CertificateV2, FixedPointProbability, ReachabilityProofV2};

fn main() {
    let proof = ReachabilityProofV2::Step {
        from: 1,
        rel_type: 10,
        to: 2,
        rel_confidence_fp: FixedPointProbability::try_new(900_000)
            .expect("valid fixed-point probability"),
        relation_id: None,
        rest: Box::new(ReachabilityProofV2::Step {
            from: 2,
            rel_type: 11,
            to: 3,
            rel_confidence_fp: FixedPointProbability::try_new(800_000)
                .expect("valid fixed-point probability"),
            relation_id: None,
            rest: Box::new(ReachabilityProofV2::Reflexive { entity: 3 }),
        }),
    };

    let cert = CertificateV2::reachability(proof);
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
