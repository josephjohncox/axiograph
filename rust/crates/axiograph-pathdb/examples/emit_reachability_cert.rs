use axiograph_pathdb::{Certificate, ReachabilityProof, VerifiedProb};

fn main() {
    let proof = ReachabilityProof::Step {
        from: 1,
        rel_type: 10,
        to: 2,
        rel_confidence: VerifiedProb::new(0.9),
        rest: Box::new(ReachabilityProof::Step {
            from: 2,
            rel_type: 11,
            to: 3,
            rel_confidence: VerifiedProb::new(0.8),
            rest: Box::new(ReachabilityProof::Reflexive { entity: 3 }),
        }),
    };

    let cert = Certificate::reachability(proof);
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
