use axiograph_pathdb::{AcceptedAxiAnchor, AxiDigest, PathdbSnapshotId};

fn main() {
    let _ = AcceptedAxiAnchor::new(
        PathdbSnapshotId::new("pathdb:42"),
        AxiDigest::new("fnv1a64:0123456789abcdef"),
    );
}
