use axiograph_pathdb::{CertificateV2, PathEquivProofV2, PathExprV2};

fn main() {
    // Two equivalent path expressions that differ only by:
    // - parenthesization (associativity),
    // - insertion/removal of identities.
    //
    // Both should normalize to `p ; q`.
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
    let left = PathExprV2::Trans {
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
    let right = PathExprV2::Trans {
        left: Box::new(p),
        right: Box::new(PathExprV2::Trans {
            left: Box::new(PathExprV2::Reflexive { entity: 2 }),
            right: Box::new(q),
        }),
    };

    let (left_norm, left_derivation) = left.normalize_with_derivation();
    let (right_norm, right_derivation) = right.normalize_with_derivation();
    assert_eq!(
        left_norm, right_norm,
        "expected left and right to normalize to the same form"
    );

    let proof = PathEquivProofV2 {
        left,
        right,
        normalized: left_norm,
        left_derivation,
        right_derivation,
    };
    let cert = CertificateV2::path_equiv(proof);

    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
