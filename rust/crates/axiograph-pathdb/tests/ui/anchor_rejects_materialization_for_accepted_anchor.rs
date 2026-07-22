use axiograph_pathdb::{AcceptedAxiAnchor, AxiDigest, MaterializationIdV2};

fn main() {
    let _ = AcceptedAxiAnchor::new(
        MaterializationIdV2::from_canonical_fields(&[b"materialization:42"]),
        AxiDigest::from_axi_text("module Demo\n"),
    );
}
