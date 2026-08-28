#![no_main]

use axiograph_pathdb::{CertificateV2, CertificateV3};
use libfuzzer_sys::fuzz_target;

const MAX_CERTIFICATE_BYTES: usize = 8 * 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    let _: Result<CertificateV2, _> =
        axiograph_security::parse_json_bounded(data, MAX_CERTIFICATE_BYTES, "fuzzed CertificateV2");
    let _: Result<CertificateV3, _> =
        axiograph_security::parse_json_bounded(data, MAX_CERTIFICATE_BYTES, "fuzzed CertificateV3");
});
