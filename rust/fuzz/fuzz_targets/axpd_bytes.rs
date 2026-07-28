#![no_main]

use axiograph_kernel::{
    MaterializationIdV2, ObjectBlobIdV2, RevisionDigestV2, SnapshotIdV2, TreeIdV2,
};
use axiograph_store::{
    AxiStore, AxpdAnchors, AxpdConfiguration, AxpdLimits, AxpdReceipt, RepositoryDescriptor,
    AXPD_FORMAT, AXPD_MATERIALIZATIONS_DIR, AXPD_MATERIALIZER_VERSION, AXPD_SCHEMA_VERSION,
};
use libfuzzer_sys::fuzz_target;
use std::str::FromStr;

const MAX_FUZZED_AXPD_BYTES: usize = 1024 * 1024;
const ZERO_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZED_AXPD_BYTES {
        return;
    }

    let Ok(directory) = tempfile::tempdir() else {
        return;
    };
    let Ok(descriptor) = RepositoryDescriptor::new("fuzz", "fixed-seed") else {
        return;
    };
    let Ok(store) = AxiStore::init(directory.path(), &descriptor) else {
        return;
    };
    let Ok(repository_id) = descriptor.repository_id() else {
        return;
    };
    let Ok(materialization_id) =
        MaterializationIdV2::from_str(&format!("axi:materialization:v2:sha256:{ZERO_HEX}"))
    else {
        return;
    };
    let Ok(snapshot_id) = SnapshotIdV2::from_str(&format!("axi:snapshot:v2:sha256:{ZERO_HEX}"))
    else {
        return;
    };
    let Ok(tree_id) = TreeIdV2::from_str(&format!("axi:tree:v2:sha256:{ZERO_HEX}")) else {
        return;
    };
    let Ok(revision) = RevisionDigestV2::from_str(&format!("axi:revision:v2:sha256:{ZERO_HEX}"))
    else {
        return;
    };
    let Ok(zero_blob) = ObjectBlobIdV2::from_str(&format!("axi:object-blob:v2:sha256:{ZERO_HEX}"))
    else {
        return;
    };
    let exact_image_digest = ObjectBlobIdV2::from_canonical_fields(&[data]);
    let Ok(configuration_digest) = AxpdConfiguration::default().digest() else {
        return;
    };
    let anchors = AxpdAnchors {
        repository_id,
        accepted_snapshot_id: snapshot_id,
        accepted_tree_id: tree_id,
        ordered_module_closure: vec![revision],
        kernel_ir_digest: zero_blob.clone(),
        canonical_fact_log_digest: zero_blob.clone(),
    };
    let receipt = AxpdReceipt {
        format: AXPD_FORMAT.to_string(),
        schema_version: AXPD_SCHEMA_VERSION,
        sqlite_version: "fuzzed".to_string(),
        materializer_version: AXPD_MATERIALIZER_VERSION.to_string(),
        configuration_digest,
        anchors,
        logical_digest: zero_blob,
        exact_image_digest,
        materialization_id: materialization_id.clone(),
        byte_len: u64::try_from(data.len()).unwrap_or(u64::MAX),
    };

    if std::fs::create_dir_all(store.root().join(AXPD_MATERIALIZATIONS_DIR)).is_err() {
        return;
    }
    if std::fs::write(store.axpd_image_path(&materialization_id), data).is_err() {
        return;
    }
    let Ok(receipt_bytes) = serde_json::to_vec(&receipt) else {
        return;
    };
    if std::fs::write(store.axpd_receipt_path(&materialization_id), receipt_bytes).is_err() {
        return;
    }
    let _ = store.open_axpd(&materialization_id, &AxpdLimits::default());
});
