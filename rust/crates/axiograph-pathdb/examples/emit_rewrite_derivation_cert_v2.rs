use axiograph_pathdb::certificate::{PathRewriteRuleV2, PathRewriteStepV2};
use axiograph_pathdb::{CertificateV2, PathExprV2, RewriteDerivationProofV2};

fn main() {
    // A tiny replayable rewrite derivation:
    //   (id ; p)  ↦  p
    let p = PathExprV2::Step {
        from: 1,
        rel_type: 10,
        to: 2,
    };
    let input = PathExprV2::Trans {
        left: Box::new(PathExprV2::Reflexive { entity: 1 }),
        right: Box::new(p.clone()),
    };
    let output = p;

    let proof = RewriteDerivationProofV2 {
        input,
        output,
        derivation: vec![PathRewriteStepV2 {
            pos: vec![],
            rule: PathRewriteRuleV2::IdLeft,
        }],
    };

    let cert = CertificateV2::rewrite_derivation(proof);
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
