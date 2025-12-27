use axiograph_pathdb::{CertificateV2, NormalizePathProofV2, PathExprV2};

fn main() {
    // A moderately interesting input:
    // - identity steps (`reflexive`) nested under composition,
    // - left-associated `trans` trees,
    // - and an inverse of a composite path composed with the path itself.
    //
    // Normalization should reduce this to a single `reflexive` node.
    let left_inner = PathExprV2::Trans {
        left: Box::new(PathExprV2::Trans {
            left: Box::new(PathExprV2::Reflexive { entity: 1 }),
            right: Box::new(PathExprV2::Step {
                from: 1,
                rel_type: 10,
                to: 2,
            }),
        }),
        right: Box::new(PathExprV2::Trans {
            left: Box::new(PathExprV2::Reflexive { entity: 2 }),
            right: Box::new(PathExprV2::Step {
                from: 2,
                rel_type: 20,
                to: 3,
            }),
        }),
    };

    let left = PathExprV2::Inv {
        path: Box::new(left_inner),
    };

    let right = PathExprV2::Trans {
        left: Box::new(PathExprV2::Trans {
            left: Box::new(PathExprV2::Step {
                from: 1,
                rel_type: 10,
                to: 2,
            }),
            right: Box::new(PathExprV2::Step {
                from: 2,
                rel_type: 20,
                to: 3,
            }),
        }),
        right: Box::new(PathExprV2::Reflexive { entity: 3 }),
    };

    let input = PathExprV2::Trans {
        left: Box::new(left),
        right: Box::new(right),
    };

    let (normalized, derivation) = input.normalize_with_derivation();
    let proof = NormalizePathProofV2 {
        input,
        normalized,
        derivation,
    };
    let cert = CertificateV2::normalize_path(proof);

    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
