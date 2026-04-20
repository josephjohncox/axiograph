use axiograph_pathdb::{AcceptedSnapshotId, PathDB};

fn main() {
    let db = PathDB::new();
    let _ = db.snapshot_index_sidecar(Some(AcceptedSnapshotId::new("accepted:42")));
}
