use axiograph_pathdb::{CertificateV2, PathExprV2, ProofProducingOptimizer, WithProof};

fn main() {
    let optimizer = ProofProducingOptimizer::default();

    // Base equivalence: two different spellings of `p ; q`.
    let p = PathExprV2::Step {
        from: 1,
        rel_type: 10,
        to: 2,
    };
    let q = PathExprV2::Step {
        from: 2,
        rel_type: 20,
        to: 3,
    };

    // ((id ; p) ; q) ; id
    let left0 = PathExprV2::Trans {
        left: Box::new(PathExprV2::Trans {
            left: Box::new(PathExprV2::Trans {
                left: Box::new(PathExprV2::Reflexive { entity: 1 }),
                right: Box::new(p.clone()),
            }),
            right: Box::new(q.clone()),
        }),
        right: Box::new(PathExprV2::Reflexive { entity: 3 }),
    };

    // p ; (id ; q)
    let right0 = PathExprV2::Trans {
        left: Box::new(p),
        right: Box::new(PathExprV2::Trans {
            left: Box::new(PathExprV2::Reflexive { entity: 2 }),
            right: Box::new(q),
        }),
    };

    let base = optimizer
        .path_equiv_v2::<WithProof>(left0, right0)
        .expect("base path equivalence should hold");

    // Congruence under post-composition: `(p ; q) · r`.
    let r = PathExprV2::Step {
        from: 3,
        rel_type: 30,
        to: 4,
    };

    let derived = optimizer
        .path_equiv_congr_right_v2::<WithProof>(&base.proof, r)
        .expect("congruence should preserve equivalence");

    let cert = CertificateV2::path_equiv(derived.proof);
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
