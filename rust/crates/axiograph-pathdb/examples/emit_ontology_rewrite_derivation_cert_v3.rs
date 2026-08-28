use std::env;

use axiograph_dsl::schema_v1::PathExprV3;
use axiograph_kernel::revision_digest_v2;
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

    let text = axiograph_security::read_utf8_file_bounded(
        std::path::Path::new(&anchor_path),
        4 * 1024 * 1024,
        "rewrite certificate .axi anchor",
    )
    .expect("read bounded anchor .axi");
    let digest = revision_digest_v2(&text);

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
            rule_ref: format!("axi-rule-v2|{digest}|OrgFamilySemantics|grandparent_def"),
        }],
    };

    let cert = CertificateV2::rewrite_derivation_v3(proof).with_anchor(AxiAnchorV1 {
        revision_digest_v2: digest.into(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
