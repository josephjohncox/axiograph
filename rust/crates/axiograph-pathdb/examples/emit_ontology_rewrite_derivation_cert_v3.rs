use std::{env, fs};

use axiograph_dsl::digest::axi_digest_v1;
use axiograph_dsl::schema_v1::PathExprV3;
use axiograph_pathdb::certificate::PathRewriteStepV3;
use axiograph_pathdb::{AxiAnchorV1, CertificateV2, RewriteDerivationProofV3};

fn main() {
    let mut args = env::args().skip(1);
    let Some(anchor_path) = args.next() else {
        eprintln!("usage: emit_ontology_rewrite_derivation_cert_v3 <anchor.axi>");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("usage: emit_ontology_rewrite_derivation_cert_v3 <anchor.axi>");
        std::process::exit(2);
    }

    let text = fs::read_to_string(&anchor_path).expect("read anchor .axi");
    let digest = axi_digest_v1(&text);

    // A tiny replayable rewrite derivation using a *domain* `.axi` rule:
    //
    //   Parent(Alice,Bob) ; Parent(Bob,Carol)  ↦  Grandparent(Alice,Carol)
    //
    // The rule is defined in the anchored `.axi` module as:
    //   theory OrgFamilySemantics: rewrite grandparent_def
    let input = PathExprV3::Trans {
        left: Box::new(PathExprV3::Step {
            from: "Alice".to_string(),
            rel: "Parent".to_string(),
            to: "Bob".to_string(),
        }),
        right: Box::new(PathExprV3::Step {
            from: "Bob".to_string(),
            rel: "Parent".to_string(),
            to: "Carol".to_string(),
        }),
    };
    let output = PathExprV3::Step {
        from: "Alice".to_string(),
        rel: "Grandparent".to_string(),
        to: "Carol".to_string(),
    };

    let proof = RewriteDerivationProofV3 {
        input,
        output,
        derivation: vec![PathRewriteStepV3 {
            pos: vec![],
            rule_ref: format!("axi:{digest}:OrgFamilySemantics:grandparent_def"),
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
