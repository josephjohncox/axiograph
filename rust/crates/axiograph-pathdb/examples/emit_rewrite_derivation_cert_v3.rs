use std::{env, fs};

use axiograph_dsl::digest::axi_digest_v1;
use axiograph_dsl::schema_v1::PathExprV3;
use axiograph_pathdb::certificate::PathRewriteStepV3;
use axiograph_pathdb::{AxiAnchorV1, CertificateV2, RewriteDerivationProofV3};

fn main() {
    let mut args = env::args().skip(1);
    let Some(anchor_path) = args.next() else {
        eprintln!("usage: emit_rewrite_derivation_cert_v3 <anchor.axi>");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: emit_rewrite_derivation_cert_v3 <anchor.axi>");
        std::process::exit(2);
    }

    let text = fs::read_to_string(&anchor_path).expect("read anchor .axi");
    let digest = axi_digest_v1(&text);

    // A tiny replayable rewrite derivation using an `.axi`-declared rule:
    //   trans(refl(a), step(a,r,b))  ↦  step(a,r,b)
    //
    // The rule is defined in `examples/anchors/rewrite_rules_anchor_v1.axi` as:
    //   theory T: rewrite id_left_axi
    let input = PathExprV3::Trans {
        left: Box::new(PathExprV3::Reflexive {
            entity: "a".to_string(),
        }),
        right: Box::new(PathExprV3::Step {
            from: "a".to_string(),
            rel: "r".to_string(),
            to: "b".to_string(),
        }),
    };
    let output = PathExprV3::Step {
        from: "a".to_string(),
        rel: "r".to_string(),
        to: "b".to_string(),
    };

    let proof = RewriteDerivationProofV3 {
        input,
        output,
        derivation: vec![PathRewriteStepV3 {
            pos: vec![],
            rule_ref: format!("axi:{digest}:T:id_left_axi"),
        }],
    };

    let cert = CertificateV2::rewrite_derivation_v3(proof).with_anchor(AxiAnchorV1 {
        axi_digest_v1: digest,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
