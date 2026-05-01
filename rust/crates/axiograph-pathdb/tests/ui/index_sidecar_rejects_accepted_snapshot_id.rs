use axiograph_pathdb::{AcceptedSnapshotId, PathDB};

fn main() {
    let db = PathDB::new();
    let accepted_snapshot: Option<AcceptedSnapshotId> =
        Some(AcceptedSnapshotId::new("accepted:42"));
    let _ = db.snapshot_index_sidecar(accepted_snapshot);
}
