use axiograph_pathdb::{CertificateV2, FixedPointProbability, ResolutionProofV2};

fn main() {
    let first = FixedPointProbability::try_new(800_000).expect("valid fixed-point probability");
    let second = FixedPointProbability::try_new(600_000).expect("valid fixed-point probability");
    let threshold = FixedPointProbability::try_new(200_000).expect("valid fixed-point probability");

    let proof = ResolutionProofV2::decide(first, second, threshold);
    let cert = CertificateV2::resolution(proof);

    println!(
        "{}",
        serde_json::to_string_pretty(&cert).expect("serialize certificate")
    );
}
